// `ask` subcommand: Q&A (human ask) logging. Thin HTTP client over the worker.
use crate::{
    api, die, die_usage, encode_uri_component, print, require_id, validate_project, ENTRY,
    ASK_LIMIT_DEFAULT, ASK_LIMIT_MAX, MAX_BODY_LEN, MAX_PROJECT_NAME_LEN, VERSION,
};
use serde_json::{Map, Value};

fn usage(key: &str) -> String {
    match key {
        "help" => "jj-agentic-aspect ask --help".into(),
        "ask.new" => "jj-agentic-aspect ask new <project> <body>".into(),
        "ask.ls" => format!("jj-agentic-aspect ask ls <project> [--limit N]   (default {ASK_LIMIT_DEFAULT}, max {ASK_LIMIT_MAX})"),
        "ask.show" => "jj-agentic-aspect ask show <id>".into(),
        "ask.set" => "jj-agentic-aspect ask set <id> --body <body>".into(),
        "ask.rm" => "jj-agentic-aspect ask rm <id>".into(),
        _ => "jj-agentic-aspect ask --help".into(),
    }
}

fn fail(msg: &str) -> ! {
    die(ENTRY, msg)
}
fn fail_usage(key: &str, reason: &str) -> ! {
    die_usage(ENTRY, &usage(key), reason)
}

fn tail(rest: &[String]) -> &[String] {
    rest.get(1..).unwrap_or(&[])
}

fn body_object(body: String) -> Value {
    let mut m = Map::new();
    m.insert("body".into(), Value::String(body));
    Value::Object(m)
}

fn parse_new_args(args: &[String]) -> (String, String) {
    let mut project: Option<String> = None;
    let mut body: Option<String> = None;
    for arg in args {
        if arg.starts_with("--") {
            fail_usage("ask.new", &format!("unknown option {arg}"));
        } else if project.is_none() {
            project = Some(arg.clone());
        } else if body.is_none() {
            body = Some(arg.clone());
        } else {
            fail_usage("ask.new", &format!("unexpected argument {arg}"));
        }
    }
    let project = project.unwrap_or_else(|| fail_usage("ask.new", "missing <project>"));
    let body = body.unwrap_or_else(|| fail_usage("ask.new", "missing <body>"));
    validate_project(ENTRY, &project);
    let n = body.encode_utf16().count();
    if n == 0 || n > MAX_BODY_LEN {
        fail_usage("ask.new", &format!("body length must be 1..{MAX_BODY_LEN}"));
    }
    (project, body)
}

fn parse_ls_args(args: &[String]) -> (String, Option<u32>) {
    let mut project: Option<String> = None;
    let mut limit: Option<u32> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--limit" {
            if limit.is_some() {
                fail_usage("ask.ls", "duplicate --limit");
            }
            i += 1;
            let v = match args.get(i) {
                Some(v) if !v.starts_with("--") => v,
                _ => fail_usage("ask.ls", "missing <N> after --limit"),
            };
            match v.parse::<u32>() {
                Ok(n) if (1..=ASK_LIMIT_MAX).contains(&n) => limit = Some(n),
                _ => fail_usage("ask.ls", &format!("--limit must be integer in 1..{ASK_LIMIT_MAX}")),
            }
        } else if arg.starts_with("--") {
            fail_usage("ask.ls", &format!("unknown option {arg}"));
        } else if project.is_none() {
            project = Some(arg.clone());
        } else {
            fail_usage("ask.ls", &format!("unexpected argument {arg}"));
        }
        i += 1;
    }
    let project = project.unwrap_or_else(|| fail_usage("ask.ls", "missing <project>"));
    validate_project(ENTRY, &project);
    (project, limit)
}

fn parse_set_args(args: &[String]) -> (String, String) {
    let mut id: Option<String> = None;
    let mut body: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--body" {
            if body.is_some() {
                fail_usage("ask.set", "duplicate --body");
            }
            i += 1;
            match args.get(i) {
                Some(v) => body = Some(v.clone()),
                None => fail_usage("ask.set", "missing <body> after --body"),
            }
        } else if arg.starts_with("--") {
            fail_usage("ask.set", &format!("unknown option {arg}"));
        } else if id.is_none() {
            id = Some(arg.clone());
        } else {
            fail_usage("ask.set", &format!("unexpected argument {arg}"));
        }
        i += 1;
    }
    let id = id.unwrap_or_else(|| fail_usage("ask.set", "missing <id>"));
    let body = body.unwrap_or_else(|| fail_usage("ask.set", "missing --body"));
    let n = body.encode_utf16().count();
    if n == 0 || n > MAX_BODY_LEN {
        fail_usage("ask.set", &format!("body length must be 1..{MAX_BODY_LEN}"));
    }
    (id, body)
}

fn dispatch(verb: &str, rest: &[String]) {
    match verb {
        "new" => {
            let (project, body) = parse_new_args(rest);
            let path = format!("/projects/{}/asks", encode_uri_component(&project));
            print(api(ENTRY, "POST", &path, Some(body_object(body))).as_ref());
        }
        "ls" => {
            let (project, limit) = parse_ls_args(rest);
            let q = limit.map(|n| format!("?limit={n}")).unwrap_or_default();
            let path = format!("/projects/{}/asks{q}", encode_uri_component(&project));
            print(api(ENTRY, "GET", &path, None).as_ref());
        }
        "show" => {
            let id = require_id(ENTRY, rest.first(), tail(rest), &usage("ask.show"));
            print(api(ENTRY, "GET", &format!("/asks/{}", encode_uri_component(&id)), None).as_ref());
        }
        "set" => {
            let (id, body) = parse_set_args(rest);
            let path = format!("/asks/{}", encode_uri_component(&id));
            print(api(ENTRY, "PATCH", &path, Some(body_object(body))).as_ref());
        }
        "rm" => {
            let id = require_id(ENTRY, rest.first(), tail(rest), &usage("ask.rm"));
            api(ENTRY, "DELETE", &format!("/asks/{}", encode_uri_component(&id)), None);
        }
        _ => fail(&format!("unknown command 'ask {verb}'; usage: {}", usage("help"))),
    }
}

/// Entry from main: argv is everything after `ask`.
pub fn run(argv: &[String]) {
    let head = argv.first().map(String::as_str);
    if argv.is_empty() || head == Some("help") || head == Some("-h") || head == Some("--help") {
        if argv.len() > 1 {
            fail_usage("help", &format!("unexpected argument {}", argv[1]));
        }
        print_help();
        return;
    }

    let verb = &argv[0];
    let rest = argv.get(1..).unwrap_or(&[]);
    dispatch(verb, rest);
}

pub fn print_help() {
    let help = HELP
        .replace("{VERSION}", VERSION)
        .replace("{MAX_BODY_LEN}", &MAX_BODY_LEN.to_string())
        .replace("{MAX_PROJECT_NAME_LEN}", &MAX_PROJECT_NAME_LEN.to_string())
        .replace("{ASK_LIMIT_DEFAULT}", &ASK_LIMIT_DEFAULT.to_string())
        .replace("{ASK_LIMIT_MAX}", &ASK_LIMIT_MAX.to_string());
    print!("{help}");
}

const HELP: &str = r#"jj-agentic-aspect ask {VERSION}

# TLDR
ask: 落盘人类抛给 AI 的请求 (Q&A 记录). 两层模型 project -> ask, id=ULID. <project>=cwd basename.
每条 ask 都是独立记录, 不串链.

  jj-agentic-aspect ask new <project> <body>
    # body=用户原话原文照搬.

输出: stdout 单行 JSON. body 不读 stdin (位置参数). 查询/修改/删除见 jj-agentic-aspect ask --help.

# PURPOSE
落盘人类抛给 AI 的请求.

# MODEL
project (name) -- ask (id=ULID)
- project 自动 upsert.
- ask 之间相互独立, 无 prev/next 关系.
- project rm 级联删全部 ask.

# I/O
- 输出: stdout 单行 JSON; DELETE 空 (204). 错误: stderr `jj-agentic-aspect: <msg>` + 非零 exit.
- <body> / --body 是位置/flag 参数, 不读 stdin.
- 限长 (chars): body 1..{MAX_BODY_LEN}, project 1..{MAX_PROJECT_NAME_LEN}.

# COMMANDS

jj-agentic-aspect ask new <project> <body>
  -> {id, project_id, body, created_at, updated_at}
jj-agentic-aspect ask ls <project> [--limit N]   (default {ASK_LIMIT_DEFAULT}, max {ASK_LIMIT_MAX}, by updated_at DESC)
  -> [{...ask}]
  err: 404
jj-agentic-aspect ask show <id>
  -> {...ask}
  err: 404
jj-agentic-aspect ask set <id> --body <body>
  -> {...ask}
  err: 400 | 404
jj-agentic-aspect ask rm <id>
  err: 404
"#;
