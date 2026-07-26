// jj-plan binary: project/spec/task workflow. Thin HTTP client over the worker.
use jj_plan_cli::{
    api, die, die_usage, encode_uri_component, parse_set_flags, print, read_stdin, require_id,
    require_no_args, run_installer, validate_body, validate_project, validate_title, MAX_BODY_LEN,
    MAX_PROJECT_NAME_LEN, MAX_TITLE_LEN, SPEC_STATUSES, TASK_STATUSES, VERSION,
};
use serde_json::{Map, Value};

const ENTRY: &str = "jj-plan";

fn usage(key: &str) -> String {
    let spec_st = SPEC_STATUSES.join("|");
    let task_st = TASK_STATUSES.join("|");
    match key {
        "help" => "jj-plan --help".into(),
        "version" => "jj-plan --version".into(),
        "update" => "jj-plan update | upgrade".into(),
        "uninstall" => "jj-plan uninstall".into(),
        "project.ls" => "jj-plan project ls".into(),
        "project.rm" => "jj-plan project rm <name>".into(),
        "spec.new" => "jj-plan spec new <project> <title> [--after <prev_spec_id>]".into(),
        "spec.ls" => "jj-plan spec ls <project>".into(),
        "spec.show" => "jj-plan spec show <id>".into(),
        "spec.set" => format!("jj-plan spec set <id> [--title T] [--body B] [--status {spec_st}]"),
        "spec.rm" => "jj-plan spec rm <id>".into(),
        "task.new" => "jj-plan task new <spec_id> <title> [--after <prev_task_id>]".into(),
        "task.ls" => "jj-plan task ls <spec_id>".into(),
        "task.set" => format!("jj-plan task set <id> [--title T] [--body B] [--status {task_st}]"),
        "task.rm" => "jj-plan task rm <id>".into(),
        _ => "jj-plan --help".into(),
    }
}

fn fail(msg: &str) -> ! {
    die(ENTRY, msg)
}
fn fail_usage(key: &str, reason: &str) -> ! {
    die_usage(ENTRY, &usage(key), reason)
}

// args after noun+verb; `rest[1..]` guarded against an empty slice.
fn tail(rest: &[String]) -> &[String] {
    rest.get(1..).unwrap_or(&[])
}

/// Parse `<first> <title> [--after <prev>]` for spec new / task new.
/// Presence only; length validation is done by the caller (matching TS order).
fn parse_new_args(
    key: &str,
    args: &[String],
    first_label: &str,
    prev_label: &str,
) -> (String, String, Option<String>) {
    let mut first: Option<String> = None;
    let mut title: Option<String> = None;
    let mut prev: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--after" {
            if prev.is_some() {
                fail_usage(key, "duplicate --after");
            }
            i += 1;
            match args.get(i) {
                Some(v) if !v.is_empty() && !v.starts_with("--") => prev = Some(v.clone()),
                _ => fail_usage(key, &format!("missing {prev_label} after --after")),
            }
        } else if arg.starts_with("--") {
            fail_usage(key, &format!("unknown option {arg}"));
        } else if first.is_none() {
            first = Some(arg.clone());
        } else if title.is_none() {
            title = Some(arg.clone());
        } else {
            fail_usage(key, &format!("unexpected argument {arg}"));
        }
        i += 1;
    }

    let first = first.unwrap_or_else(|| fail_usage(key, &format!("missing {first_label}")));
    let title = title.unwrap_or_else(|| fail_usage(key, "missing <title>"));
    (first, title, prev)
}

fn new_payload(title: String, body: String, prev: Option<String>) -> Value {
    let mut m = Map::new();
    m.insert("title".into(), Value::String(title));
    m.insert("body".into(), Value::String(body));
    if let Some(p) = prev {
        m.insert("prev_id".into(), Value::String(p));
    }
    Value::Object(m)
}

fn run(noun: &str, verb: Option<&str>, rest: &[String]) {
    match (noun, verb) {
        ("project", Some("ls")) => {
            require_no_args(ENTRY, rest, &usage("project.ls"));
            print(api(ENTRY, "GET", "/projects", None).as_ref());
        }
        ("project", Some("rm")) => {
            let name = match rest.first() {
                Some(n) if !n.starts_with("--") => n,
                _ => fail_usage("project.rm", "missing <name>"),
            };
            if let Some(extra) = rest.get(1) {
                fail_usage("project.rm", &format!("unexpected argument {extra}"));
            }
            api(ENTRY, "DELETE", &format!("/projects/{}", encode_uri_component(name)), None);
        }
        ("spec", Some("new")) => {
            let (project, title, prev) =
                parse_new_args("spec.new", rest, "<project>", "<prev_spec_id>");
            validate_project(ENTRY, &project);
            validate_title(ENTRY, &title);
            let body = read_stdin();
            validate_body(ENTRY, &body);
            let path = format!("/projects/{}/specs", encode_uri_component(&project));
            print(api(ENTRY, "POST", &path, Some(new_payload(title, body, prev))).as_ref());
        }
        ("spec", Some("ls")) => {
            let project = match rest.first() {
                Some(p) if !p.starts_with("--") => p,
                _ => fail_usage("spec.ls", "missing <project>"),
            };
            if let Some(extra) = rest.get(1) {
                fail_usage("spec.ls", &format!("unexpected argument {extra}"));
            }
            validate_project(ENTRY, project);
            let path = format!("/projects/{}/specs", encode_uri_component(project));
            print(api(ENTRY, "GET", &path, None).as_ref());
        }
        ("spec", Some("show")) => {
            let id = require_id(ENTRY, rest.first(), tail(rest), &usage("spec.show"));
            print(api(ENTRY, "GET", &format!("/specs/{}", encode_uri_component(&id)), None).as_ref());
        }
        ("spec", Some("set")) => {
            let id = require_id(ENTRY, rest.first(), &[], &usage("spec.set"));
            let flags = parse_set_flags(ENTRY, tail(rest), SPEC_STATUSES, &usage("spec.set"));
            let path = format!("/specs/{}", encode_uri_component(&id));
            print(api(ENTRY, "PATCH", &path, Some(flags.to_json())).as_ref());
        }
        ("spec", Some("rm")) => {
            let id = require_id(ENTRY, rest.first(), tail(rest), &usage("spec.rm"));
            api(ENTRY, "DELETE", &format!("/specs/{}", encode_uri_component(&id)), None);
        }
        ("task", Some("new")) => {
            let (spec_id, title, prev) =
                parse_new_args("task.new", rest, "<spec_id>", "<prev_task_id>");
            validate_title(ENTRY, &title);
            let body = read_stdin();
            validate_body(ENTRY, &body);
            let path = format!("/specs/{}/tasks", encode_uri_component(&spec_id));
            print(api(ENTRY, "POST", &path, Some(new_payload(title, body, prev))).as_ref());
        }
        ("task", Some("ls")) => {
            let spec_id = match rest.first() {
                Some(s) if !s.starts_with("--") => s,
                _ => fail_usage("task.ls", "missing <spec_id>"),
            };
            if let Some(extra) = rest.get(1) {
                fail_usage("task.ls", &format!("unexpected argument {extra}"));
            }
            let spec = api(ENTRY, "GET", &format!("/specs/{}", encode_uri_component(spec_id)), None);
            match spec.as_ref().and_then(|v| v.get("tasks")) {
                Some(tasks) if tasks.is_array() => print(Some(tasks)),
                _ => fail("unexpected response: tasks missing"),
            }
        }
        ("task", Some("set")) => {
            let id = require_id(ENTRY, rest.first(), &[], &usage("task.set"));
            let flags = parse_set_flags(ENTRY, tail(rest), TASK_STATUSES, &usage("task.set"));
            let path = format!("/tasks/{}", encode_uri_component(&id));
            print(api(ENTRY, "PATCH", &path, Some(flags.to_json())).as_ref());
        }
        ("task", Some("rm")) => {
            let id = require_id(ENTRY, rest.first(), tail(rest), &usage("task.rm"));
            api(ENTRY, "DELETE", &format!("/tasks/{}", encode_uri_component(&id)), None);
        }
        _ => {
            let cmd = [Some(noun), verb].into_iter().flatten().collect::<Vec<_>>().join(" ");
            fail(&format!("unknown command '{cmd}'; usage: {}", usage("help")));
        }
    }
}

fn print_help() {
    let help = HELP
        .replace("{VERSION}", VERSION)
        .replace("{SPEC_STATUSES}", &SPEC_STATUSES.join("|"))
        .replace("{TASK_STATUSES}", &TASK_STATUSES.join("|"))
        .replace("{MAX_TITLE_LEN}", &MAX_TITLE_LEN.to_string())
        .replace("{MAX_BODY_LEN}", &MAX_BODY_LEN.to_string())
        .replace("{MAX_PROJECT_NAME_LEN}", &MAX_PROJECT_NAME_LEN.to_string());
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

    let noun = &argv[0];
    let verb = argv.get(1).map(String::as_str);
    let rest = argv.get(2..).unwrap_or(&[]);

    if noun == "ask" {
        fail(&format!(
            "'ask' is a jj-ask command; run 'jj-ask {}' instead",
            verb.unwrap_or("--help")
        ));
    }
    if noun == "status" {
        fail(&format!(
            "'status' is a jj-status command; run 'jj-status {}' instead",
            verb.unwrap_or("--help")
        ));
    }
    run(noun, verb, rest);
}

const HELP: &str = r#"jj-plan {VERSION}

# TLDR
jj-plan: AI 用的 Spec/Task 跟踪 CLI. 三层模型 project -> spec -> task, id=ULID. <project>=cwd basename.
循环: 写 spec 立计划 -> 拆 task -> 推 task status (todo/doing/done/blocked) -> 所有 task done 后 spec set done.

  jj-plan spec new <project> <title>     # body 从 stdin 读; project 不存在自动建
  jj-plan task new <spec_id> <title>     # body 从 stdin 读; 默认追加链尾, --after <id> 中间插
  jj-plan task set <id> --status <s>     # 亦可改 --title/--body
  jj-plan spec set <id> --status done    # 收尾, 需所有 task 已 done

输出: stdout 单行 JSON. 查询/删除/错误码/链语义见 jj-plan --help.

# PURPOSE
为 AI 设计的 Spec/Task 跟踪 CLI.

# MODEL
project (name, 主键) -- spec (id=ULID) -- task (id=ULID)
- project 自动 upsert: spec new 首次提及即创建; 无 `project new`.
- spec/task 用 --after 串成单链, 禁止 fork (一个 prev 至多一个后继, 二次引用 -> 409).
- 中间删/插自动接续: A->B->C 删 B => A->C; --after A 插 X => A->X->B->C.
- 级联删除 (不可逆): project rm 删其下全部 spec+task; spec rm 删其下全部 task.

# I/O
- 输出: stdout 单行 JSON; DELETE 返回空 (HTTP 204).
- 错误: stderr 单行 `jj-plan: <msg>` + 非零 exit; 客户端不重试.
- new 命令的 body 从 stdin 读 (无 stdin = 空 body); set 改 body 用 --body flag, 整体覆盖.
- id 一律 ULID, 必须从响应 JSON 取, 不可构造或截断.
- 限长 (chars): title 1..{MAX_TITLE_LEN}, body 0..{MAX_BODY_LEN}, project 1..{MAX_PROJECT_NAME_LEN}.

# COMMANDS

jj-plan --help | --version
jj-plan update | upgrade | uninstall     仅在用户明确要求时执行 (同时影响 jj-ask; update/upgrade 等价)

jj-plan project ls
  -> [{name, created_at, updated_at, specs:[{...spec, tasks:[...task]}]}]
jj-plan project rm <name>
  err: 404

jj-plan spec new <project> <title> [--after <prev_spec_id>]
  -> {id, project_id, title, body, status:"active", prev_id, created_at, updated_at}
  err: 400 prev 跨项目/不存在 | 409 prev 已有后继
jj-plan spec ls <project>
  -> [{...spec, tasks:[...task]}]   (链序)
  err: 404
jj-plan spec show <id>
  -> {...spec, tasks:[...task]}
  err: 404
jj-plan spec set <id> [--title T] [--body B] [--status {SPEC_STATUSES}]
  至少传一个 flag.
  -> {...spec}
  err: 400 无 flag/status 非法 | 404
jj-plan spec rm <id>
  err: 404 | 409 并发

jj-plan task new <spec_id> <title> [--after <prev_task_id>]
  不传 --after 追加链尾.
  -> {id, spec_id, title, body, status:"todo", prev_id, created_at, updated_at}
  err: 400 prev 跨 spec/不存在 | 404 spec 不存在 (仅无 --after 时) | 409 并发
jj-plan task ls <spec_id>
  -> [...task]   (链序)
  err: 404
jj-plan task set <id> [--title T] [--body B] [--status {TASK_STATUSES}]
  至少传一个 flag.
  -> {...task}
  err: 400 | 404
jj-plan task rm <id>
  err: 404 | 409 并发

# STATUS
spec  active (默认, 含立项与执行中) | done (所有 task done 后切)
task  todo (默认) -> doing -> done; 任意非 done 可 -> blocked
blocked: 切入时原因写 body, 解除后回 todo/doing.
系统不强制状态机, 任意状态可互切, 由 AI 自律.

# BEHAVIOR
- 默认沿 task 链找首个非 done 推进, 不跳序.
- spec 链必须按链序完成: A->B->C, A done 才进 B.
- 用户显式指定 id => 跳序推进; AI 不得自行跳序.
- spec done 必须在其所有 task done 之后.
- 拿到 409 不重试, 重新查询当前状态再决策.
"#;
