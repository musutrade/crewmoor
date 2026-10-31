# CrewMoor 项目公约

## 1. 先读官方 LLM 入口与文档

接入 Herdr 前，先查当前任务相关的官方说明，再核对实际安装版本的 `--help`、`--skill`、schema；有歧义时查对应版本源码。不要凭印象设计接口或用测试替身虚构字段。

| 产品 | 官方 LLM / Agent 入口 | 官方文档 |
| --- | --- | --- |
| Herdr TUI | [LLM 索引](https://herdr.dev/llms.txt)、[完整 LLM 文档](https://herdr.dev/llms-full.txt)、[Agent 指南](https://herdr.dev/agent-guide.md) | [文档首页](https://herdr.dev/tui/docs/)、[Agent 自动化](https://herdr.dev/tui/docs/agent-automation/) |
| Herdr App | [官方 AGENTS.md](https://github.com/herdrdev/herdr-app/blob/master/defaults/AGENTS.md)（[原文](https://raw.githubusercontent.com/herdrdev/herdr-app/master/defaults/AGENTS.md)）；优先读安装版本生成的 `~/.config/herdr-app/AGENTS.md` 或该平台实际配置目录对应文件 | [官方 Guide](https://herdr.dev/guide/)、[官方源码](https://github.com/herdrdev/herdr-app) |
| herdr-core（App 的运行底座） | 安装版本的 `hc --skill`、`hc --help`、`hc schema` | [官方入口](https://herdr.dev/core/)、[命令文档](https://github.com/herdrdev/herdr-core/blob/master/docs/commands.md) |

Herdr TUI 的 LLM 索引当前位于站点根路径，不要猜成 `/tui/llms.txt`；Herdr App 使用官方 Agent 指南，不把 TUI 的索引当成 App 文档。在线默认分支可能领先安装版本，引用实现结论时记录实际版本或源码 revision。

## 2. 对接容器，以容器直接提供的能力为准

优先直接使用官方创建、启动、定位、等待和关闭接口；已有等待、启动或资源管理能力直接复用，不在 CrewMoor 再造同层机制。

先给出完成当前需求的最短官方调用链和返回身份，再实现必要的薄层配置与记录。只有发现并证明当前需求确有能力缺口时，才讨论最小补充；不为假想未来需求预建统一适配、通用恢复或多层守卫。

对本次新建的专用 pane/terminal，记录容器实例与返回句柄，定位和结束只作用于本次明确拥有的资源。不要套用管理任意已有会话的复杂流程，不误关用户既有会话或含用户资源的实例。启动请求受理与实际进程启动分开报告，结果不明不盲目重发。

保留子进程环境隔离，避免把调度/开发会话标记传播到新容器；保留用户认证、工具与代理配置，不修改 `~/.codex/.env` 或其链接目标，不擅自统一代理或切换直连。

## 3. Agent 自身已有能力直接复用

当前仅适配 Codex 和 Claude；其他 Agent 暂不接入，不预建通用插件框架。Agent 原生启动参数、会话能力、任务执行及反馈能力能够满足需求时直接使用，不在容器适配层重新实现。

Agent 的所有状态以 Agent hooks 提供的状态为准；直接复用 Codex、Claude 已有的 hooks 能力，不从终端输出、进程存活或容器状态推断 Agent 状态，也不为不同容器另建状态适配。hooks 未提供状态时只报告未知，不补猜状态。容器句柄、进程启动与退出事实仅用于资源定位和安全结束，不代替 hooks 状态。

只记录实际版本的 hooks 能提供且语义明确的状态，不为凑齐状态增加容器推断或 ready 门槛。Agent 的业务进展和完成结果仍由 Agent/skill 主动反馈；会话启动、轮次结束或进程退出不等于业务完成。

hooks 状态接入见 [Issue #4](https://github.com/musutrade/crewmoor/issues/4)；Issue #2 只负责启动、定位和自有资源安全结束，不增加 ready 门槛、状态反馈协议或 hooks 实现。

## 已有实施约束

全部实现及测试驱动使用 Rust 最新稳定版，不做旧版兼容。沿用 CRAP ≤ 10、非测试代码禁止 `unwrap()`、零 Clippy 豁免的质量门禁。并行验证使用各自独立的 `CARGO_TARGET_DIR`，不清理共享构建目录。

各角色通过 herdr-team-flow 主动交接与回报，不互相轮询。GitHub 操作显式指定 `musutrade/crewmoor`，避免因当前目录误操作其他仓库。用户最新范围决定优先，公约不扩大安装、真实测试或发布授权。
