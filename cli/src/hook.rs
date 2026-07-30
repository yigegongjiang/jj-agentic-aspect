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
    ENTRY, MAX_STATUS_BODY_LEN, VERSION,
};
use serde_json::{json, Map, Value};

// Mirror the worker's bounds (worker/src/index.ts).
const MAX_SESSION_ID_LEN: usize = 128;
const MAX_EVENT_LEN: usize = 64;
const MAX_SOURCE_LEN: usize = 64;
const SESSION_LIST_LIMIT_DEFAULT: u32 = 50;

/// Default event source preserves the original Claude Code-only CLI contract.
const DEFAULT_SOURCE: &str = "claude-code";

// Upload budget in UTF-8 bytes (what D1 actually stores), kept under the
// worker's MAX_STATUS_BODY_LEN — which is checked in UTF-16 units, and UTF-16
// length is never larger than the UTF-8 byte count, so this bound satisfies
// both. Sized so whole tool outputs and long prompts land verbatim.
const BODY_BUDGET: usize = 1_400_000;
// Per-string truncation passes, tried in order until the payload fits
// BODY_BUDGET. The first pass effectively means "no truncation" for anything a
// real hook emits; the later ones only exist so a pathological payload still
// gets recorded instead of being dropped.
const STR_LIMITS: [usize; 6] = [1_300_000, 400_000, 120_000, 24_000, 2_048, 256];
// Effectively no array cap — the byte budget above is the real bound.
const MAX_ARRAY_ITEMS: usize = 100_000;
// Hard deadline per upload attempt; a hook must never stall the session.
const HOOK_TIMEOUT: Duration = Duration::from_secs(10);

fn usage(key: &str) -> String {
    match key {
        "help" => "jj-agentic-aspect hook --help".into(),
        "hook.ingest" => "jj-agentic-aspect hook [--source <s>]   (hook JSON on stdin)".into(),
        "hook.sessions" => format!(
            "jj-agentic-aspect hook sessions <project> [--limit N]   (default {SESSION_LIST_LIMIT_DEFAULT}, 0 = 全部)"
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
/// generous pass down, then fall back to a flat skeleton of top-level keys
/// (nested values become "…[dropped: …]" placeholders so no field vanishes
/// silently), and finally to the event's identity alone. Every path returns
/// valid JSON within BODY_BUDGET — an over-budget body would be rejected by the
/// worker and the event lost.
fn fit_body(payload: &Value) -> String {
    for max_str in STR_LIMITS {
        let mut candidate = payload.clone();
        shrink(&mut candidate, max_str);
        let s = serde_json::to_string(&candidate).unwrap_or_default();
        if !s.is_empty() && s.len() <= BODY_BUDGET {
            return s;
        }
    }
    // Share the budget across however many top-level keys there are, so a
    // payload with hundreds of fields keeps all of its keys (with short value
    // prefixes) instead of collapsing to nothing.
    let key_count = payload.as_object().map(Map::len).unwrap_or(0).max(1);
    let per_key = (BODY_BUDGET / (key_count * 2)).clamp(16, 256);
    let mut skeleton = Map::new();
    if let Some(obj) = payload.as_object() {
        for (k, val) in obj {
            match val {
                Value::String(s) => {
                    skeleton.insert(k.clone(), Value::String(clip(s, per_key)));
                }
                Value::Number(_) | Value::Bool(_) | Value::Null => {
                    skeleton.insert(k.clone(), val.clone());
                }
                // Nested values can't be kept at this size, but the key must
                // still show up — "the field existed and was dropped" is
                // information the dashboard should not lose.
                Value::Array(items) => {
                    skeleton.insert(
                        k.clone(),
                        Value::String(format!("…[dropped: array of {} items]", items.len())),
                    );
                }
                Value::Object(map) => {
                    skeleton.insert(
                        k.clone(),
                        Value::String(format!(
                            "…[dropped: object with keys {}]",
                            clip(&map.keys().cloned().collect::<Vec<_>>().join(","), per_key)
                        )),
                    );
                }
            }
        }
    }
    skeleton.insert("_truncated".into(), Value::Bool(true));
    let s = serde_json::to_string(&Value::Object(skeleton)).unwrap_or_default();
    if !s.is_empty() && s.len() <= BODY_BUDGET {
        return s;
    }
    // Even the skeleton can overflow (hundreds of top-level keys). Keep only
    // the event's identity plus a count of what was dropped: an oversized body
    // would be rejected by the worker, which would lose the event entirely.
    let obj = payload.as_object();
    let identity = json!({
        "hook_event_name": payload.get("hook_event_name").cloned().unwrap_or(Value::Null),
        "session_id": payload.get("session_id").cloned().unwrap_or(Value::Null),
        "cwd": payload.get("cwd").cloned().unwrap_or(Value::Null),
        "_truncated": true,
        "_dropped_keys": obj.map(Map::len).unwrap_or(0),
        // Values are gone at this point; the field names still say what the
        // event carried.
        "_dropped_key_names": clip(
            &obj.map(|o| o.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default(),
            8000,
        ),
    });
    serde_json::to_string(&identity).unwrap_or_else(|_| "{\"_truncated\":true}".into())
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

/// Codex `/goal <text>` sets the thread objective instead of submitting a user
/// turn: no UserPromptSubmit hook fires (Codex 0.146 has no goal-related hook
/// event at all), so a session started that way records no human ask and the
/// dashboard shows an empty title. The objective *is* written to the rollout
/// transcript as a `thread_goal_updated` event, so at turn end we read it back
/// and report it as a synthetic `ThreadGoal` event. The worker dedupes repeats
/// (same session + same body), so re-reporting every turn is free and a mid-
/// session `/goal` change still lands as a new event.
const GOAL_TRIGGER_EVENTS: [&str; 2] = ["Stop", "SessionEnd"];
const GOAL_MARKER: &str = "thread_goal_updated";
/// Bounds for rollout-derived metadata — a hook must not stall on a
/// pathological file or a huge sessions tree.
const ROLLOUT_MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const ROLLOUT_MAX_DIRS_SCANNED: usize = 4_000;

/// Locate the rollout transcript for `session_id`. Codex passes
/// `transcript_path` on SessionStart but leaves it null on Stop/SessionEnd, so
/// fall back to searching the sessions tree — rollout files carry the session
/// id in their name (`rollout-<ts>-<session_id>.jsonl`).
fn codex_rollout_path(session_id: &str, payload: &Value) -> Option<std::path::PathBuf> {
    if let Some(p) = payload.get("transcript_path").and_then(Value::as_str) {
        let path = std::path::PathBuf::from(p);
        if !p.is_empty() && path.is_file() {
            return Some(path);
        }
    }
    let home = std::env::var("CODEX_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(|h| std::path::Path::new(&h).join(".codex")))?;
    let mut budget = ROLLOUT_MAX_DIRS_SCANNED;
    find_rollout(&home.join("sessions"), session_id, &mut budget)
}

/// Depth-first search for `*<session_id>*.jsonl`, newest directories first
/// (the tree is year/month/day, so reverse name order == newest first).
fn find_rollout(dir: &std::path::Path, session_id: &str, budget: &mut usize) -> Option<std::path::PathBuf> {
    if *budget == 0 {
        return None;
    }
    *budget -= 1;
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            subdirs.push(path);
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".jsonl") && name.contains(session_id) {
            return Some(path);
        }
    }
    subdirs.sort();
    for sub in subdirs.into_iter().rev() {
        if let Some(hit) = find_rollout(&sub, session_id, budget) {
            return Some(hit);
        }
    }
    None
}

/// Last non-empty objective recorded in the rollout transcript, if any.
fn codex_goal_objective(path: &std::path::Path) -> Option<String> {
    use std::io::BufRead;
    if std::fs::metadata(path).ok()?.len() > ROLLOUT_MAX_FILE_BYTES {
        return None;
    }
    let reader = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let mut objective: Option<String> = None;
    for line in reader.lines().map_while(Result::ok) {
        // Cheap pre-filter: only a handful of lines in a rollout are goals.
        if !line.contains(GOAL_MARKER) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let p = v.get("payload").unwrap_or(&Value::Null);
        if p.get("type").and_then(Value::as_str) != Some(GOAL_MARKER) {
            continue;
        }
        let goal = p.get("goal").unwrap_or(&Value::Null);
        let text = goal
            .get("objective")
            .and_then(Value::as_str)
            .or_else(|| goal.as_str())
            .unwrap_or("")
            .trim();
        if !text.is_empty() {
            objective = Some(text.to_string());
        }
    }
    objective
}

fn report_codex_goal(project: &str, session_id: &str, source: &str, payload: &Value) {
    let Some(path) = codex_rollout_path(session_id, payload) else {
        return;
    };
    let Some(objective) = codex_goal_objective(&path) else {
        return;
    };
    let body = fit_body(&json!({
        "hook_event_name": "ThreadGoal",
        "session_id": session_id,
        "cwd": payload.get("cwd").cloned().unwrap_or(Value::Null),
        "objective": objective,
        // Not a real hook event — say where it came from so the timeline never
        // implies Codex emitted it.
        "_derived_from": format!("{GOAL_MARKER} in {}", path.to_string_lossy()),
    }));
    upload_event(project, session_id, "ThreadGoal", source, &body);
}

const TOKEN_USAGE_KEYS: [&str; 6] = [
    "input_tokens",
    "cached_input_tokens",
    "cache_write_input_tokens",
    "output_tokens",
    "reasoning_output_tokens",
    "total_tokens",
];

fn token_usage(v: &Value) -> Option<Value> {
    let obj = v.as_object()?;
    let mut kept = Map::new();
    for key in TOKEN_USAGE_KEYS {
        if let Some(n) = obj.get(key).and_then(Value::as_u64) {
            kept.insert(key.into(), Value::from(n));
        }
    }
    (!kept.is_empty()).then_some(Value::Object(kept))
}

fn token_usage_diff(end: &Value, start: Option<&Value>) -> Value {
    let mut diff = Map::new();
    for key in TOKEN_USAGE_KEYS {
        let end_n = end.get(key).and_then(Value::as_u64).unwrap_or(0);
        let start_n = start
            .and_then(|v| v.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        diff.insert(key.into(), Value::from(end_n.saturating_sub(start_n)));
    }
    Value::Object(diff)
}

fn keep_str(dst: &mut Map<String, Value>, key: &str, src: Option<&Value>) {
    if let Some(s) = src.and_then(Value::as_str).filter(|s| !s.is_empty()) {
        dst.insert(key.into(), Value::String(s.to_string()));
    }
}

/// Best-effort Codex turn observability. Official hooks expose tools and the
/// final response but no commentary/token event. The rollout format is
/// explicitly unstable, so this parser whitelists a tiny schema and returns
/// None on drift; the original hook event remains unaffected.
fn codex_turn_summary_from_reader<R: std::io::BufRead>(
    reader: R,
    session_id: &str,
    turn_id: &str,
    cwd: &Value,
    final_message: Option<&str>,
) -> Option<Value> {
    let mut matched_turn = false;
    let mut in_turn = false;
    let mut last_total: Option<Value> = None;
    let mut turn_baseline: Option<Value> = None;
    let mut turn_total: Option<Value> = None;
    let mut last_usage: Option<Value> = None;
    let mut model_context_window: Option<u64> = None;
    let mut progress: Vec<Value> = Vec::new();
    let mut summary = Map::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let row_type = row.get("type").and_then(Value::as_str);
        let payload = row.get("payload").unwrap_or(&Value::Null);

        if row_type == Some("session_meta") {
            let sid = payload
                .get("session_id")
                .or_else(|| payload.get("id"))
                .and_then(Value::as_str);
            if sid == Some(session_id) {
                keep_str(&mut summary, "codex_cli_version", payload.get("cli_version"));
                keep_str(&mut summary, "originator", payload.get("originator"));
            }
            continue;
        }

        if row_type == Some("turn_context") {
            in_turn = payload.get("turn_id").and_then(Value::as_str) == Some(turn_id);
            if in_turn {
                if !matched_turn {
                    turn_baseline = last_total.clone();
                }
                matched_turn = true;
                keep_str(&mut summary, "model", payload.get("model"));
                keep_str(&mut summary, "effort", payload.get("effort"));
                keep_str(&mut summary, "approval_policy", payload.get("approval_policy"));
                keep_str(
                    &mut summary,
                    "collaboration_mode",
                    payload.get("collaboration_mode").and_then(|v| v.get("mode")),
                );
                keep_str(
                    &mut summary,
                    "sandbox_policy",
                    payload.get("sandbox_policy").and_then(|v| v.get("type")),
                );
            }
            continue;
        }

        if row_type != Some("event_msg") {
            continue;
        }
        match payload.get("type").and_then(Value::as_str) {
            Some("token_count") => {
                let info = payload.get("info").unwrap_or(&Value::Null);
                let total = info.get("total_token_usage").and_then(token_usage);
                if let Some(total) = total {
                    last_total = Some(total.clone());
                    if in_turn {
                        turn_total = Some(total);
                    }
                }
                if in_turn {
                    last_usage = info.get("last_token_usage").and_then(token_usage);
                    model_context_window = info.get("model_context_window").and_then(Value::as_u64);
                }
            }
            Some("agent_message") if in_turn => {
                let phase = payload.get("phase").and_then(Value::as_str).unwrap_or("");
                let message = payload
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                // Stop already carries the final answer. Keep only commentary
                // that official Codex hooks otherwise cannot observe.
                if !message.is_empty()
                    && phase != "final_answer"
                    && final_message.map(str::trim) != Some(message)
                {
                    let mut item = Map::new();
                    item.insert("message".into(), Value::String(message.to_string()));
                    if !phase.is_empty() {
                        item.insert("phase".into(), Value::String(phase.to_string()));
                    }
                    keep_str(&mut item, "timestamp", row.get("timestamp"));
                    progress.push(Value::Object(item));
                }
            }
            _ => {}
        }
    }

    if !matched_turn {
        return None;
    }
    summary.insert("hook_event_name".into(), Value::String("CodexTurnSummary".into()));
    summary.insert("session_id".into(), Value::String(session_id.to_string()));
    summary.insert("turn_id".into(), Value::String(turn_id.to_string()));
    summary.insert("cwd".into(), cwd.clone());
    if !progress.is_empty() {
        summary.insert("progress".into(), Value::Array(progress));
    }
    if let Some(total) = turn_total {
        summary.insert(
            "turn_token_usage".into(),
            token_usage_diff(&total, turn_baseline.as_ref()),
        );
        summary.insert("session_token_usage".into(), total);
    }
    if let Some(usage) = last_usage {
        if let Some(n) = usage.get("input_tokens").and_then(Value::as_u64) {
            summary.insert("context_used_tokens".into(), Value::from(n));
        }
        summary.insert("last_token_usage".into(), usage);
    }
    if let Some(n) = model_context_window {
        summary.insert("model_context_window".into(), Value::from(n));
    }
    summary.insert(
        "_derived_from".into(),
        Value::String("Codex rollout transcript (best effort; internal format)".into()),
    );
    Some(Value::Object(summary))
}

fn report_codex_turn_summary(
    project: &str,
    session_id: &str,
    source: &str,
    payload: &Value,
) {
    let Some(turn_id) = payload.get("turn_id").and_then(Value::as_str) else {
        return;
    };
    let Some(path) = codex_rollout_path(session_id, payload) else {
        return;
    };
    if std::fs::metadata(&path).ok().map(|m| m.len()) > Some(ROLLOUT_MAX_FILE_BYTES) {
        return;
    }
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let reader = std::io::BufReader::new(file);
    let cwd = payload.get("cwd").unwrap_or(&Value::Null);
    let final_message = payload.get("last_assistant_message").and_then(Value::as_str);
    let Some(summary) =
        codex_turn_summary_from_reader(reader, session_id, turn_id, cwd, final_message)
    else {
        return;
    };
    let body = fit_body(&summary);
    upload_event(project, session_id, "CodexTurnSummary", source, &body);
}

/// The ingest entry point. Every failure path is a silent `return` — exit 0,
/// no stdout — because this runs inside someone's live agent session.
fn run_ingest(source: &str) {
    let raw = read_stdin();
    if raw.trim().is_empty() {
        return;
    }
    // Unparsable stdin still gets reported, wrapped in a synthetic event: a
    // silent drop would make the occurrence invisible on the dashboard, which
    // is exactly when you need to see it. The raw text rides along as a field,
    // so fit_body clips it like any other string.
    let payload: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => json!({
            "hook_event_name": "UnparsedPayload",
            "_parse_error": e.to_string(),
            "_unparsed": raw,
        }),
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
    // A basename is essentially always derivable; when it isn't, park the event
    // under "unknown" rather than dropping it.
    let project = match project_name(&payload) {
        Some(p) if len16(&p) <= 128 => p,
        _ => "unknown".to_string(),
    };
    let body = fit_body(&payload);
    debug_assert!(len16(&body) <= MAX_STATUS_BODY_LEN);
    // Native hook data is the durable contract and always uploads first.
    // Best-effort rollout enrichment must never delay it behind extra work.
    upload_event(&project, &session_id, &event, source, &body);

    // Rollout drift only omits derived metadata; the original hook event above
    // remains intact. Worker/Web state inference ignores these derived tails.
    if source.contains("codex") {
        if event == "Stop" {
            report_codex_turn_summary(&project, &session_id, source, &payload);
        }
        if GOAL_TRIGGER_EVENTS.contains(&event.as_str()) {
            report_codex_goal(&project, &session_id, source, &payload);
        }
    }
}

/// POST one event. One retry: a transient network blip shouldn't cost an event.
/// Still capped by HOOK_TIMEOUT per attempt and still silent on failure — a
/// hook must never disturb the hosting session.
fn upload_event(project: &str, session_id: &str, event: &str, source: &str, body: &str) {
    let path = format!("/projects/{}/statuses", encode_uri_component(project));
    let upload = json!({ "session_id": session_id, "event": event, "source": source, "body": body });
    if try_api("POST", &path, Some(upload.clone()), Some(HOOK_TIMEOUT)).is_err() {
        let _ = try_api("POST", &path, Some(upload), Some(HOOK_TIMEOUT));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn codex_turn_summary_keeps_only_target_turn_observability() {
        let rows = [
            json!({"type":"session_meta","payload":{
                "id":"session-1","cli_version":"0.146.0","originator":"codex-tui"
            }}),
            json!({"type":"event_msg","payload":{"type":"token_count","info":{
                "total_token_usage":{"input_tokens":80,"cached_input_tokens":20,"output_tokens":20,"total_tokens":100}
            }}}),
            json!({"type":"turn_context","payload":{
                "turn_id":"turn-2","model":"gpt-test","effort":"xhigh",
                "approval_policy":"never","sandbox_policy":{"type":"danger-full-access"},
                "collaboration_mode":{"mode":"default"}
            }}),
            json!({"timestamp":"2026-07-30T00:00:01Z","type":"event_msg","payload":{
                "type":"agent_message","phase":"commentary","message":"checking production"
            }}),
            json!({"type":"event_msg","payload":{"type":"token_count","info":{
                "total_token_usage":{"input_tokens":200,"cached_input_tokens":100,"output_tokens":50,"total_tokens":250},
                "last_token_usage":{"input_tokens":80,"cached_input_tokens":60,"output_tokens":10,"total_tokens":90},
                "model_context_window":1000
            }}}),
            json!({"type":"event_msg","payload":{
                "type":"agent_message","phase":"final_answer","message":"done"
            }}),
            json!({"type":"turn_context","payload":{"turn_id":"turn-3"}}),
            json!({"type":"event_msg","payload":{
                "type":"agent_message","phase":"commentary","message":"wrong turn"
            }}),
        ];
        let input = rows
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let summary = codex_turn_summary_from_reader(
            Cursor::new(input),
            "session-1",
            "turn-2",
            &Value::String("/repo".into()),
            Some("done"),
        )
        .expect("summary");

        assert_eq!(summary["codex_cli_version"], "0.146.0");
        assert_eq!(summary["effort"], "xhigh");
        assert_eq!(summary["turn_token_usage"]["total_tokens"], 150);
        assert_eq!(summary["turn_token_usage"]["input_tokens"], 120);
        assert_eq!(summary["session_token_usage"]["total_tokens"], 250);
        assert_eq!(summary["context_used_tokens"], 80);
        assert_eq!(summary["model_context_window"], 1000);
        assert_eq!(summary["progress"].as_array().map(Vec::len), Some(1));
        assert_eq!(summary["progress"][0]["message"], "checking production");
    }
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
                Ok(n) => limit = Some(n),
                _ => fail_usage(
                    "hook.sessions",
                    "--limit must be a non-negative integer (0 = all)",
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
        .replace("{SESSION_LIST_LIMIT_MAX}", "0 = 全部");
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
- 默认不截断: 单条 body 额度 1.4 MB (D1 单值上限内), 整段工具输出 / 长 prompt 原样入库.
- 仅极端超额才逐档压缩 (1.3M/400K/120K/24K/2048/256 字符, 带 truncated 标记); 数组不设条数上限.
- 再超额则退化为顶层字段骨架 (标量原样, 对象/数组留 key + 占位), 最后退化为事件标识 + 全部字段名, 不静默消失.
- stdin 非 JSON 也上报 (event=UnparsedPayload, body 带原文), 不静默丢弃; 上传失败自动重试 1 次.
- 永远 exit 0, 不写 stdout, 上报限时 10s: 任何失败静默丢弃, 绝不干扰宿主 session.
- 未知 flag 静默忽略 (hook 行配置错误也不得影响宿主).
- codex 专项: `/goal` 不触发任何 hook (codex 0.146 无 goal 事件). Stop/SessionEnd 时回读 rollout transcript
  的 thread_goal_updated, 合成 event=ThreadGoal 上报 (body.objective + _derived_from); 同 body 重复由 worker 折叠.
  session 摘要 first_prompt 在无 UserPromptSubmit 时回退到该 objective.
- codex Stop 时防御性回读当前 turn: 合成 CodexTurnSummary (commentary + effort + token/context + CLI 元数据);
  rollout 为非稳定格式, 解析失败仅缺该摘要, 原始 hook 不受影响.

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
jj-agentic-aspect hook sessions <project> [--limit N]   (default {SESSION_LIST_LIMIT_DEFAULT}, {SESSION_LIST_LIMIT_MAX}, by last_at DESC)
  -> [{session_id, source, events_count, turns_count, tools_count, errors_count, first_at, last_at, first_prompt, last_event}]
  err: 404
jj-agentic-aspect hook ls <project> <session_id>
  -> [{id, project_id, session_id, event, source, body, created_at}]   (时间序)
  err: 404
jj-agentic-aspect hook rm <project> <session_id>
  err: 404
"#;
