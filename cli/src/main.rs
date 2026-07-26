// jj-agentic-aspect: single binary, three subcommands.
//   plan  — Spec/Task tracking (was the jj-plan binary)
//   ask   — human ask (Q&A) logging (was jj-ask)
//   hook  — agent hook session-event logging (was jj-status)
// Global commands (--help/--version/update/uninstall) live only at this level.
use jj_agentic_aspect::{ask, die, die_usage, hook, plan, run_installer, ENTRY, VERSION};

fn fail_usage(reason: &str) -> ! {
    die_usage(ENTRY, "jj-agentic-aspect --help", reason)
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    let head = argv.first().map(String::as_str);
    if argv.is_empty() || head == Some("help") || head == Some("-h") || head == Some("--help") {
        if argv.len() > 1 {
            fail_usage(&format!("unexpected argument {}", argv[1]));
        }
        print!("{}", HELP.replace("{VERSION}", VERSION));
        return;
    }
    if head == Some("-v") || head == Some("--version") {
        if argv.len() > 1 {
            fail_usage(&format!("unexpected argument {}", argv[1]));
        }
        println!("{VERSION}");
        return;
    }
    if head == Some("update") || head == Some("upgrade") {
        if argv.len() > 1 {
            fail_usage(&format!("unexpected argument {}", argv[1]));
        }
        run_installer(ENTRY, &[]);
        return;
    }
    if head == Some("uninstall") {
        if argv.len() > 1 {
            fail_usage(&format!("unexpected argument {}", argv[1]));
        }
        run_installer(ENTRY, &["--uninstall"]);
        return;
    }

    let rest = argv.get(1..).unwrap_or(&[]);
    match argv[0].as_str() {
        "plan" => plan::run(rest),
        "ask" => ask::run(rest),
        "hook" => hook::run(rest),
        // Old top-level nouns from the pre-merge binaries: point at the new home.
        "project" | "spec" | "task" => die(
            ENTRY,
            &format!(
                "'{0}' moved under the plan subcommand; run 'jj-agentic-aspect plan {0} ...' instead",
                argv[0]
            ),
        ),
        "status" | "sessions" => die(
            ENTRY,
            &format!(
                "'{}' moved under the hook subcommand; run 'jj-agentic-aspect hook ...' instead",
                argv[0]
            ),
        ),
        other => fail_usage(&format!("unknown subcommand '{other}'")),
    }
}

const HELP: &str = r#"jj-agentic-aspect {VERSION}

# TLDR
jj-agentic-aspect: AI 专用 Spec/Task/Ask/Session 追踪 CLI, 单二进制三个子命令. <project>=cwd basename.

  jj-agentic-aspect plan ...   # Spec/Task 跟踪: 立计划 -> 拆任务 -> 推状态
  jj-agentic-aspect ask ...    # 落盘人类抛给 AI 的请求 (Q&A 记录)
  jj-agentic-aspect hook ...   # 落盘 agent hook 的 session 运行事件 (source: claude-code / codex / ...)

各子命令详情: jj-agentic-aspect plan --help | ask --help | hook --help

# GLOBAL COMMANDS

jj-agentic-aspect --help | --version
jj-agentic-aspect update | upgrade      重装到最新版 (仅在用户明确要求时执行; update/upgrade 等价)
jj-agentic-aspect uninstall             删除二进制, 保留配置 (仅在用户明确要求时执行)

# CONFIG
~/.config/jj-agentic-aspect/config.json (XDG; 旧路径 ~/.config/jj-plan 等只读 fallback):
endpoint + cf_access_client_id + cf_access_client_secret (Cloudflare Access service token).
"#;
