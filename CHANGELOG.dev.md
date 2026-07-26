```When Editing
本文档作用: 面向开发者的发版记录; CHANGELOG.md 的超集, 1:1 镜像 + 技术变更子项
遵循 AGENTS.md 文档编写规范
- 每条主项 = CHANGELOG.md 对应条目 (原文), 下方缩进子项承载技术变更
- 子项 MAY 写路径 / 函数 / 机制; ≤ 1 行
```

# Changelog (developer, follow [CHANGELOG.md](./CHANGELOG.md))

> 历史 27 版 (≤ 0.8.23) 在双文件分界确立前写成, 原文照搬未回填; 用户向 / 开发向严格分界自 **0.8.24** 起执行.

## [0.16.0] - 2026-07-26

### Added

- 新增 `jj-status` CLI: 挂进 Claude Code hooks (`~/.claude/settings.json` 指向 `jj-status hook`, 样例见 `jj-status --help`) 后自动记录每个 session 的运行事件 (提示词 / 工具调用 / 停止等); 上报静默且永不干扰宿主 session, 随安装脚本一并安装。
  - `worker/migrations/0005_add_statuses.sql`: `statuses` 表 (id ULID / project_id FK CASCADE / session_id / event / body=原始 hook JSON / created_at, 无 updated_at=追加只读) + 复合索引 `(project_id, session_id)`。
  - `worker/src/index.ts`: `POST /projects/:name/statuses` (project upsert + bump); `GET /projects/:name/sessions` (GROUP BY 摘要 + `json_extract` 取首条 `UserPromptSubmit` prompt 预览, `json_valid` 兜底); `GET /projects/:name/sessions/:sid/statuses` (ULID 序, 上限 5000); `DELETE /projects/:name/sessions/:sid`; project rename/merge 两分支迁移 `statuses.project_id`; `GET /projects` 增 `sessions_count` (COUNT DISTINCT session_id)。
  - `cli/src/bin/jj-status.rs`: `hook` 子命令 stdin 读事件 JSON, project=JSON `cwd` basename (回退 `$CLAUDE_PROJECT_DIR`/进程 cwd), 递归截断超长字符串 (2048→256 两档) + 数组截前 100 项 + 骨架兜底, 保证 body ≤ 上限; 任何失败静默 exit 0, 上报限时 10s; 查询 `sessions`/`ls`/`rm`。
  - `cli/src/lib.rs`: `load_config`/`api` 拆出可失败的 `try_load_config`/`try_api(timeout)` 供 hook 静默路径复用; `jj-plan` 对 `status` noun 重定向提示 (同 `ask`)。
  - `scripts/install.sh` + `scripts/install-local.sh` `BINARIES` + `.github/workflows/release.yml` 编译/产物/checksums 增 `jj-status`。
- dashboard 项目页新增 SESSIONS tab: session 列表 (首条提示词 / 时长 / 事件数) + 单 session 事件时间线; 每条事件提炼重点直接展示, 原始 JSON 可展开, 短内容自动全渲染; 支持删除整个 session。
  - `web/lib/types.ts` (`SessionSummary`/`Status`/`sessions_count`) + `web/lib/api.ts` (`listSessions`/`listSessionStatuses`/`deleteSession`) + `web/lib/format.ts` (`fmtDuration`)。
  - 新增 `web/components/SessionsView.tsx` (卡片列表) + `SessionDetail.tsx` (时间线: 按事件类型提 digest — prompt/tool_name/command/file_path 等, badge 分色, raw JSON 折叠, body ≤ 400 字符且无 digest 时自动展开; 自带 5s 可见页轮询)。
  - `web/components/Dashboard.tsx`: 新路由 `?p=<project>&sess=<session_id>` + sessions 状态镜像 asks 模式 + 删除确认; `ProjectTabs.tsx` 第三 tab; `ProjectsList.tsx` 计数行增 sessions。

## [0.15.0] - 2026-07-23

### Changed

- CLI 由 Bun/TypeScript 迁移为 Rust 原生实现; 命令 / 参数 / stdin / 输出 / 安装与更新方式完全不变, 调用方无需改动。
  - `cli/`: 删 `build.ts` / `package.json` / `bun.lock` / `tsconfig.json` / `src/*.ts`; 新增 `Cargo.toml` (ureq + serde_json[preserve_order], release strip+lto+abort) + `build.rs` (读 `../VERSION` 注入 `JJ_VERSION`) + `src/lib.rs` 共享层 + `src/bin/{jj-plan,jj-ask}.rs` 双二进制。
  - `serde_json` preserve_order + compact 序列化 = 旧 `JSON.stringify(JSON.parse(text))` 逐字节等价; 长度校验用 UTF-16 单位对齐 JS `String.length`; 手写 arg 解析复刻 usage/错误语义 (--help 与解析错误路径对旧 TS 逐字节比对通过, 唯 flag 缺值文案有别)。
  - `.github/workflows/release.yml` `build-cli`: `ubuntu`+bun → `macos-latest`+`cargo build --release` 双 target (`aarch64`/`x86_64-apple-darwin`); 产物按 install.sh 约定改名 `…-macos-{arm64,x64}`; 校验和 `sha256sum` → `shasum -a 256`。`deploy-worker` 不变。
  - `workflow.md` / `README.md` CLI 技术栈与调试命令 bun → cargo。
- 网络安装脚本迁至 `scripts/install.sh` (curl 地址随之变更)。0.14.x 及更早二进制的 `update` 指向旧地址会失败, 需用新地址重装一次。
  - `git mv install.sh scripts/install.sh`; 同步 `README.md` curl URL、`cli/src/lib.rs` `INSTALL_URL`、脚本头 usage 注释 → `…/main/scripts/install.sh`。
  - 新增 `scripts/install-local.sh`: `cargo build --release` 本机原生构建 + 装入 `$HOME/.local/bin` (供 dev / 发布后本机自安装); `workflow.md` 发布流程新增「本机自安装」步骤调用它。

### Removed

- 移除 bearer token 认证; CLI 与 Worker 统一走 Cloudflare Access (CLI 用 service token)。仅配了 `token` 的旧配置需改为 service token 键 (该模式本就无法通过生产的 Access 网关)。
  - `worker/src/index.ts`: 删 `JJPLAN_TOKEN` binding + authMiddleware bearer 分支 → 仅校验 Access JWT (`Cf-Access-Jwt-Assertion` / `CF_Authorization` cookie, issuer+AUD 绑定)。
  - `cli/src/lib.rs`: `Config` 去 `token`; 仅 `endpoint` + `cf_access_client_id` + `cf_access_client_secret`; api 只发 `CF-Access-Client-Id/Secret` 头。
  - 删 worker 测试套件 (`worker/test/` + `vitest.config.ts` + `package.json` test 脚本 & `vitest`/`vitest-pool-workers` devDeps); CI 不跑 worker 测试, deploy 不受影响。
  - `worker/wrangler.toml` / `web/lib/api.ts` 注释同步为「Access 为唯一认证路径」。

## [0.14.0] - 2026-07-23

### Changed (BREAKING — CLI 命令改名)

- CLI 命令改名 `jjplan` → `jj-plan`, `jjask` → `jj-ask`; 旧命令名不再发布。需重装并更新所有调用方 (含全局 AI 规则里的命令名), 旧二进制自行 `rm`。
  - `cli/build.ts` `ENTRIES` = `["jj-plan","jj-ask"]`; 源文件 `git mv` → `cli/src/jj-plan.ts` / `jj-ask.ts`; 各 `ENTRY` 常量 + USAGE/help 文本全改名。
  - `install.sh` `BINARIES` + `.github/workflows/release.yml` 产物 glob/文案改名; Release 只发 `jj-plan-macos-*` / `jj-ask-macos-*`。
  - `package.json` name (`jj-plan-cli`/`-web`/`-worker`) + web 文案 (dashboard description / ask 检索 placeholder / `localStorage` key `jj-plan_project_tab`) 同步。
  - 保留 (基础设施标识符, 改名会中断已部署服务): Worker `name` / D1 `database_name` = `jjplan`, secret `JJPLAN_TOKEN`, 生产域名 `jjplan.yigegongjiang.com`。
- 配置迁移到 `~/.config/jj-plan/config.json`; 旧 `~/.config/jjplan` 与 `~/.jjplan` 仍作只读 fallback, 现有配置无需改动。
  - `cli/src/shared.ts`: `CONFIG_PATH` 换 `jj-plan`; `LEGACY_CONFIG_PATH` 扩为 `LEGACY_CONFIG_PATHS` 数组 (`~/.config/jjplan` → `~/.jjplan`), `resolveConfigPath()` 按序只读回退, 无运行时搬迁。

## [0.13.0] - 2026-07-17

### Security

- 关闭 workers.dev 旁路入口 (`jjplan.fan-yang2019.workers.dev` 生产 + `*-` 预览): 该入口绕过 Cloudflare Access 仅 bearer 兜底; 现统一只经受 Access 保护的自定义域访问, 自定义域不受影响。
  - `worker/wrangler.toml`: `workers_dev = false` (生产 `<name>.workers.dev` 路由) + `preview_urls = false` (`<version>-<name>.workers.dev` 预览 URL); 下次部署即禁用两者。放进 IaC 而非仅面板切换 —— 面板 off 会被下次 tag 部署按 wrangler 默认 (true) 重置。
  - `README.md`: 架构「认证」项去掉 "endpoint 指向 workers.dev" 过时描述, 仅留受 Access 保护的自定义域。

## [0.12.0] - 2026-07-17

### Changed

- CLI 配置文件迁移到 XDG 标准位置 `~/.config/jjplan/config.json` (尊重 `$XDG_CONFIG_HOME`); 旧路径 `~/.jjplan/config.json` 仍作 fallback, 现有配置无需改动。
  - `cli/src/shared.ts`: `CONFIG_PATH` = `${XDG_CONFIG_HOME:-~/.config}/jjplan/config.json`; 新增 `LEGACY_CONFIG_PATH` (`~/.jjplan`) + `resolveConfigPath()` (canonical 存在则用, 否则 legacy, 皆无则回退 canonical 供报错); `loadConfig` 全程用解析出的 path。
  - `README.md` 使用段 + `install.sh` uninstall 提示同步为 canonical 路径 (标注 legacy fallback)。

## [0.11.1] - 2026-07-17

### Changed

- README 补充认证说明 (Cloudflare Access / Google 登录 / service token), 与 0.10.x–0.11.0 的认证改动对齐; 无运行时变化.
  - `README.md`: 价值主张 `endpoint/token`→`endpoint + 凭证`; 使用段列出 config 两种凭证 (bearer `token` / service token `cf_access_client_id`+`cf_access_client_secret`) + dashboard 经 Cloudflare Access (Google) 登录; 架构新增「认证」项 (dashboard Google SSO / CLI bearer|service token / Worker 双认证 / endpoint workers.dev|自定义域)。
  - 全仓库核对: `AGENTS.md` / `workflow.md` / `install.sh` / CLI `--help` 均无 auth 文案, 无需改。

## [0.11.0] - 2026-07-17

### Added

- CLI 支持 Cloudflare Access Service Token 认证: config 填 `cf_access_client_id` + `cf_access_client_secret`, endpoint 指向受 Access 保护的域即可, 凭证由 Cloudflare 签发/可吊销, 与网页 Google 登录统一。原 `token` 方式仍兼容, 现有配置不受影响。
  - `cli/src/shared.ts`: `Config` 加可选 `cf_access_client_id`/`cf_access_client_secret`, `token` 变可选; `loadConfig` 校验 `endpoint` + (`token` 或 service token 对), 二者缺一即报错; `api()` 按存在与否发 `Authorization: Bearer` 与/或 `CF-Access-Client-Id`/`CF-Access-Client-Secret` 头。
  - Worker 零改动: service token 经 Access edge 校验 (Service Auth policy) 后注入 `Cf-Access-Jwt-Assertion`, 由 0.10.0 的 `authMiddleware` 按 iss/aud 校验。
  - 用法: endpoint 指向已受 Access 保护的 `jjplan.yigegongjiang.com`; 在同一 app 加 Service Auth policy (与 Allow Google 共存, Service Auth 优先评估)。headless/AI agent 场景 Cloudflare 官方推荐 service token, 非浏览器登录。

## [0.10.1] - 2026-07-17

### Changed

- 接入 Cloudflare Access 校验参数 (team domain + AUD), 网页 Google 登录链路打通 (需 Cloudflare 侧 Access 已对自定义域生效).
  - `wrangler.toml` `[vars]`: `CF_ACCESS_TEAM_DOMAIN = https://yigegongjiang.cloudflareaccess.com`, `CF_ACCESS_AUD = e255…c6b` (非机密, 走 CI `wrangler deploy`; 本机 wrangler 非交互无 API token 故不本地 secret put).
  - 无代码改动; 仅打开 0.10.0 已埋好的 Access 校验分支 (`authMiddleware` 见 iss/aud 齐备即验 JWT).

## [0.10.0] - 2026-07-17

### Changed

- Web 面板登录改用 Cloudflare Access (Google) 单点登录, 取代原固定密码框; 需在 Cloudflare 侧配置 Access 后生效. CLI (jjplan / jjask) 认证方式不变.
  - Worker: 新 `authMiddleware` 替换 4 处 `bearerAuth` — 先比对 `Bearer == JJPLAN_TOKEN` (CLI); 未命中且 `CF_ACCESS_TEAM_DOMAIN`+`CF_ACCESS_AUD` 齐备时, 用 `jose` (`createRemoteJWKSet`+`jwtVerify`, 校验 `iss`/`aud`) 验 `Cf-Access-Jwt-Assertion` header 或 `CF_Authorization` cookie; 否则 401. Access env 缺省 = bearer-only, CLI 契约与全部 worker 测试不变. 加 `jose` 依赖; JWKS 按 team domain isolate 级缓存.
  - Web: `api.ts` 删 token 参数 + 不再发 `Authorization` (依赖同源 `CF_Authorization` cookie); `Dashboard.tsx` 删密码登录界面 / `STORAGE_KEY` / token+draftToken+loginError state / `login()`, 挂载即 `load()`, 401→顶部横幅提示刷新重登 (原 `fallbackToLogin`→`onUnauthorized`); `AskSearch.tsx` 去 `token` prop.
  - `wrangler.toml`: 注释说明 `CF_ACCESS_TEAM_DOMAIN` / `CF_ACCESS_AUD` 为可选 vars (非 secret), 未设即 bearer-only.

### Security

- 浏览器不再持有 API token: 面板身份由 Cloudflare Access (Google) 会话校验, token 仅保留在 CLI 侧.
  - `aud` 绑定该 Access app 的 AUD tag → 同 team 其他 app 签发的 JWT 被拒; `iss` 绑定 team domain; JWKS 6 周轮换由 `createRemoteJWKSet` 按 `kid` 自动刷新.

## [0.9.0] - 2026-07-16

### Added

- Dashboard 首页新增 jjask 全局检索: 输入关键词 (空格分隔即多词 AND) 实时检索跨全部 project 的 ask, 命中词高亮, 点击结果直达所属 project 的 asks.
  - Worker: 新增 `GET /asks?q=&limit=` — 空白拆词 AND (`body LIKE ? ESCAPE '\'`, 上限 16 词), `escapeLike` 转义 `\ % _`, 显式列名 (无 origin), `ORDER BY updated_at DESC`, limit 默认 50 / 上限 200; 空 q 返回 `[]`; 认证复用 `/asks/*` 中间件 (Hono 通配符亦匹配裸 `/asks`).
  - Worker tests: auth cases 加 `GET /asks`; 新增 `describe(GET /asks search)` 9 例 (空q / 跨project / 多词AND / ASCII大小写 / CJK子串 / LIKE通配符转义 / 排序 / limit 边界).
  - Web: `api.searchAsks`; 新组件 `AskSearch.tsx` (250ms 防抖 + `reqIdRef` 防乱序 + 正则高亮 `<mark>` + project chip 跳转前预写 ASKS tab + IME 组合输入感知: 组合中不检索, `isComposing` 时 Escape 不误清); `Dashboard` home 用 `AskSearch` 包 `ProjectsList` 作 fallback; `ProjectTabs` 导出 `PROJECT_TAB_STORAGE_KEY`; `types.ts` 加 `ASK_SEARCH_LIMIT_*`; `next.config.mjs` dev rewrite 加裸 `/asks`.

## [0.8.26] - 2026-06-15

### Changed

- 文档体系去双语化 + 精简: 删除全部 `.zh.md` 镜像 (AGENTS/README/deploy/llm-doc-style), AGENTS.md 改中文直写; `deploy.md` 合并重命名为 `workflow.md`; `llm-doc-style.md` 写作规范内联到 AGENTS.md; 旧 `CHANGELOG` (无扩展名) 删除. CLI / Worker / Web 行为零变化.
  - 删除: `AGENTS.zh.md`, `README.zh.md`, `deploy.md`, `deploy.zh.md`, `llm-doc-style.md`, `llm-doc-style.zh.md`, `CHANGELOG` (无扩展名旧文件)
  - 新建: `workflow.md` (合并 deploy.md 内容 + 调试段), `CHANGELOG.dev.md` (本文件, 从 0.8.24 起双文件分界)
  - 重写: `AGENTS.md` 英文 → 中文, 路由指向 `workflow.md`; `README.md` 英文 → 中文精简版

## [0.8.25] - 2026-06-14

### Changed (BREAKING)

- `jjask new` 移除 `--origin` 参数, 只保留 `<body>` 位置参数; API 响应不再包含 `origin` 字段. DB 列保留, 老数据不受影响.
  - CLI: `parseAskNewArgs` 删除 `--origin` 分支, 返回类型去掉 `origin?`; `ask.new` handler 不再构造 `origin` payload.
  - Worker: 删除 `parseAskOrigin`; `AskRow` 去掉 `origin`; `readAsk` / asks list 改显式列名 (`SELECT id, project_id, body, created_at, updated_at`); POST INSERT 去掉 origin 列 (DB `DEFAULT ''` 兜底).
  - Web: `Ask` 接口去掉 `origin`; `AsksView.tsx` / `AskEditDialog.tsx` 删除 origin 渲染块.
  - Tests: 删除 4 个 origin 校验用例, 更新 2 个引用 origin 的用例.
  - 全局 `AGENTS.md` jjask 文档段同步简化.

## [0.8.24] - 2026-06-08

### Changed

- 文档体系对齐 cli-template: 全套双语化 (英文主版 + 中文镜像), 发布记录拆为用户向 / 开发向两份, 自本版起严格分界. CLI / Worker / Web 行为零变化.
  - 新建 `llm-doc-style.{md,zh.md}` (文档写法单源); `CLAUDE.md` 内容重组进 `AGENTS.{md,zh.md}`, `CLAUDE.md` 转 symlink → `AGENTS.md`; `README.{md,zh.md}` / `deploy.{md,zh.md}` 转双语 (deploy 加 TL;DR + CHANGELOG 双文件镜像步骤 + amend/force-push 禁用条件); `CHANGELOG.md` 拆出开发向 `CHANGELOG`, 历史 27 版 (≤0.8.23) 原文未回填.

## [0.8.23] - 2026-05-17

### Added

- 二进制双架构 (macOS x64 + arm64); Release 附 `checksums.txt`, `install.sh` best-effort 校验, 并支持 `REPO` / `VERSION` / `INSTALL_DIR` 覆写.

### Changed (BREAKING)

- 子命令 `self-update` 更名为 `update` (`upgrade` 为别名), 与 [cli-template](https://github.com/yigegongjiang/cli-template) 对齐. 旧名不再识别.

### Changed

- CLI 构建抽到 `cli/build.ts`, finally 清 `*.bun-build`; 自更新加源码守护 (源码运行 `update` 直接拒绝, 走 install.sh 兜底).
- CI 加 `concurrency: release-${ref}` + `generate_release_notes`; install URL 由 `JJ_REPO` 派生.
- 文档体系对齐 cli-template: README / CLAUDE 加 AI-only 声明, deploy.md 四段化, CHANGELOG 加 SemVer 头 + compare 链接尾.

## [0.8.22] - 2026-05-17

### Changed

- **`jjask --help` TLDR 段 `--origin` 文案再次重写, 把改写敏感度从"口语化/含糊就改写"降到"无法直接执行才改写"**: 背景 — 0.8.x 版的"body=改写 (原话口语化/含糊) 则 --origin MUST=原话"把改写说成两条等价路径之一, AI (claudecode / codex) 容易把任何稍微口语化的原话判定为可改写, 改写时又频繁丢失原话细节/重点/数字/路径, 导致后续执行偏离用户真实诉求. 新文案显式声明: 默认 `body=原话原文照搬, 省 --origin (这是绝大多数情况)`; 改写阈值收紧到 `原话本身无法直接喂给后续 AI 执行 (缺主语/无指令/纯情绪宣泄/严重歧义 — 不是"口语化"也不是"含糊", 而是真的执行不了)`; 改写质量强约束 `body MUST 保留原话全部细节/信息/重点/约束/数字/路径/术语, 仅补足执行所需的缺失成分, 不删减/不抽象/不概括/不归纳/不润色`; 兜底原则 `拿不准就照搬原话`. 仅改 `cli/src/jjask.ts` 的 `printHelp` TLDR, USAGE 签名 / COMMANDS 段 / worker / web / schema 不动.

### Notes

- 零 BREAKING. CLI 协议 / worker / 数据库 / web 全部不变, 仅 `jjask --help` 输出文本调整. 用户全局 `~/.claude/CLAUDE.md` 中如果嵌入了旧版 jjask 引导文本, 需同步替换为新版 (本工程不主动修改用户全局规则).

## [0.8.21] - 2026-05-17

### Changed (BREAKING — `jjask` CLI 协议)

- **`jjask` 移除 `--after` 参数, asks 转为扁平记录**: 每条 ask 都是独立记录, 不再支持串链 (无 prev/next 关系). 背景: 实际使用中所有 ask 都是 standalone, 链结构未带来价值, 反而带来 fork 拒绝 (409) / 中间删 rewire 等额外复杂度.
- CLI `jjask.ts`: `new` 的 `--after <prev_ask_id>` 参数与对应解析逻辑删除; USAGE / help text 中 chain / fork / 409 prev 相关条款全部清理.
- Worker `src/index.ts`: `POST /projects/:name/asks` 删除 `prev_id` 入参校验 (跨项目 / 不存在 / 409 fork) 与对应 try/catch; `DELETE /asks/:id` 改回单语句 DELETE, 去掉 successor rewire / OCC CAS / NOT EXISTS 守护; `AskRow` 删 `prev_id` 字段.
- Worker `0004_drop_ask_prev_id.sql`: `DROP INDEX uq_asks_succ` + `DROP INDEX idx_asks_prev` + `ALTER TABLE asks DROP COLUMN prev_id`. 原有 ask 数据 body / origin / project_id / created_at / updated_at 全部保留.
- Web `lib/types.ts`: `Ask` 接口删 `prev_id`. `components/AsksView.tsx`: 移除 `buildChains` / `ChainGraph` 引用与 standalone / chain 拆分逻辑, 改为单纯 grid 渲染所有 ask. `lib/chain.ts` + `components/ChainGraph.tsx` 保留 (`SpecsView` / `SpecDetail` 仍按 spec / task 链渲染).
- Worker tests: 删除 `asks chain` describe 下全部 prev_id 用例 (chain 拒绝 / 跨项目 / 409 / DELETE rewire), 拆为独立的 `GET /asks/:id` / `PATCH /asks/:id` / `DELETE /asks/:id` describes; `rename migrates asks alongside specs` 用例去掉 chain 完整性断言, 仅断言迁移结果.

### Notes

- BREAKING 仅影响 `jjask new --after`; ask 创建/查询/编辑/删除其余语义不变. CLI 旧二进制传 `--after` 直接报 `unknown option`. 本次发版同步 worker + 数据库迁移, 不存在版本错位窗口. 已有数据 body / origin / project / 时间戳全部保留; 链信息按需求丢弃, 不可恢复.

## [0.8.20] - 2026-05-16

### Changed

- **CLI 源码 jjplan / jjask 分文件**: `cli/src/main.ts` 单文件 (运行时用 `JJPLAN_ENTRY` define 分叉两路) 拆为 `cli/src/{shared,jjplan,jjask}.ts` — `shared.ts` 平铺 config / api / io / parse / installer 与各类常量; `jjplan.ts` 含 project / spec / task handlers + USAGE + help + main; `jjask.ts` 同构. 删 `--define JJPLAN_ENTRY`, ENTRY 在各入口顶部硬编码.
- **install.sh 内部循环化**: 接口不变 (`install.sh [install|update|uninstall]`), 内部用 `BINARIES` 数组 + `install_one` / `uninstall_one` 函数提取循环体. `self-update` / `uninstall` 仍同时影响 jjplan + jjask.
- **CI release.yml**: `files: cli/dist/*-macos-arm64` 改用 glob.

### Notes

- 零 BREAKING. 仅源码结构 + 安装脚本内部循环化, 运行行为与 0.8.19 一致.

## [0.8.19] - 2026-05-16

### Fixed

- **恢复浏览器 swipe-back 手势**: 删除 `globals.css` body 上的 `overscroll-behavior-x: none`. 该声明在根级生效, 会同时禁用 macOS 触控板两指左/右滑的浏览器导航手势, 导致全站无法滑动返回上一页. 拦截范围收窄到 `ChainGraph` 横滚容器自身 (`overscroll-x-contain`), 防止 chain 滚到尽头时把横向滚动冒泡给 body. 效果: 在 chain 卡片区横滑只滚卡片; 页面其他位置横滑浏览器 swipe-back 正常.

## [0.8.18] - 2026-05-16

### Added

- **Project 改名入口 (含合并语义)**: Dashboard 首页 `ProjectsList` 卡片 footer 在 `delete` 旁加 `rename` 入口, 点击弹出 `RenameProjectDialog` 单 input 对话框. 目标 name 不存在 → rename (改名); 目标 name 已命中现有 project → 对话框文案与按钮配色切到橙色 "merge" 模式, 提示两个 project 的 spec / task / ask 全部合并到目标, 此操作不可撤销. 解决场景: AI 经 `jjplan` / `jjask` 提交时把 project name 传错 (cwd basename 偶尔被改写), 之前只能 CLI 手动迁移, 现在 web 端直接纠正.
- Worker 新增 `PATCH /projects/:name` body `{ new_name: string }`. rename 路径: 单 D1 batch 中 `INSERT 新 project (复用旧 created_at, updated_at=ts)` → `UPDATE specs.project_id` → `UPDATE asks.project_id` → `DELETE 旧 project`. merge 路径: `UPDATE specs.project_id` → `UPDATE asks.project_id` → `UPDATE target.updated_at` → `DELETE 旧 project`. 合并安全性依赖三点 schema 事实 — `specs.id` / `asks.id` 是 ULID 全局唯一 (跨项目不撞 PK), `uq_specs_succ` / `uq_asks_succ` 是 partial unique index (`prev_id IS NOT NULL`) 且 ULID 唯一保证 prev_id 不撞, 多 head (`prev_id IS NULL`) 在 schema 中合法且 `orderSpecs` 已支持. tasks 经 `spec_id` 间接归属, 不动. 校验: `new_name` 必须 string, 长度 1..128, 不等于当前 name; 旧 name 不存在 → 404.
- Worker 测试 +14: PATCH 全路径覆盖 (rename / merge / 各类 400 / 404 / 链结构保留).
- Web `api.renameProject` + `types.MAX_PROJECT_NAME_LEN=128`. 改名成功后若当前 route 仍指向旧 name, 自动 `navigate` 到新 name 对应路由 (`project` / `spec`), 并触发一次 silent reload 替代本地 splice (合并后两条链折叠成一项, 本地拼接复杂度收益不划算).

### Notes

- 零 BREAKING. CLI 不加 rename 命令 (本次仅 web 入口). 不改 schema, 不引入 `ON UPDATE CASCADE`. 并发写孤儿 (rename / merge 过程中另一客户端正向旧 name 写入) 是已知低概率边界, 不处理.

## [0.8.17] - 2026-05-16

### Fixed

- **SPA 自制路由 scroll restoration 缺失**: web 端是 Static Export SPA (`web/app/page.tsx` 唯一路由 + `Dashboard.tsx` 内 `useState<Route>` + `window.history.pushState` 自制视图切换), 浏览器不会自动归零/恢复 `scrollY`, 导致两个现象 — (1) `ProjectsList` 下滚后点项目进入 `ProjectTabs`/`SpecDetail`, 内容继承上一级 `scrollY`, 不从顶部渲染; (2) 后退时上一级 `scrollY` 不还原, 落到 0 或被 clamp 到 `maxScroll`. 根因 = 自制路由没有补齐 MPA 模式下浏览器免费给的 scroll 行为, 浏览器原生 `history.scrollRestoration = 'auto'` 也救不了 (前进时 scrollY 未归零 → 浏览器记录的 entry baseline 是上一级位置, 返回时按错误 baseline 恢复且错过 React re-render 时机). 修复 = `Dashboard.tsx` mount 时 `window.history.scrollRestoration = 'manual'` (cleanup 还原); `navigate()` 先 `replaceState({...current, scrollY: window.scrollY})` 把离开位置绑到当前 entry, 再 `pushState({scrollY: 0})` 推新 entry, 再 `window.scrollTo(0,0)`, 最后 `setRoute`; `popstate` handler 从 `e.state.scrollY` 读目标 Y, `setRoute(readRoute())` 后用双 `requestAnimationFrame` 等新视图 commit + layout 完成再 `window.scrollTo(0, target)` (单 rAF 会踩在 React concurrent 渲染前, 目标 Y 大于当前内容高度时被 clamp). 滚动容器 = window (commit 05e5169 撤所有 sticky/fixed 后 normal flow), 不引入新依赖, 不替换自制路由.

### Notes

- 零 BREAKING. CLI / Worker / Schema / API 零变化, 纯 `web/components/Dashboard.tsx` 改动. 首次直开 URL (hydrate 路径) 行为不变, `scrollY=0` 自然.

## [0.8.16] - 2026-05-16

### Changed

- **AskCard footer 与其他卡片对齐**: 删除 `AsksView.AskCard` footer 里独占一行的 `ask.id` (ULID), 仅保留 `updated {time}`. 根因 = 历史遗留, ProjectsList/SpecsView.SpecNode/SpecDetail.TaskNode/TaskItem 都不展示 id, 只有 AskCard 露出 — ULID 对人无意义、对 AI 经 `jjask` CLI 拿, UI 无需展示. 同步把 time 样式从 `text-[10px] text-zinc-500` 提到 `text-[11px] text-zinc-400 font-mono` 并加 `updated ` 前缀, 跟 ProjectsList footer 完全一致.

### Notes

- 零 BREAKING. CLI / Worker / Schema / API 零变化, 纯 UI.

## [0.8.15] - 2026-05-16

### Fixed

- **iPhone Spec/Task 卡片单节点横滑**: 与 Ask 卡片对齐, 单 spec / 单 task 不再强制走 ChainGraph 的 `flex+min-w-max+overflow-x-auto` 路径. 根因 = `SpecsView` / `SpecDetail` 把所有节点 (包括 length=1) 都塞进 ChainGraph, 卡片 `min-w-[16rem] max-w-[28rem]` (Spec) / `min-w-[14rem] max-w-[24rem]` (Task) 在 iPhone 视口 (~351px 可用宽) 下被父级 `min-w-max` 撑到 max-w 上限, 触发卡片内横滑且 title `truncate` 被绕过. 修复 = 仿 `AsksView` (0.8.9 已为 Ask 卡片完成的方案): `buildChains` 拆 standalones (length=1) / chains (length>=2), standalones 入 `grid [grid-template-columns:repeat(auto-fill,minmax(min(20rem,100%),1fr))]` 单列 100%, chains 才走 ChainGraph + 节点 `w-[22rem] shrink-0` (= Ask 链节点宽度); `SpecNode` / `TaskNode` 去 `min-w-/max-w-` 限制改 `w-full min-w-0`, title `truncate` 在新 grid 父级下生效.

### Notes

- 零 BREAKING. CLI / Worker / Schema / API 零变化. 桌面端单节点行为变化: 不再 `min-w-max` 横滑, 改为 grid 多列自适应 (auto-fill, 列宽 ≥20rem), 与 Ask 卡片一致. 真链 (length>=2) 行为完全不变 (仍 ChainGraph 横滑 + ChainBadge + fade gradient + border-l-2 起点锚).

## [0.8.14] - 2026-05-16

### Docs

- CHANGELOG cleanup: 0.8.10/0.8.11/0.8.12 是同一 immersive 问题的失败尝试 (fixed header / sticky tab bar / fixed+blur), 均在 0.8.13 完全撤销. 合并这三段冗长描述为 0.8.13 单段最终方案描述, 删除失败尝试细节 (git log 保留事实). 仅 docs 改动, 代码层零变化.

## [0.8.13] - 2026-05-16

### Changed

- **iPhone 顶部 immersive (最终方案: 纯 normal flow scroll)**: header / tab bar 都不固定, scroll up 时跟内容一起滚出 viewport, main 直达屏幕顶部 (status bar 下方). 实现 = `viewport-fit=cover` (`app/layout.tsx` 加 `viewport = { viewportFit: 'cover', themeColor: '#09090b' }`, 让 webview 含 status bar 区域) + 全 normal flow 元素 (`Dashboard` `<header>` = `bg-zinc-950 border-b pt-[env(safe-area-inset-top)]` 自然占顶并把 breadcrumb 推到安全区下方; `<main>` = `py-3 sm:py-6` 无 mt/pt 顶部 offset; `ProjectTabs` tablist 纯 block: `-mx-3 sm:-mx-4 px-3 sm:px-4 flex … border-b`).
- 删除 0.8.8 引入的 `useScrollDirection` + `scrollHidden` 隐藏机制及对应 hook 文件 (`lib/useScrollDirection.ts`). 删除 header / tab bar 的 sticky / fixed / backdrop-blur / transition-translate 全部样式. 删除 `<ProjectTabs scrollHidden>` prop.

### Notes

- 零 BREAKING. CLI / Worker / Schema / API 零变化.
- 0.8.10 / 0.8.11 / 0.8.12 是同一问题的失败尝试 (fixed header / sticky tab bar / fixed+blur), 均在 0.8.13 完全撤销, 见 git log. CHANGELOG 仅保留最终方案描述.
- 失去 scroll-up reveal header / sticky tab bar 的便利: 切 tab / 看 breadcrumb 需 scroll 到顶部. 这是 immersive 的物理代价 (任何 sticky/fixed 都会引入占位, 与 immersive 矛盾).

## [0.8.9] - 2026-05-15

### Fixed

- **iPhone 横向飘晃**: 4 处修复 — (1) `AsksView` standalone grid `minmax(22rem, 1fr)` → `minmax(min(20rem, 100%), 1fr)`, 22rem=352px 在 iPhone SE 视口 ~351px 可用宽下溢出 1px 触发 body 横滚, `min(20rem, 100%)` 让窄屏单列宽=容器宽不溢出, 宽屏 ≥20rem 才起多列; (2) `ChainGraph` 横滚容器 `-mx-4 px-4` → `-mx-3 px-3 sm:-mx-4 sm:px-4`, 严格对齐 Dashboard `<main>` 的 `px-3 sm:px-4`, 修复 mobile 时 chain 容器向外多延 4px 超视口; (3) `globals.css` html/body 加 `overflow-x: hidden` + `body { overscroll-behavior-x: none }`, 任何残余 1px 溢出都不会触发横滑, 同时抑制 iOS 横向 overscroll bounce; (4) Dashboard 顶栏 + `ProjectTabs` tab bar 的 `backdrop-blur` 在 mobile 下移除 (`bg-zinc-950` 纯色), 桌面 `sm:` 保留磨砂 — 修复 iOS Safari 上 `sticky + backdrop-blur + transform` 同时存在时已知的 jitter / 抖动.

### Notes

- 零 BREAKING. 桌面端视觉 0 变化 (backdrop-blur 仅 mobile 退化为纯色). chain 横滚容器 mobile padding 由 16px 缩到 12px, 与 main 一致.

## [0.8.8] - 2026-05-15

### Changed

- **去重 header**: 删 `SpecsView` 内部 `<h3>plans</h3> + N plans · M tasks` 行 / `AsksView` 内部 `<h3>asks</h3> + N asks · standalone · chains` 行 + 整个 `Header` 子组件. ProjectTabs tab bar (`PLANS specCount · taskCount` / `ASKS askCount`) 已承担 section 标识 + 计数职责, 避免 tab 下方紧接一行视觉上几乎相同的文字.
- **沙盒滚动 → 整页滚动**: 删 `ProjectTabs` 容器 `h-[calc(100vh-9rem)] min-h-[20rem]`, 删 `SpecsView` / `AsksView` 的 `<section h-full flex flex-col>` 和内层 `flex-1 min-h-0 overflow-y-auto overflow-x-hidden`. 内容自然伸展, `window` 接管 Y 滚动. ChainGraph 自身的 `overflow-x-auto` 横滚不受影响.
- **mobile scroll-aware 自动隐藏**: 新增 `lib/useScrollDirection.ts` (window scroll 监听 + rAF throttle, scrollY > 60 且 down → `hidden=true`, up 立即 `false`). Dashboard 顶栏与 ProjectTabs tab bar 在 mobile scroll-down 时同步 `-translate-y-full` 隐藏, scroll-up 立即回显, 释放垂直可视空间. 桌面端 `sm:translate-y-0` 永远显示, 行为不变.
- **mobile 紧凑布局**: Dashboard `<header>` 高度 `h-14` → `h-12 sm:h-14`; `<main>` padding `px-4 py-6` → `px-3 py-3 sm:px-4 sm:py-6`; ProjectTabs tab bar `gap-6 py-2` → `gap-4 py-1.5 sm:gap-6 sm:py-2`; tab 计数 mobile 简写 `N · M`, 桌面完整 `N specs · M tasks`.
- **tab bar sticky**: tab bar 加 `sticky top-12 sm:top-14 z-10 bg-zinc-950/90 backdrop-blur` 让其在 window 滚动时贴在顶栏下方, 与顶栏一起 scroll-aware hide. z-index 调整: header z-20 > tab bar z-10 > 内容.

### Notes

- 零 BREAKING. CLI / Worker / Schema / API 零变化. 桌面端布局变化: 内容区不再固定高度沙盒, 长 chain / 多 chain 由整页 scroll 承载, 与 mobile 一致.

## [0.8.7] - 2026-05-15

### Changed

- `ChainGraph` 加 3 层窄屏可感知性 (仅 `chain.length >= 2` 生效, length=1 单 spec 完全保持现状): (a) chain 上方插入一行 header `[链节图标] chain · N items · swipe →`, 首屏即告知"这是 N 节点链", 与独立 item 视觉差异化; (b) 横向滚动容器右沿叠 `pointer-events-none w-8 bg-gradient-to-l from-zinc-950 to-transparent` fade 渐变, 暗示内容向右延伸; (c) chain 第一个 item wrapper 加 `border-l-2 border-zinc-700 pl-1.5` 起点锚, 即便看不到第二个 item 也能识别"链起点".
- 解决 iPhone 视口 ~375-400px 下首个 `AskCard` (`w-[22rem]` ≈ 352px) 几乎占满屏幕、`no-scrollbar` 又隐藏滚动条、用户首屏完全感知不到后续链节点的体验问题. 三处用 ChainGraph (`AsksView` / `SpecsView` / `SpecDetail`) 自动受益, 调用方零改动.

### Notes

- 零 BREAKING. CLI / Worker / Schema / API 零变化. 宽屏不影响 (`min-w-max` 横向布局, fade 仅覆盖右沿).

## [0.8.6] - 2026-05-15

### Changed

- Web project 页 ASKS / PLANS 区由 `SplitPane` 上下拖拽分割改为 **Tab 切换**: 顶部 tab bar 两枚 `PLANS N specs · M tasks` / `ASKS K`, 选中项以 `border-b-2 border-zinc-100` + `font-semibold` 强调, 未选中 `text-zinc-500 hover:text-zinc-300`. 单区块独占内容区高度 (`h-[calc(100vh-9rem)]` 减 tab bar), 解决原 SplitPane 单区最多 90% 的占比受限问题.
- 默认 tab = `PLANS` (jjplan 核心产物), 用户选择持久化到 `localStorage('jjplan_project_tab')`, 值 `plans` | `asks`, 与既有 `jjplan_token` / `jjplan_project_tab` 模式一致.
- 新增 `web/components/ProjectTabs.tsx` (受控 tab 容器 + 计数徽标 + 持久化), 删除 `web/components/SplitPane.tsx` (无其它引用).

### Notes

- 零 BREAKING. CLI / Worker / Schema / API 零变化. 旧 localStorage key `jjplan_split_ratio` 不主动清理 (无副作用, 用户清浏览器即可).

## [0.8.5] - 2026-05-15

### Changed

- Web 控制台品牌名简化: HTML `<title>` 由 `jjplan` 改为 `JJ`; 登录页 H1 由 `jjplan` (text-3xl font-semibold) 升级为 `JJ` (text-5xl font-black tracking-tight), 字号 +2 档、字重至 900、显式高对比白色, 突出极简两字母品牌; 副标题文案 `dashboard · 输入密码以继续` 改为 `console · 输入密码以继续`, 与品牌简化呼应.
- 顶栏面包屑根节点 (登录后所有页面顶部可见) label 由 `jjplan` 改为 `JJ`, 并在 button/span 两种渲染分支下统一应用 `font-black tracking-tight` 高亮样式 (原仅在首页页面态下加粗), 跨页面保持品牌一致.

### Notes

- 零 BREAKING. CLI 名 `jjplan` / `jjask` 不变, Worker/Schema/API/数据模型零变化. 登录页底部"创建 plan / task / ask 请使用 jjplan / jjask CLI"引导文案仍指向真实 CLI 名.

## [0.8.4] - 2026-05-15

### Added

- Web 首页 project 卡片新增 asks 计数: 在原有 `N plans · N tasks` 后追加 `· N asks`, 单复数与既有字段一致 (`1 ask` / `N asks`). 一眼看出每个项目的人机交互记录密度.
- Worker `GET /projects` 响应新增 `asks_count: number` 字段, 用 `COUNT(*) GROUP BY project_id` 一次性聚合, 不嵌入 ask 数组以保持列表接口轻量. 项目无 ask 时计数为 0.

### Notes

- 零 BREAKING. 旧客户端忽略新字段即可; CLI 行为/数据模型零变化.

## [0.8.3] - 2026-05-15

### Changed

- `jjplan --help` / `jjask --help` TLDR 段补 `<project>=cwd basename` 约定: 让 cc (Claude Code) 在交互场景下默认用当前工作目录的 basename 作为 project name, 避免 AI 自由发挥导致 project 名分裂.
- 删 `jjask --help` 三处 "与 jjplan 独立"/"与 jjplan 不同" 强加对比: TLDR / PURPOSE / I/O 段各一处, 改为独立陈述. AI 不需要被告知两者关系, 自会判断.
- `jjask --help` TLDR 段 `--origin` 文案强约束化: 原"决策规则"式两路径并列 (4 行) 压成 1 行强制规则 — `body=改写时 --origin MUST=原话, 不可省`. 修复 cc 改写 body 后漏传 --origin 的高频问题.
- `jjask --help` TLDR 段补 `[--after <prev_ask_id>]` 参数及注释行, 与 `~/.codex/AGENTS.md` (cc/codex 全局规则) 对齐. 原 main.ts TLDR 漏列 --after, 导致 cc 仅读 help 时不知道 ask 可串链.

### Notes

- 零 BREAKING. 用户仍可显式指定任意 project name 覆盖默认. CLI 行为/API/数据模型零变化.

## [0.8.2] - 2026-05-15

### Changed

- Web project 页改为 ASKS / PLANS 上下可拖动分割布局: 新增 `SplitPane` 组件, 默认 50/50, 用户拖动横条调整比例 (clamp 10%-90%), 双击横条重置 50/50. 比例持久化到 `localStorage` (key `jjplan_split_ratio`), 刷新页面沿用上次值.
- 去掉 0.8.1 引入的 ASKS `max-h: 28rem` 固定限高, 改由 SplitPane 控制. ASKS / PLANS 内部各自独立纵向滚动, 不再触发 main 整页滚动.
- 拖动时 `document.body` 临时锁 `user-select: none` + `cursor: row-resize`, 避免选中文本与光标抖动.

### Notes

- 零 BREAKING. API / 数据模型 / ChainGraph / AskCard / SpecNode 不变.
- 容器高度用 `calc(100vh - 9rem)` 减去 header (3.5rem) 与 main padding (3rem) 与余量, 设 `min-h: 20rem` 防止极小窗口下塌陷.

## [0.8.1] - 2026-05-15

### Changed

- Web ASKS 区 UX 重构: 独立 ask (无 prev / 无后续的 length=1 链) 改用 CSS Grid `auto-fill minmax(22rem,1fr)` 多列网格, 1440px 宽屏可排 4 列, 解决原先每张独立卡片独占一行右侧 70% 空白的问题. 真正的链 (length>=2) 仍走 ChainGraph 单行横滚, 链头→尾箭头语义不变.
- ASKS 区整体加 `max-h: 28rem` + `overflow-y-auto`, asks 再多也不会把 PLANS 推下去, 第一屏 PLANS 始终可见.
- ASKS header 计数细化: 同时存在独立 ask 与链时追加 `· K standalone · M chains` 副标签; 单一形态省略副标签避免冗余.
- ChainGraph 横向滚动条 (ASKS 链 / PLANS 链) 隐藏: globals.css 新增 `.no-scrollbar` 工具类 (`scrollbar-width: none` + WebKit `::-webkit-scrollbar { display: none }`), 横向溢出仍可拖动滚动, 仅去掉视觉滚动条占位. 纵向 ASKS 滚动条保留以提示可滚.

### Notes

- 零 BREAKING. 不动 API / 数据模型 / 路由 / 持久化. 仅 `web/components/AsksView.tsx` 内的 className 与渲染分支调整, AskCard / ChainGraph / lib/chain 保持原状.
- AskCard 自身宽度由父级决定 (grid 路径 `w-full`, chains 路径外层包 `w-[22rem] shrink-0`), 行为一致.

## [0.8.0] - 2026-05-15

### Added

- 新增 `jjask` 二进制 (与 `jjplan` 共享一份 CLI 源码), 端点 `/projects/:name/asks` 与 `/asks/:id`, migration `0003_add_asks.sql` 仅 CREATE `asks` 表. Schema/路由/Dashboard 链表与级联沿用 `specs`. `asks` 字段: body (必填) + origin (创建后不可改) + prev_id 单链.
- install.sh 同时安装 `jjplan` + `jjask`, 共用 `~/.jjplan/config.json`.

## [0.7.0] - 2026-05-10

### Changed

- 精简 `jjplan --help` 输出: 162 行 -> 71 行 (-56%), 信息密度优化, 命令行为零变化. 删除 `TYPICAL FLOW` heredoc 示例段、`CONFIG` 路径段、`PITFALLS` (其内容已分散到各命令说明)、推荐 body 模板 (## 背景 / ## 目标 / ## 方案 / ## 兼容性); fork 禁止 / 级联删除 / 中间插入规则集中到 `MODEL` 段一处表述, 各命令说明不再重复; 每条命令统一为「签名 + `->` 返回 + `err:` 错误码」三行结构; `STATUS` 段从 12 行枚举压到 4 行; `EXECUTION` 改名 `BEHAVIOR` 并从 11 行三段式压到 5 条 bullet.

### Notes

- 命令签名、返回 JSON 字段、错误码语义、限长常量、状态枚举与默认值均未变更; 与上一版客户端 / Worker 完全兼容.

## [0.6.0] - 2026-05-10

### Changed

- Web 列表卡片重构 (ProjectsList / SpecsView 的 SpecNode / SpecDetail 的 TaskNode):
  - 宽度从固定 `w-56 / w-48` 改为自适应 `min-w / max-w`, 卡片随标题撑开, 在宽屏下不再过早收窄
  - 标题从 `line-clamp-2 break-words` 改为单行 `truncate` + 原生 `title` tooltip, 极长才省略, 不再过早换行
  - 操作按钮 (edit / delete) 从 `absolute opacity-0 group-hover:opacity-100` 改为底部 footer 与 status 同行, 始终可见, 触屏可达
- 行为零变化, 无 props / API 调整, 仅 className 与 DOM 结构微调.

## [0.5.0] - 2026-05-09

### Changed

- list 排序口径从 `created_at DESC` 切到 `updated_at DESC`. `GET /projects` SQL 加 `ORDER BY updated_at DESC`, `orderSpecs` 链头排序基准跟进, web `buildChains` 删除二次内存排序. 现有客户端无需改动, 列表里"最近活动过"的 project / spec 会自动浮到最前.
- 任意 mutation (POST / PATCH / DELETE × spec / task) 现在级联 bump 父链 `updated_at`: 改 task 同步刷新所属 spec + project; 改 spec 同步刷新所属 project. 每个 mutation 用 D1 batch + shared ts 把"自身写入 + 父级 bump"打成原子事务.

### Notes

- DELETE 路径的父级 bump 加 `NOT EXISTS` 守卫, CAS 失败时 `updated_at` 不被错误推进.
- 历史数据 `created_at == updated_at`, 排序口径切换不会颠倒已有顺序. 无 schema 变更, 无新 migration.
- worker 内部 `buildPatch` 签名从内取 `now()` 改为外部传 `ts`, 让父级 bump 与自身写入用同一时间戳.

## [0.4.0] - 2026-05-09

### Changed (BREAKING)

- spec status 由 `draft | active | done` 简化为 `active | done`. 新建 spec 默认值由 `draft` 改为 `active`. 推荐流变为 `active -> done`. `draft` 与 `active` 在原模型中无任何代码层差异, 仅是文档建议, 删之.
- 旧客户端 `PATCH /specs/:id` 传 `status="draft"` 将返回 400. 升级 CLI / Web 即可.
- migration `0002_drop_draft.sql` 把存量 `draft` 行就地改为 `active`, 不丢数据.

### Notes

- task 状态保持 `todo | doing | done | blocked` 不变. blocked 与其他三态正交, 信息独立, 保留.

## [0.3.0] - 2026-05-09

### Added

- `jjplan task new <spec_id> <title> [--after <prev_task_id>]`: 支持在指定 task 之后插入新 task, 中间插入时原后继的 prev_id 自动重连到新 task (A->B->C, `--after A` 新建 X => A->X->B->C). 不传 --after 维持原行为 (追加链尾).
- `POST /specs/:id/tasks` 接受可选 body 字段 `prev_id`. 协议向后兼容: 旧客户端不传该字段, 走原自动追加路径.
- Web Dashboard: 已登录后每 5s 静默自动刷新, tab 从隐藏切回时立即触发一次, 让 CLI 端改动在数秒内反映到页面. 协议未变.

## [0.2.0] - 2026-05-09

### Changed

- 重写 `jjplan --help` 输出, 使 AI 能从单次帮助调用完整掌握 CLI 能力: 数据模型 (project ⊃ spec ⊃ task)、I/O 协定 (stdin/stdout/exit code)、每命令意图与返回 JSON 形状、状态语义与流转、典型工作流示例、常见陷阱.

[0.9.0]: https://github.com/yigegongjiang/jj-plan/compare/v0.8.26...v0.9.0
[0.8.26]: https://github.com/yigegongjiang/jj-plan/compare/v0.8.25...v0.8.26
[0.8.25]: https://github.com/yigegongjiang/jj-plan/compare/v0.8.24...v0.8.25
[0.8.24]: https://github.com/yigegongjiang/jj-plan/compare/v0.8.23...v0.8.24
[0.8.23]: https://github.com/yigegongjiang/jj-plan/compare/v0.8.22...v0.8.23
