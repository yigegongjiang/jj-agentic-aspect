```When Editing
本文档作用: 工程工作流程 (可用工具 / 调试 / 发布); MUST NOT 写工程说明 (→ README.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 所有段落均为条件段, 根据工程实际决定保留或删除; 存在即为明确流程, MUST NOT 附加强度标记
- 发布内按顺序编号步骤; 顶部 TL;DR ≤ 5 行; 删除子段后重编号保持连续
- 风险点 / 不可逆操作用 `>` 引用块; 高危操作 MUST 标禁用条件
```

# 可用工具

- `gh`: 已登录

# 调试

```sh
cd cli && cargo build --release && ./target/release/jj-agentic-aspect --version # CLI 验证
```

`--version` 输出须等于根 `VERSION`.

# 发布

push `v*` tag 触发 Actions: 部署 Worker + 编译 CLI 二进制 (jj-agentic-aspect × x64/arm64) 附 Release.

## TL;DR

1. 验证：`cd cli && cargo build --release`
2. 写版本：`VERSION` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致)
3. 发布：commit + annotated tag (`-m`) + push branch + tag
4. 本机自安装：`./scripts/install-local.sh` (源码 release 构建 → `~/.local/bin`)
5. 修上版 bug：amend + 删远程 tag + 重打 + force push

## 1. 验证

```sh
cd cli && cargo build --release && ./target/release/jj-agentic-aspect --version
```

`--version` 输出须等于根 `VERSION`. build / version 任一失败 → 停止.

## 2. 写版本

- 版本号: 默认递增 PATCH; 新功能 → MINOR; 不兼容改动 → MAJOR
- `VERSION` (唯一信源; CI 校验 `VERSION == tag (去 v)`, 不一致 fail) + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑

## 3. 发布

```sh
git commit -m "X.Y.Z: <一句话>"
git push origin main
git tag vX.Y.Z -m "X.Y.Z"   # tag.gpgsign=true → MUST 带 message
git push origin vX.Y.Z
```

> tag MUST 带 message (`-m`): `tag.gpgsign=true` 会把 lightweight tag 强制升为签名 tag 但缺 message → fail.

## 4. 本机自安装

发布后把刚发布的版本装到本机 (源码 release 构建, 与 tag 内容一致):

```sh
./scripts/install-local.sh   # cargo build --release + 装入 ~/.local/bin
```

## 5. 修上版 bug

上版存在明显 bug 时, amend 修复后重新发布.

> **commit + tag 必须一起更新**: amend 后 commit hash 变, 但远程 tag 仍指旧 hash → Release 产物与 main HEAD 偏离. 只 force push commit 不够, 远程 tag MUST 删除重建, 否则 Actions 不会重跑.

```sh
git commit -a --amend --no-edit
git push --force-with-lease origin main
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag vX.Y.Z -m "X.Y.Z"
git push origin vX.Y.Z
```

> Release 会被覆写, 旧二进制不可恢复.
