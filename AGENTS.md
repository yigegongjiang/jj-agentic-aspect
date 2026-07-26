# AGENTS

> 当前 project 是 AI Only 工程，人类输入的所有需求，都需要 AI(Claude Code、Codex...) 自主完成后续的所有工作。
> 默认 MUST(必须强制遵守); 仅例外用 SHOULD/MAY/MUST NOT/NEVER 标注。

## 指令路由（MUST）

- 【MUST】代码 / 架构 / 命令 / 结构 → [README.md](./README.md)
- 【MUST】调试 / 版本 / changelog / git commit+push / 预部署+发布 → [workflow.md](./workflow.md)

## 工作模式 (MUST、AI-only)

- 【MUST】代码 / 测试 / 构建 / 部署 / 发布 全部由 AI(Claude Code、Codex、...) 执行，人类仅输入原始需求 + 需求完成后的验收
- 【MUST】workflow.md 中的所有操作（git push / deploy / publish 等）已预授权，AI 直接执行，MUST NOT 等待人类确认
- 【MUST】决策 (架构 / 选型 / 命名 / 依赖 / ...) 需 AI 自行抉择, MUST NOT 拉人类进设计回路，MUST NOT 反问人类
- 【MUST】决策前通过各种方式（本工程检索、在线检索、其他途径）获取项目/需求事实，根据事实进行最优决策

## 工作流程

1. 分析人类输入的原始需求
2. AI 抉择并执行后续工作；开发过程中按需走 [workflow.md#调试](./workflow.md) 验证变更
3. 执行 [workflow.md#发布](./workflow.md) 完整流程；未发布上线 = 需求未完成，MUST NOT 在发布完成前停止

## 文档编写规范

- 全部文档只供 AI 查看，MUST 简洁精炼, 零冗余; MUST NOT 废话填充
- 能一行不写两行, 能一个单词不写两个单词, 能列表不写段落; 短句; `->` `/` `+` 替连接词
- 强度词: MUST / MUST NOT / SHOULD / MAY / NEVER
- 单一信源: 跨文档用 link 引用, MUST NOT 复述事实
- AGENTS 只写 LLM 约束, MUST NOT 塞工程说明 / 命令 / 安装
- 本段 = 全局写作标准; 其他 md 的 When Editing 仅补充各自特有约束

## 硬性规则

- **稳定优先**: `jj-agentic-aspect` 是 rock-stable 追踪系统; 稳定性优先于功能扩展 / UI 质量 / 架构抽象 / 重构
- **契约 vs 内部**: CLI/API 行为 = 稳定契约; 数据结构 = 内部实现. 变更前确认当前行为, 变更后保持兼容
- **自举**: 维护本仓库 MUST 通过 `jj-agentic-aspect plan` 自身 (创建/拆解/追踪 Spec/Task), 不在 chat 或散落 md 中; 摩擦 = 缺陷, 优先修复
- **边界**: 不引入 Spec/Task/Ask 之外的产品模型 (除非用户明确要求); 不加额外运行时 / 服务形态 / 存储 / 后台守护; 默认稳定、窄、可维护
- **平台**: macOS only (x64 + arm64); `install.sh` 和 `update` 主动拒绝其他 OS/arch
- **版本一致性**: 根目录 `VERSION` 文件 = 唯一信源; `cli/build.rs` 注入二进制为 `JJ_VERSION`; Actions 首步校验 `VERSION == tag (去 v)`, 不一致则失败
