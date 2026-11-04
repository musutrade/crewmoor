---
name: crewmoor-team-flow
description: 使用 crewmoor team start/bind 与 message send，在用户直接面对的团队 dispatcher 和 worker 之间主动派工、交接、回报；处理原生 Hook 通知与会话变更，适用于 Herdr TUI 和 Herdr App/core。
---

# CrewMoor 团队协作

CrewMoor 提供团队创建、固定双端 binding、消息传输和原生会话核对；Agent
完成业务并主动回报。只设一层 dispatcher：用户直接在 `team start` 创建并记录的
dispatcher pane / terminal 下达任务。启动方完成交接后退出调度职责。

## 1. 启动身份与人类入口

```text
crewmoor team start /absolute/team.toml /absolute/state
crewmoor team bind /returned/team_dir WORKER_ROLE
```

使用 `team start` 返回的 `team_dir` 和 `roles[]`，按配置中的 `dispatcher` 角色名
选择其返回 `handle` 与 `feedback_instance`，不要从侧栏顺序、示例或环境变量猜。
启动方把 CLI、团队目录、角色身份和 worker binding 路径交给该 dispatcher；用户
随后直接在其返回的 pane / terminal 输入自然语言，不需要另建用户角色、自发自收
消息或 binding。不保留外层调度 Agent，不收养既有会话。

`team bind` 需要两端原生 SessionStart 已关联各自 session。它为每个 worker 写入
一次 `TEAM_DIR/roles/WORKER_ROLE/binding.json`，之后请求与回复都复用该文件；
不要再用独立的 `message bind` 覆盖或重复创建。缺少 session/binding 时报告具体
缺项一次，不轮询、不盲目重复启动、不自动 rebind。启动受理、进程位置和退出事实
只用于资源定位，不等于 Agent ready 或业务完成。

Codex 的 `initial_prompt` 使用原生启动 argv 传递一次启动说明，之后才进入团队消息
协议；core 模式同样如此，不依赖 `HERDR_ENV`。TUI / App/core 使用同一角色与消息
约定，不在不同容器间另造 Agent 状态适配或恢复层。

## 2. 业务消息是平铺 JSON

```json
{
  "id": "request-001",
  "from": "lead",
  "to": "dev",
  "reply_to": null,
  "content": "实现已分配需求，完成验证后主动回报准确提交和证据。"
}
```

- `id`：本批唯一的新消息 ID；传输层不提供自动去重保证。
- `from` / `to`：该 binding 已登记且互不相同的角色名；dispatcher 不必命名为 `lead`。
- `reply_to`：回复所针对的原消息 ID；新请求可省略或为 null，不填写角色名。
- `content`：非空、至多 16384 字节的 UTF-8 文本。

发送者把平铺 JSON 写入本批消息文件，再用任务给定的准确 CLI / binding 路径发送：

```bash
"$CREWMOOR" message send "$BINDING" "$MESSAGE_FILE"
```

不要把接收时的 `{"team":...,"message":...}` 外层信封作为发送文件。收到消息后
由 Agent 处理业务，主动通过同一 binding 回复，例如：

```json
{
  "id": "reply-001",
  "from": "dev",
  "to": "lead",
  "reply_to": "request-001",
  "content": "实现已提交；head、验证日志、真实验收和剩余限制见交接材料。"
}
```

回复使用新 `id`，`reply_to` 保持原请求 ID。CLI 检查标识符格式及其不等于自己的
`id`，不维护消息历史或自动核实关联；正确关联由 Agent/本技能负责。

`transport: accepted` 只代表原生发送受理，不等于接收、执行或完成。实际收件依靠
接收方主动回报。结果不明时保存原请求与收据，报告一次，不自动重发。普通业务
消息只走 `crewmoor message send`，不绕过它使用 `herdr agent prompt`、`hc send`
或终端输入。它们可以是 CrewMoor 内部传输实现，不是另一条团队业务通道。

## 3. 主动交接与证据

dispatcher 是唯一派工者。worker 等待有效任务后执行，只按已授权范围推进；长任务
主动报告发现、限制和下一步，不把等待变成反复读取对方文件或状态。

完成或需要修正时，交接至少包含：需求、base/head 完整 SHA、目的、主要文件、
验证日志、真实验收与剩余限制。区分传输受理、本人实际收件、自动测试、独立审计、
真实原生验收和 CI；不得相互代替。没有可推进事项时结束本轮，等待主动消息。

## 4. Hook 通知与避免循环

Agent 状态仅以实际版本原生 hooks 提供的事实为准。未提供时报告 unknown，
不从终端内容、进程存活或容器检测猜 Agent 状态。业务完成依靠 Agent/skill 主动
回报，Stop、SessionEnd、模型轮次结束或进程退出均不等于业务完成。

已安装的用户级 callback 收集已注册实例的原生事实。`team bind` 为 worker 写入
`binding-ref.json`，其 Stop / SessionEnd 和 session rejection 通过固定协议转交
dispatcher。无需手动伪造 Hook 或重复发送已自动转发的通知。

dispatcher 没有 worker `binding-ref.json`，自己的事件保留在本地，不向不存在的
上游 Agent 转发。`team bind` 不再写入专用于自动交付的
`dispatcher-binding-ref.json`；遗留文件不会被 callback 使用，也不自动清理。
用户在 dispatcher 本人的 pane 中接收业务进展和异常说明。

Stop Hook 不查询 GitHub、不选择或交付下一项需求；通过这条 Hook 路径自动接续
的能力暂停，承接方案待 [#51](https://github.com/musutrade/crewmoor/issues/51)
讨论确定。本步不提供 watcher、后台服务或替代触发机制。业务 Agent 仍可显式使用
`issue assign/current/done/next`。保留的 worker 转发预算限制仍待 #45 处理，
不把本次移除视为全部超时问题已解决。

Stop/SessionEnd 是通知，没有业务动作时不回复。确需派下一项或确认一次收件时，
每条通知至多回复一次，不建立通用 ACK 循环。worker 仍须主动发送业务交接。

## 5. Session 变化

实例标记、Agent 类型和不可变原生 session 关联事件；cwd 只辅助首次未知 session
的主 SessionStart，后续切换目录不等于换会话。无实例标记、不存在的实例或错误
Agent 类型不进入日志；已关联且 Agent 匹配的拒收输入可留下固定类别的
`hook-rejected` 事实，不因此建立会话、转发或 halt。

重启、重新登录或 `/clear` 等操作使原生 session 改变时，`feedback show` 中的
`session-rejected` 保留新旧 session，原关联和 binding 不被替换。涉及该角色的
普通 bind/send 均拒绝。worker 的 halt 通知只对未变化的 dispatcher 使用受限
通知路径；dispatcher 自身变化时不承诺上游通知，用户直接查看其事实。

dispatcher 停止向受影响角色派工，向用户一次说明角色、新旧 session 和拒收事实。
不自动恢复、rebind、重启或收养新会话，等待用户决定。不要给已 halt 的 worker
再发送确认消息。已明确授权的独立验收资源按其测试计划清理，不影响真实团队。

```text
crewmoor feedback show REGISTRY INSTANCE
crewmoor team stop TEAM_DIR
```

只结束当前任务明确拥有、返回身份仍匹配的资源；保留现有容器服务和用户资源。
`feedback show` 是诊断入口，不是轮询调度机制。

## 6. 本批记录与本地技能同步

使用任务 brief 中给定的绝对日期，业务记录、消息和证据写入本批目录。
不读写用户/调度 memory（包括 `~/.codex/memories/`、
`~/.claude/projects/*/memory/`），不修改 `~/.codex/.env` 或其链接目标，
保留用户认证、工具和代理设置。独立开发/审计 worktree 与 Cargo target 按任务分配。

- 批次状态统一放 `~/.local/state/crewmoor/batches/<batch>/`；brief、团队配置、决策、消息、证据及 dev/audit/release worktree 都在该批次目录下。`teams/`、`feedback/`、`target/` 保持原位。
- 仓库 `docs/` 只保留工具文档；历史 issue 验收证据移至对应批次的 `evidence/`，引用通过索引说明存放位置，不把证据重新写回仓库。
- 移动含已登记 worktree 的批次时使用 `git worktree move`，不直接 `mv`；结束后核对 worktree 登记和干净状态，不 prune/reset/clean/remove 或删除分支。

本文件是受版本审查的技能源码。发布它不会自动修改本机已安装技能。需要同步时，
先记录准确目标路径、旧哈希、已审查 source revision/新哈希和备份/恢复路径；
在该 revision 已审查并获准使用后，逐文件备份、比较、复制和核验，不创建新的
链接关系，不静默覆盖同期改动，不引入安装框架或修改其他 Agent 配置。
