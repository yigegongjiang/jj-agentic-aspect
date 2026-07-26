//! jj-agentic-aspect: one binary, three subcommands (plan / ask / hook).
//! This crate root holds the shared helpers; each subcommand lives in its own
//! module below and is dispatched from src/main.rs.
//!
//! The CLI is a thin HTTP client over the Cloudflare Worker API. It holds no
//! local state. Auth is a Cloudflare Access service token only — the endpoint
//! sits behind Cloudflare Access, which validates the service-token pair at the
//! edge and injects the JWT the worker trusts. (The old bearer-token mode was
//! dead against the Access-protected endpoint and has been dropped.)

pub mod ask;
pub mod hook;
pub mod plan;

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

// Injected by build.rs from the repo-root VERSION file.
pub const VERSION: &str = env!("JJ_VERSION");

pub const INSTALL_URL: &str = "https://raw.githubusercontent.com/yigegongjiang/jj-agentic-aspect/main/scripts/install.sh";

/// Binary name == error-message prefix for every subcommand.
pub const ENTRY: &str = "jj-agentic-aspect";

pub const SPEC_STATUSES: &[&str] = &["active", "done"];
pub const TASK_STATUSES: &[&str] = &["todo", "doing", "done", "blocked"];
pub const MAX_TITLE_LEN: usize = 200;
pub const MAX_BODY_LEN: usize = 65536;
pub const MAX_PROJECT_NAME_LEN: usize = 128;

pub const ASK_LIMIT_DEFAULT: u32 = 3;
pub const ASK_LIMIT_MAX: u32 = 100;

// ─── error helpers ──────────────────────────────────────────────────────────

/// Write `<entry>: <message>` to stderr (whitespace collapsed, like the old
/// TS `message.replace(/\s+/g,' ').trim()`) and exit 1.
pub fn die(entry: &str, message: &str) -> ! {
    let collapsed = message.split_whitespace().collect::<Vec<_>>().join(" ");
    eprintln!("{entry}: {collapsed}");
    std::process::exit(1);
}

pub fn die_usage(entry: &str, usage: &str, reason: &str) -> ! {
    die(entry, &format!("{reason}; usage: {usage}"));
}

// ─── config ───────────────────────────────────────────────────────────────

/// Config lives at $XDG_CONFIG_HOME/jj-agentic-aspect/config.json (default
/// ~/.config/jj-agentic-aspect/config.json). Legacy paths stay honoured as
/// read-only fallbacks so older installs keep working without a move:
///   - $XDG_CONFIG_HOME/jj-plan/config.json (0.14–0.16, pre jj-agentic-aspect)
///   - $XDG_CONFIG_HOME/jjplan/config.json  (0.12–0.13, pre-rename XDG path)
///   - ~/.jjplan/config.json                (pre-0.12)
pub struct Config {
    pub endpoint: String,
    pub cf_access_client_id: String,
    pub cf_access_client_secret: String,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn config_home() -> PathBuf {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(home()).join(".config"),
    }
}

pub fn config_path() -> PathBuf {
    config_home().join("jj-agentic-aspect").join("config.json")
}

// Canonical path wins; legacy paths are read only when canonical is absent, in
// order (newest first). Returns the canonical path when none exist so error
// messages point users at the path they should create.
fn resolve_config_path() -> PathBuf {
    let canonical = config_path();
    if canonical.exists() {
        return canonical;
    }
    let legacy = [
        config_home().join("jj-plan").join("config.json"),
        config_home().join("jjplan").join("config.json"),
        PathBuf::from(home()).join(".jjplan").join("config.json"),
    ];
    for p in legacy {
        if p.exists() {
            return p;
        }
    }
    canonical
}

/// Fallible config load. The regular CLI path wraps it in `die`; the hook
/// ingest path must never kill the hosting session, so it consumes the Err
/// silently instead.
pub fn try_load_config() -> Result<Config, String> {
    let path = resolve_config_path();
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("unable to read {}: {e}", path.display()))?;
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid JSON in {}: {e}", path.display()))?;

    let str_field = |k: &str| parsed.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());

    let endpoint = str_field("endpoint")
        .ok_or_else(|| format!("{} must contain \"endpoint\"", path.display()))?
        .to_string();
    match (str_field("cf_access_client_id"), str_field("cf_access_client_secret")) {
        (Some(id), Some(secret)) => Ok(Config {
            endpoint,
            cf_access_client_id: id.to_string(),
            cf_access_client_secret: secret.to_string(),
        }),
        _ => Err(format!(
            "{} must contain \"cf_access_client_id\" + \"cf_access_client_secret\"",
            path.display()
        )),
    }
}

// ─── HTTP ────────────────────────────────────────────────────────────────

/// Fallible API call. `body` is sent as JSON when present (POST/PATCH).
/// Returns `Ok(None)` on 204 / empty body, `Ok(Some(json))` otherwise.
/// `timeout` bounds the whole request — the hook path uses a short one so a
/// slow network can never stall a Claude Code session.
pub fn try_api(
    method: &str,
    path: &str,
    body: Option<Value>,
    timeout: Option<std::time::Duration>,
) -> Result<Option<Value>, String> {
    let cfg = try_load_config()?;
    let url = format!("{}{}", cfg.endpoint.trim_end_matches('/'), path);

    let mut req = ureq::request(method, &url)
        .set("CF-Access-Client-Id", &cfg.cf_access_client_id)
        .set("CF-Access-Client-Secret", &cfg.cf_access_client_secret);
    if let Some(t) = timeout {
        req = req.timeout(t);
    }

    let result = match &body {
        Some(b) => {
            let payload = serde_json::to_string(b).expect("serialize request body");
            req.set("content-type", "application/json").send_string(&payload)
        }
        None => req.call(),
    };

    let (status, text) = match result {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.into_string().unwrap_or_default();
            (status, text)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let status_text = resp.status_text().to_string();
            let text = resp.into_string().unwrap_or_default();
            let msg = if text.is_empty() { status_text } else { text };
            return Err(format!("HTTP {code}: {msg}"));
        }
        Err(ureq::Error::Transport(t)) => {
            return Err(format!("network error: {t}"));
        }
    };

    if status == 204 || text.is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => Ok(Some(v)),
        Err(_) => {
            let snippet: String = text.chars().take(200).collect();
            Err(format!("non-JSON response: {snippet}"))
        }
    }
}

/// Perform one API call, exiting via `die` on any failure. The standard path
/// for every interactive command.
pub fn api(entry: &str, method: &str, path: &str, body: Option<Value>) -> Option<Value> {
    try_api(method, path, body, None).unwrap_or_else(|e| die(entry, &e))
}

/// Percent-encode a path segment like JS `encodeURIComponent`: keep the
/// unreserved set (A-Za-z0-9 - _ . ! ~ * ' ( )), percent-encode every other
/// byte of the UTF-8 encoding. Needed so CJK project names / ULIDs land in the
/// URL path the way Hono's param decoder expects.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')');
        if keep {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

// ─── I/O ────────────────────────────────────────────────────────────────

/// `new` command bodies come from stdin. Empty when stdin is a TTY (no pipe).
pub fn read_stdin() -> String {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return String::new();
    }
    let mut buf = String::new();
    let _ = stdin.lock().read_to_string(&mut buf);
    buf
}

/// Print compact single-line JSON (matches JS `JSON.stringify`). Prints nothing
/// for `None` (e.g. DELETE / 204).
pub fn print(value: Option<&Value>) {
    if let Some(v) = value {
        let s = serde_json::to_string(v).expect("serialize response");
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{s}");
    }
}

// ─── validation (lengths measured in UTF-16 units == JS String.length) ──────

fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

pub fn validate_title(entry: &str, title: &str) {
    let n = len16(title);
    if n == 0 || n > MAX_TITLE_LEN {
        die(entry, &format!("title length must be 1..{MAX_TITLE_LEN}"));
    }
}

pub fn validate_body(entry: &str, body: &str) {
    if len16(body) > MAX_BODY_LEN {
        die(entry, &format!("body too long (max {MAX_BODY_LEN} chars)"));
    }
}

pub fn validate_project(entry: &str, name: &str) {
    let n = len16(name);
    if n == 0 || n > MAX_PROJECT_NAME_LEN {
        die(entry, &format!("project name length must be 1..{MAX_PROJECT_NAME_LEN}"));
    }
}

// ─── flag parsing for `set` commands ────────────────────────────────────────

#[derive(Default)]
pub struct PatchFlags {
    pub title: Option<String>,
    pub body: Option<String>,
    pub status: Option<String>,
}

impl PatchFlags {
    /// Emit only the set fields as a JSON object for a PATCH body.
    pub fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        if let Some(t) = &self.title {
            map.insert("title".into(), Value::String(t.clone()));
        }
        if let Some(b) = &self.body {
            map.insert("body".into(), Value::String(b.clone()));
        }
        if let Some(s) = &self.status {
            map.insert("status".into(), Value::String(s.clone()));
        }
        Value::Object(map)
    }
}

/// Parse `--title/--body/--status` (both `--flag value` and `--flag=value`).
/// At least one field is required; `status` is validated against `allowed`.
pub fn parse_set_flags(
    entry: &str,
    args: &[String],
    allowed_statuses: &[&str],
    usage: &str,
) -> PatchFlags {
    let mut flags = PatchFlags::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let (key, inline): (&str, Option<String>) = match arg.split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k, Some(v.to_string())),
            _ => (arg.as_str(), None),
        };
        let slot = match key {
            "--title" => &mut flags.title,
            "--body" => &mut flags.body,
            "--status" => &mut flags.status,
            _ if key.starts_with("--") => die_usage(entry, usage, &format!("unknown option {key}")),
            _ => die_usage(entry, usage, &format!("unexpected argument {arg}")),
        };
        let value = match inline {
            Some(v) => v,
            None => {
                i += 1;
                match args.get(i) {
                    Some(v) => v.clone(),
                    None => die_usage(entry, usage, &format!("option {key} requires an argument")),
                }
            }
        };
        *slot = Some(value);
        i += 1;
    }

    if flags.title.is_none() && flags.body.is_none() && flags.status.is_none() {
        die_usage(entry, usage, "no fields provided");
    }
    if let Some(s) = &flags.status {
        if !allowed_statuses.contains(&s.as_str()) {
            die_usage(
                entry,
                usage,
                &format!("invalid status '{s}'; allowed: {}", allowed_statuses.join("|")),
            );
        }
    }
    if let Some(t) = &flags.title {
        validate_title(entry, t);
    }
    if let Some(b) = &flags.body {
        validate_body(entry, b);
    }
    flags
}

pub fn require_no_args(entry: &str, args: &[String], usage: &str) {
    if !args.is_empty() {
        die_usage(entry, usage, &format!("unexpected argument {}", args[0]));
    }
}

/// `id` must be present and not look like a flag; `rest` must be empty.
pub fn require_id(entry: &str, id: Option<&String>, rest: &[String], usage: &str) -> String {
    match id {
        Some(s) if !s.starts_with("--") => {
            if !rest.is_empty() {
                die_usage(entry, usage, &format!("unexpected argument {}", rest[0]));
            }
            s.clone()
        }
        _ => die_usage(entry, usage, "missing <id>"),
    }
}

// ─── installer (update / uninstall via install.sh) ──────────────────────────

pub fn run_installer(entry: &str, args: &[&str]) {
    let is_uninstall = args.contains(&"--uninstall");
    // Refuse update when the running executable is not the installed binary
    // (e.g. `cargo run` resolves to a target/ path, not the installed name).
    // Uninstall is harmless — it just clears ~/.local/bin/<entry> — so allow it.
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    if !is_uninstall && exe_name != entry {
        die(
            entry,
            &format!(
                "refusing update: current executable is \"{exe_name}\", expected \"{entry}\"; \
                 update only works on the installed binary, not when running from source"
            ),
        );
    }
    let suffix = if args.is_empty() {
        String::new()
    } else {
        format!(" -s -- {}", args.join(" "))
    };
    let cmd = format!("curl -fsSL {INSTALL_URL} | bash{suffix}");
    let ok = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        die(entry, if is_uninstall { "uninstall failed" } else { "update failed" });
    }
}
