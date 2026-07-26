// jj-status binary: Claude Code hook session-event logging.
//
// `jj-status hook` is wired into Claude Code's hooks (settings.json) and runs
// on every hook event. It reads the event JSON from stdin, shrinks oversized
// fields, and uploads to the worker. It is a pure observer: it always exits 0
// and never writes to stdout, so a broken config / network / payload can never
// block or pollute the hosting session.
use std::time::Duration;

use jj_plan_cli::{
    api, die, die_usage, encode_uri_component, print, read_stdin, run_installer, try_api,
    validate_project, MAX_BODY_LEN, VERSION,
};
use serde_json::{json, Map, Value};

const ENTRY: &str = "jj-status";

// Mirror the worker's bounds (worker/src/index.ts).
const MAX_SESSION_ID_LEN: usize = 128;
const MAX_EVENT_LEN: usize = 64;
const SESSION_LIST_LIMIT_DEFAULT: u32 = 50;
const SESSION_LIST_LIMIT_MAX: u32 = 200;

// Upload budget (UTF-16 units), safely under the worker's MAX_BODY_LEN after
// JSON escaping overhead.
const BODY_BUDGET: usize = 60000;
// Per-string truncation passes: generous first, aggressive fallback.
const STR_LIMITS: [usize; 2] = [2048, 256];
// Arrays beyond this length carry no reading value on a dashboard timeline.
const MAX_ARRAY_ITEMS: usize = 100;
// Hard deadline for the upload; a hook must never stall the session.
const HOOK_TIMEOUT: Duration = Duration::from_secs(10);

fn usage(key: &str) -> String {
    match key {
        "help" => "jj-status --help".into(),
        "version" => "jj-status --version".into(),
        "update" => "jj-status update | upgrade".into(),
        "uninstall" => "jj-status uninstall".into(),
        "status.hook" => "jj-status hook   (hook JSON on stdin)".into(),
        "status.sessions" => format!(
            "jj-status sessions <project> [--limit N]   (default {SESSION_LIST_LIMIT_DEFAULT}, max {SESSION_LIST_LIMIT_MAX})"
        ),
        "status.ls" => "jj-status ls <project> <session_id>".into(),
        "status.rm" => "jj-status rm <project> <session_id>".into(),
        _ => "jj-status --help".into(),
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

/// Serialize the payload within BODY_BUDGET: try the generous string limit,
/// then the aggressive one, then fall back to a flat skeleton of top-level
/// scalars (still valid JSON, still carries the event identity).
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

/// Project = basename of the session's cwd (from the hook JSON), falling back
/// to $CLAUDE_PROJECT_DIR, then the hook process cwd.
fn project_name(payload: &Value) -> Option<String> {
    let from_json = payload.get("cwd").and_then(Value::as_str).map(str::to_string);
    let from_env = std::env::var("CLAUDE_PROJECT_DIR").ok().filter(|s| !s.is_empty());
    let from_proc = std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    for dir in [from_json, from_env, from_proc].into_iter().flatten() {
        if let Some(base) = std::path::Path::new(&dir).file_name() {
            let name = base.to_string_lossy().into_owned();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// The hook entry point. Every failure path is a silent `return` — exit 0, no
/// stdout — because this runs inside someone's live Claude Code session.
fn run_hook() {
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
    let upload = json!({ "session_id": session_id, "event": event, "body": body });
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
                fail_usage("status.sessions", "duplicate --limit");
            }
            i += 1;
            let v = match args.get(i) {
                Some(v) if !v.starts_with("--") => v,
                _ => fail_usage("status.sessions", "missing <N> after --limit"),
            };
            match v.parse::<u32>() {
                Ok(n) if (1..=SESSION_LIST_LIMIT_MAX).contains(&n) => limit = Some(n),
                _ => fail_usage(
                    "status.sessions",
                    &format!("--limit must be integer in 1..{SESSION_LIST_LIMIT_MAX}"),
                ),
            }
        } else if arg.starts_with("--") {
            fail_usage("status.sessions", &format!("unknown option {arg}"));
        } else if project.is_none() {
            project = Some(arg.clone());
        } else {
            fail_usage("status.sessions", &format!("unexpected argument {arg}"));
        }
        i += 1;
    }
    let project = project.unwrap_or_else(|| fail_usage("status.sessions", "missing <project>"));
    validate_project(ENTRY, &project);
    (project, limit)
}

fn run(verb: &str, rest: &[String]) {
    match verb {
        "hook" => {
            // Extra args are ignored, not fatal: a misconfigured hook line must
            // still not break the session.
            run_hook();
        }
        "sessions" => {
            let (project, limit) = parse_sessions_args(rest);
            let q = limit.map(|n| format!("?limit={n}")).unwrap_or_default();
            let path = format!("/projects/{}/sessions{q}", encode_uri_component(&project));
            print(api(ENTRY, "GET", &path, None).as_ref());
        }
        "ls" => {
            let (project, sid) = parse_project_session("status.ls", rest);
            let path = format!(
                "/projects/{}/sessions/{}/statuses",
                encode_uri_component(&project),
                encode_uri_component(&sid)
            );
            print(api(ENTRY, "GET", &path, None).as_ref());
        }
        "rm" => {
            let (project, sid) = parse_project_session("status.rm", rest);
            let path = format!(
                "/projects/{}/sessions/{}",
                encode_uri_component(&project),
                encode_uri_component(&sid)
            );
            api(ENTRY, "DELETE", &path, None);
        }
        _ => fail(&format!("unknown command '{verb}'; usage: {}", usage("help"))),
    }
}

fn print_help() {
    let help = HELP
        .replace("{VERSION}", VERSION)
        .replace("{SESSION_LIST_LIMIT_DEFAULT}", &SESSION_LIST_LIMIT_DEFAULT.to_string())
        .replace("{SESSION_LIST_LIMIT_MAX}", &SESSION_LIST_LIMIT_MAX.to_string());
    print!("{help}");
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    let head = argv.first().map(String::as_str);
    if argv.is_empty() || head == Some("help") || head == Some("-h") || head == Some("--help") {
        if argv.len() > 1 {
            fail_usage("help", &format!("unexpected argument {}", argv[1]));
        }
        print_help();
        return;
    }
    if head == Some("-v") || head == Some("--version") {
        if argv.len() > 1 {
            fail_usage("version", &format!("unexpected argument {}", argv[1]));
        }
        println!("{VERSION}");
        return;
    }
    if head == Some("update") || head == Some("upgrade") {
        if argv.len() > 1 {
            fail_usage("update", &format!("unexpected argument {}", argv[1]));
        }
        run_installer(ENTRY, &[]);
        return;
    }
    if head == Some("uninstall") {
        if argv.len() > 1 {
            fail_usage("uninstall", &format!("unexpected argument {}", argv[1]));
        }
        run_installer(ENTRY, &["--uninstall"]);
        return;
    }

    let verb = &argv[0];
    let rest = argv.get(1..).unwrap_or(&[]);
    run(verb, rest);
}

const HELP: &str = r#"jj-status {VERSION}

# TLDR
jj-status: 落盘 Claude Code hook 的 session 运行事件. 层级 project -> session -> event.
`jj-status hook` 挂进 Claude Code hooks 后自动上报, 事后经 dashboard 按 session 时间线回看.

  jj-status hook                          # hook 专用: stdin 读事件 JSON, 静默上报, 永远 exit 0
  jj-status sessions <project>            # 该项目的 session 摘要列表

输出: stdout 单行 JSON. 查询/删除见 jj-status --help.

# PURPOSE
记录 Claude Code 每个 session 的运行过程 (提示词/工具调用/停止等 hook 事件), 供 web 端回看.

# MODEL
project (name) -- session (session_id, 来自 Claude Code) -- event (id=ULID, 追加只读)
- project 自动 upsert; project = hook JSON 的 cwd basename (回退 $CLAUDE_PROJECT_DIR / 进程 cwd).
- event 不可修改; 删除只按整个 session.
- project rm 级联删全部 event.

# HOOK 行为
- stdin 读 Claude Code hook JSON, 提取 session_id + hook_event_name, 原始 JSON 作 body 上报.
- 超长字段递归截断 (带 truncated 标记), 超长数组截前 100 项, 保证 body 不超限.
- 永远 exit 0, 不写 stdout, 上报限时 10s: 任何失败静默丢弃, 绝不干扰宿主 session.

# 配置 (~/.claude/settings.json; 事件集可按需增删, 未知事件同样原样落盘)
{
  "hooks": {
    "SessionStart":       [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "UserPromptSubmit":   [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "PostToolUse":        [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "PostToolUseFailure": [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "PermissionDenied":   [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "Notification":       [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "Stop":               [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "SubagentStop":       [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "PreCompact":         [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }],
    "SessionEnd":         [{ "hooks": [{ "type": "command", "command": "jj-status hook" }] }]
  }
}

# COMMANDS

jj-status --help | --version
jj-status update | upgrade | uninstall    仅在用户明确要求时执行 (同时影响 jj-plan/jj-ask)

jj-status hook
  stdin=hook JSON; 无输出, 永远 exit 0.
jj-status sessions <project> [--limit N]   (default {SESSION_LIST_LIMIT_DEFAULT}, max {SESSION_LIST_LIMIT_MAX}, by last_at DESC)
  -> [{session_id, events_count, first_at, last_at, first_prompt}]
  err: 404
jj-status ls <project> <session_id>
  -> [{id, project_id, session_id, event, body, created_at}]   (时间序)
  err: 404
jj-status rm <project> <session_id>
  err: 404
"#;
