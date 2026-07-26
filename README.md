```When Editing
本文档作用: 工程总览 (价值主张 / 使用 / 架构 / 结构); MUST NOT 写发布流程 (→ workflow.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 章节按需增删, 只留项目真有的; 首行一行价值主张, MUST NOT 带 LLM 提示
- 短并列项用表格; 可执行步骤 fenced + `#` 注释同行
- NEVER 写「开发」段 (VibeCoding 不向人类解释 dev 命令)
```

# `jj-agentic-aspect`

AI 专用 Spec/Task/Ask/Session 追踪系统 (macOS only, x64 + arm64). 数据存 Cloudflare D1 (Worker); 单一本地 CLI `jj-agentic-aspect`, 子命令 `plan` / `ask` / `hook`.

## 使用

```sh
curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-agentic-aspect/main/scripts/install.sh | bash
```

一键安装 `jj-agentic-aspect` 到 `$HOME/.local/bin/` (自动清理旧版 `jj-plan`/`jj-ask`/`jj-status` 二进制). 配置 `~/.config/jj-agentic-aspect/config.json` (遵循 XDG, 尊重 `$XDG_CONFIG_HOME`; 旧路径 `~/.config/jj-plan`、`~/.config/jjplan`、`~/.jjplan` 仍作只读 fallback): `endpoint` + Cloudflare Access service token (`cf_access_client_id` + `cf_access_client_secret`). `jj-agentic-aspect --help` 及各子命令 `--help` 查看命令; dashboard 经 Cloudflare Access (Google) 登录.

`hook` 子命令记录 agent session 运行事件 (source 标注上报方, 目前 claude-code, 预留 codex 等): 在 `~/.claude/settings.json` 的 `hooks` 里把各事件指向 `jj-agentic-aspect hook --source claude-code` (配置样例见 `jj-agentic-aspect hook --help`), 即自动上报, dashboard SESSIONS tab 回看.

## 架构

- **模型**: project -> spec -> task (ULID id); ask 按 project 扁平存储; status (hook 事件, 带 source) 按 project -> session 追加只读
- **技术栈**: Rust CLI + Cloudflare Worker (D1) + Next.js SPA (静态导出, Worker 托管)
- **认证**: 单一路径 = Cloudflare Access JWT. dashboard 经 Google SSO (无密码); CLI 用 Access service token (headless, 边缘校验后注入 JWT); Worker 对受保护路由校验 Access JWT (issuer + AUD 绑定); endpoint 指向受 Access 保护的自定义域 (workers.dev 旁路入口已关闭)
- **入口**: 唯一入口 `jj-agentic-aspect.yigegongjiang.com` (worker 自定义域 + Access 应用同名绑定); D1 database_name `jjplan` 保留旧名 (label, D1 无 rename)

## 项目结构

<!-- prettier-ignore -->
| 目录 | 职责 |
|---|---|
| `cli/` | CLI 二进制 (`jj-agentic-aspect`, 子命令 plan/ask/hook), Rust |
| `worker/` | Cloudflare Worker + D1 migrations |
| `web/` | Next.js dashboard SPA (静态导出) |
