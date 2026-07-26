```When Editing
本文档作用: 工程工作流程 (可用工具 / 调试 / 发布); MUST NOT 写工程说明 (→ README.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 所有段落均为条件段, 根据工程实际决定保留或删除; 存在即为明确流程, MUST NOT 附加强度标记
- 发布内按顺序编号步骤; 顶部 TL;DR ≤ 5 行; 删除子段后重编号保持连续
- 风险点 / 不可逆操作用 `>` 引用块; 高危操作 MUST 标禁用条件
```

# 可用工具

- `gh`: 已登录
- `wrangler` (`npx wrangler`): 已登录

# 调试

```sh
cd cli && cargo build --release && ./target/release/jj-agentic-aspect --version # CLI 验证
```

`--version` 输出须等于根 `VERSION`.

# 发布

代码变更完成后立即执行 (= 需求交付的最后环节). 交付 = 预部署 + push.

push `v*` tag 触发 Actions: 复跑 Worker 部署 (幂等) + 编译 CLI 二进制 (jj-agentic-aspect × x64/arm64) 附 Release.

## TL;DR

1. 验证：`cd cli && cargo build --release` + `cd web && bun run typecheck`
2. 写版本：`VERSION` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致)
3. 预部署：web build + D1 migrations + `npx wrangler deploy` + `./scripts/install-local.sh`
4. 发布：commit + annotated tag (`-m`) + push branch + tag

## 1. 验证

```sh
cd cli && cargo build --release && ./target/release/jj-agentic-aspect --version
cd web && bun run typecheck
cd worker && bun run typecheck
```

`--version` 输出须等于根 `VERSION`. build / typecheck / version 任一失败 → 停止.

## 2. 写版本

- 版本号: 默认递增 PATCH; 新功能 → MINOR; 不兼容改动 → MAJOR
- `VERSION` (唯一信源; CI 校验 `VERSION == tag (去 v)`, 不一致 fail) + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑

## 3. 预部署

本机完成实际交付: Worker + web 上线, CLI 装入本机.

```sh
cd web && bun install && bun run build                    # 静态导出 → web/out (wrangler [assets] 取用)
cd ../worker && bun install
npx wrangler d1 migrations apply jjplan --remote          # 先迁移, 再部署
npx wrangler deploy
cd .. && ./scripts/install-local.sh                       # cargo build --release + 装入 ~/.local/bin
```

- `jjplan` = D1 database label (数据早于改名, 不重命名)
- web build MUST 在 deploy 前完成, 否则 Worker 带旧静态资源上线

> D1 migrations 直接改远程库 schema/数据, 不可回滚; migrations 目录只增不改已应用条目.

## 4. 发布

```sh
git commit -m "X.Y.Z: <一句话>"
git push origin main
git tag vX.Y.Z -m "X.Y.Z"   # tag.gpgsign=true → MUST 带 message
git push origin vX.Y.Z
```

> tag MUST 带 message (`-m`): `tag.gpgsign=true` 会把 lightweight tag 强制升为签名 tag 但缺 message → fail.
