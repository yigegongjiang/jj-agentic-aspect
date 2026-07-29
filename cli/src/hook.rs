// `hook` subcommand: agent hook session-event logging.
//
// `jj-agentic-aspect hook --source <s>` is wired into the host agent's hooks
// (e.g. Claude Code settings.json / Codex hooks.json) and runs on every hook
// event. It reads the event JSON from stdin, shrinks oversized fields, and
// uploads to the worker.
// It is a pure observer: it always exits 0 and never writes to stdout, so a
// broken config / network / payload can never block or pollute the hosting
// session. `--source` names the emitting agent and defaults to claude-code.
use std::time::Duration;

use crate::{
    api, die, die_usage, encode_uri_component, print, read_stdin, try_api, validate_project,
    ENTRY, MAX_BODY_LEN, VERSION,
};
use serde_json::{json, Map, Value};

// Mirror the worker's bounds (worker/src/index.ts).
const MAX_SESSION_ID_LEN: usize = 128;
const MAX_EVENT_LEN: usize = 64;
const MAX_SOURCE_LEN: usize = 64;
const SESSION_LIST_LIMIT_DEFAULT: u32 = 50;
const SESSION_LIST_LIMIT_MAX: u32 = 200;

/// Default event source preserves the original Claude Code-only CLI contract.
const DEFAULT_SOURCE: &str = "claude-code";

// Upload budget (UTF-16 units), safely under the worker's MAX_BODY_LEN after
// JSON escaping overhead.
const BODY_BUDGET: usize = 60000;
// Per-string truncation passes, tried in order until the payload fits
// BODY_BUDGET. The first pass lets a single string use nearly the whole budget,
// so ordinary events (prompts, assistant replies, tool responses) reach the
// dashboard verbatim; later passes only kick in for payloads that genuinely
// cannot fit.
const STR_LIMITS: [usize; 5] = [58000, 24000, 8192, 2048, 256];
// Arrays beyond this length carry no reading value on a dashboard timeline.
const MAX_ARRAY_ITEMS: usize = 1000;
// Hard deadline for the upload; a hook must never stall the session.
const HOOK_TIMEOUT: Duration = Duration::from_secs(10);

fn usage(key: &str) -> String {
    match key {
        "help" => "jj-agentic-aspect hook --help".into(),
        "hook.ingest" => "jj-agentic-aspect hook [--source <s>]   (hook JSON on stdin)".into(),
        "hook.sessions" => format!(
            "jj-agentic-aspect hook sessions <project> [--limit N]   (default {SESSION_LIST_LIMIT_DEFAULT}, max {SESSION_LIST_LIMIT_MAX})"
        ),
        "hook.ls" => "jj-agentic-aspect hook ls <project> <session_id>".into(),
        "hook.rm" => "jj-agentic-aspect hook rm <project> <session_id>".into(),
        _ => "jj-agentic-aspect hook --help".into(),
    }
}

fn fail(msg: &str) -> ! {
    die(ENTRY, msg)
}
fn fail_usage(key: &str, reason: &str) -> ! {
    die_usage(ENTRY, &usage(key), reason)
}

fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Truncate to at most `max` UTF-16 units on a char boundary, appending a
/// marker with the original size so the dashboard can say what was cut.
fn clip(s: &str, max: usize) -> String {
    if len16(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let w = ch.len_utf16();
        if used + w > max {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push_str(&format!("…[truncated, {} chars total]", len16(s)));
    out
}

/// Recursively shrink a hook payload in place: long strings clipped, long
/// arrays capped. Structure and keys are preserved so the dashboard can still
/// render "the important parts" of every event.
fn shrink(v: &mut Value, max_str: usize) {
    match v {
        Value::String(s) => {
            if len16(s) > max_str {
                *v = Value::String(clip(s, max_str));
            }
        }
        Value::Array(items) => {
            if items.len() > MAX_ARRAY_ITEMS {
                let dropped = items.len() - MAX_ARRAY_ITEMS;
                items.truncate(MAX_ARRAY_ITEMS);
                items.push(Value::String(format!("…[{dropped} more items truncated]")));
            }
            for item in items.iter_mut() {
                shrink(item, max_str);
            }
        }
        Value::Object(map) => {
            for (_, item) in map.iter_mut() {
                shrink(item, max_str);
            }
        }
        _ => {}
    }
}

/// Serialize the payload within BODY_BUDGET: walk STR_LIMITS from the most
/// generous pass down, then fall back to a flat skeleton of top-level scalars
/// (still valid JSON, still carries the event identity).
fn fit_body(payload: &Value) -> String {
    for max_str in STR_LIMITS {
        let mut candidate = payload.clone();
        shrink(&mut candidate, max_str);
        let s = serde_json::to_string(&candidate).unwrap_or_default();
        if !s.is_empty() && len16(&s) <= BODY_BUDGET {
            return s;
        }
    }
    let mut skeleton = Map::new();
    if let Some(obj) = payload.as_object() {
        for (k, val) in obj {
            match val {
                Value::String(s) => {
                    skeleton.insert(k.clone(), Value::String(clip(s, 256)));
                }
                Value::Number(_) | Value::Bool(_) | Value::Null => {
                    skeleton.insert(k.clone(), val.clone());
                }
                _ => {}
            }
        }
    }
    skeleton.insert("_truncated".into(), Value::Bool(true));
    serde_json::to_string(&Value::Object(skeleton)).unwrap_or_else(|_| "{\"_truncated\":true}".into())
}

/// Project = basename of $CLAUDE_PROJECT_DIR (the session's start directory,
/// stable for the whole session), falling back to the hook JSON cwd, then the
/// hook process cwd. The JSON cwd follows the session shell — an agent that
/// cd's into a subdirectory would otherwise split the session into a bogus
/// project named after that subdirectory.
fn project_name(payload: &Value) -> Option<String> {
    let from_env = std::env::var("CLAUDE_PROJECT_DIR").ok().filter(|s| !s.is_empty());
    let from_json = payload.get("cwd").and_then(Value::as_str).map(str::to_string);
    let from_proc = std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    for dir in [from_env, from_json, from_proc].into_iter().flatten() {
        if let Some(base) = std::path::Path::new(&dir).file_name() {
            let name = base.to_string_lossy().into_owned();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Parse the ingest arg list. Only `--source <s>` / `--source=<s>` is
/// meaningful; anything else is silently ignored — a misconfigured hook line
/// must never break the hosting session. Empty / missing value falls back to
/// the default.
fn parse_source(args: &[String]) -> String {
    let mut source: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--source" {
            if let Some(v) = args.get(i + 1) {
                if !v.is_empty() && !v.starts_with("--") {
                    source = Some(v.clone());
                    i += 1;
                }
            }
        } else if let Some(v) = arg.strip_prefix("--source=") {
            if !v.is_empty() {
                source = Some(v.to_string());
            }
        }
        i += 1;
    }
    clip(&source.unwrap_or_else(|| DEFAULT_SOURCE.to_string()), MAX_SOURCE_LEN)
}

/// The ingest entry point. Every failure path is a silent `return` — exit 0,
/// no stdout — because this runs inside someone's live agent session.
fn run_ingest(source: &str) {
    let raw = read_stdin();
    if raw.trim().is_empty() {
        return;
    }
    let payload: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return,
    };
    let session_id = clip(
        payload.get("session_id").and_then(Value::as_str).unwrap_or("unknown"),
        MAX_SESSION_ID_LEN,
    );
    let event = clip(
        payload
            .get("hook_event_name")
            .and_then(Value::as_str)
            .unwrap_or("Unknown"),
        MAX_EVENT_LEN,
    );
    let project = match project_name(&payload) {
        Some(p) if len16(&p) <= 128 => p,
        _ => return,
    };
    let body = fit_body(&payload);
    debug_assert!(len16(&body) <= MAX_BODY_LEN);

    let path = format!("/projects/{}/statuses", encode_uri_component(&project));
    let upload = json!({ "session_id": session_id, "event": event, "source": source, "body": body });
    // Ignore the outcome entirely: losing one event beats disturbing a session.
    let _ = try_api("POST", &path, Some(upload), Some(HOOK_TIMEOUT));
}

fn parse_project_session(key: &str, args: &[String]) -> (String, String) {
    let mut pos: Vec<String> = Vec::new();
    for arg in args {
        if arg.starts_with("--") {
            fail_usage(key, &format!("unknown option {arg}"));
        }
        pos.push(arg.clone());
    }
    match pos.len() {
        0 => fail_usage(key, "missing <project>"),
        1 => fail_usage(key, "missing <session_id>"),
        2 => {
            validate_project(ENTRY, &pos[0]);
            (pos[0].clone(), pos[1].clone())
        }
        _ => fail_usage(key, &format!("unexpected argument {}", pos[2])),
    }
}

fn parse_sessions_args(args: &[String]) -> (String, Option<u32>) {
    let mut project: Option<String> = None;
    let mut limit: Option<u32> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--limit" {
            if limit.is_some() {
                fail_usage("hook.sessions", "duplicate --limit");
            }
            i += 1;
            let v = match args.get(i) {
                Some(v) if !v.starts_with("--") => v,
                _ => fail_usage("hook.sessions", "missing <N> after --limit"),
            };
            match v.parse::<u32>() {
                Ok(n) if (1..=SESSION_LIST_LIMIT_MAX).contains(&n) => limit = Some(n),
                _ => fail_usage(
                    "hook.sessions",
                    &format!("--limit must be integer in 1..{SESSION_LIST_LIMIT_MAX}"),
                ),
            }
        } else if arg.starts_with("--") {
            fail_usage("hook.sessions", &format!("unknown option {arg}"));
        } else if project.is_none() {
            project = Some(arg.clone());
        } else {
            fail_usage("hook.sessions", &format!("unexpected argument {arg}"));
        }
        i += 1;
    }
    let project = project.unwrap_or_else(|| fail_usage("hook.sessions", "missing <project>"));
    validate_project(ENTRY, &project);
    (project, limit)
}

/// Entry from main: argv is everything after `hook`.
/// Bare / flags-only invocation = the ingest path used inside agent hooks
/// (silent, always exit 0); the query verbs stay strict and fail loudly.
pub fn run(argv: &[String]) {
    let head = argv.first().map(String::as_str);
    if head == Some("help") || head == Some("-h") || head == Some("--help") {
        if argv.len() > 1 {
            fail_usage("help", &format!("unexpected argument {}", argv[1]));
        }
        print_help();
        return;
    }
    match head {
        None => run_ingest(DEFAULT_SOURCE),
        Some("sessions") => {
            let (project, limit) = parse_sessions_args(&argv[1..]);
            let q = limit.map(|n| format!("?limit={n}")).unwrap_or_default();
            let path = format!("/projects/{}/sessions{q}", encode_uri_component(&project));
            print(api(ENTRY, "GET", &path, None).as_ref());
        }
        Some("ls") => {
            let (project, sid) = parse_project_session("hook.ls", &argv[1..]);
            let path = format!(
                "/projects/{}/sessions/{}/statuses",
                encode_uri_component(&project),
                encode_uri_component(&sid)
            );
            print(api(ENTRY, "GET", &path, None).as_ref());
        }
        Some("rm") => {
            let (project, sid) = parse_project_session("hook.rm", &argv[1..]);
            let path = format!(
                "/projects/{}/sessions/{}",
                encode_uri_component(&project),
                encode_uri_component(&sid)
            );
            api(ENTRY, "DELETE", &path, None);
        }
        Some(s) if s.starts_with('-') => run_ingest(&parse_source(argv)),
        Some(other) => fail(&format!("unknown command 'hook {other}'; usage: {}", usage("help"))),
    }
}

pub fn print_help() {
    let help = HELP
        .replace("{VERSION}", VERSION)
        .replace("{SESSION_LIST_LIMIT_DEFAULT}", &SESSION_LIST_LIMIT_DEFAULT.to_string())
        .replace("{SESSION_LIST_LIMIT_MAX}", &SESSION_LIST_LIMIT_MAX.to_string());
    print!("{help}");
}

const HELP: &str = r#"jj-agentic-aspect hook {VERSION}

# TLDR
hook: 落盘 agent hook 的 session 运行事件. 层级 project -> session -> event; 每条事件带 source (哪个 agent 上报).
`--source claude-code` / `--source codex` 挂进对应 agent hooks 后自动上报, 事后经 dashboard 按 session 时间线回看.

  jj-agentic-aspect hook [--source <s>]           # hook 专用: stdin 读事件 JSON, 静默上报, 永远 exit 0; source 默认 claude-code
  jj-agentic-aspect hook sessions <project>       # 该项目的 session 摘要列表

输出: stdout 单行 JSON. 查询/删除见 jj-agentic-aspect hook --help.

# PURPOSE
记录 agent 每个 session 的运行过程 (提示词/工具调用/停止等 hook 事件), 供 web 端回看.

# MODEL
project (name) -- session (session_id, 来自宿主 agent) -- event (id=ULID, 追加只读)
- project 自动 upsert; project = $CLAUDE_PROJECT_DIR basename (回退 hook JSON cwd / 进程 cwd; session 内 cd 不改归属).
- 同一 session_id 的后续事件固定归入其首个事件所在 project (worker 端 session affinity), 不随 cwd 漂移.
- event 带 source (上报方: claude-code / codex / ..., 默认 claude-code), 不可修改; 删除只按整个 session.
- project rm 级联删全部 event.

# HOOK 行为
- stdin 读 hook JSON, 提取 session_id + hook_event_name, 原始 JSON 作 body 上报.
- 超长字段递归截断 (带 truncated 标记), 超长数组截前 100 项, 保证 body 不超限.
- 永远 exit 0, 不写 stdout, 上报限时 10s: 任何失败静默丢弃, 绝不干扰宿主 session.
- 未知 flag 静默忽略 (hook 行配置错误也不得影响宿主).

# 配置: Claude Code (~/.claude/settings.json; 全事件集 = Claude Code 2.1.220 实测支持; 事件集可按需增删, 未知事件同样原样落盘)
# MessageDisplay = assistant 中间进度文本 (payload: message_id/index/final/delta, delta 按 message_id+index 拼接)
{
  "hooks": {
    "SessionStart":       [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "SessionEnd":         [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "Setup":              [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "UserPromptSubmit":   [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "UserPromptExpansion":[{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "MessageDisplay":     [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "PreToolUse":         [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "PostToolUse":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "PostToolUseFailure": [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "PostToolBatch":      [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "PermissionRequest":  [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "PermissionDenied":   [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "Notification":       [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "SubagentStart":      [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "SubagentStop":       [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "TaskCreated":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "TaskCompleted":      [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "TeammateIdle":       [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "Stop":               [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "StopFailure":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "PreCompact":         [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "PostCompact":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "InstructionsLoaded": [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "ConfigChange":       [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "CwdChanged":         [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "FileChanged":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "WorktreeCreate":     [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "WorktreeRemove":     [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "Elicitation":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }],
    "ElicitationResult":  [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source claude-code" }] }]
  }
}

# 配置: Codex (~/.codex/hooks.json; 首次启动按提示 review + trust)
{
  "description": "Record Codex sessions in jj-agentic-aspect.",
  "hooks": {
    "SessionStart":      [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "UserPromptSubmit":  [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "PreToolUse":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "PostToolUse":       [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "PermissionRequest": [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "SubagentStart":     [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "SubagentStop":      [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "Stop":              [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "PreCompact":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "PostCompact":       [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex" }] }],
    "SessionEnd":        [{ "hooks": [{ "type": "command", "command": "jj-agentic-aspect hook --source codex", "timeout": 3 }] }]
  }
}

# COMMANDS

jj-agentic-aspect hook [--source <s>]
  stdin=hook JSON; 无输出, 永远 exit 0. source 默认 claude-code, 超 64 字符截断.
jj-agentic-aspect hook sessions <project> [--limit N]   (default {SESSION_LIST_LIMIT_DEFAULT}, max {SESSION_LIST_LIMIT_MAX}, by last_at DESC)
  -> [{session_id, source, events_count, first_at, last_at, first_prompt}]
  err: 404
jj-agentic-aspect hook ls <project> <session_id>
  -> [{id, project_id, session_id, event, source, body, created_at}]   (时间序)
  err: 404
jj-agentic-aspect hook rm <project> <session_id>
  err: 404
"#;
