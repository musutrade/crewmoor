# Evidence by source and scope

Code review, automatic verification, actual Agent startup, user App observation and owned stop are separate conclusions. Agent semantic status is unknown; these are resource/UI facts, not hooks or business completion.

## a1 existing-space entrypoint

Source `a1f09bb4cc93c48d5d97664b1fdb0c1610fb5650`, runtime v11. Independent code/automatic review passed; the dispatcher/auditor also reported that head's CI passed. This does not cover a later candidate.

The CrewMoor entrypoint started Claude `2.1.316` on existing core `hc 0.1.15`, at the user's Windows App `0.1.10` connection `vps`, space `gem`, tab `1`. Returned `vps/t1y` / `cm-app-claude`, Agent PID `3658935`, start ticks `180933207`. Admission and actual executable/cwd were independently recorded; server/holder/Agent whitelist marker key lists were empty. User confirmed the same new Claude was visible, manual input received a reply, then explicitly said `收到了，观察完成`.

The one authorized stop returned exit 0, `target-stopped-host-retained`: terminal native not-found, recorded Agent exited/PID path absent, terminal binding released. Existing server `2721315`, holder `2721697` and socket inode `92169` were unchanged; selected space/tab retained. Original `t1n` and old held resources were not queried or changed. User's later `显示关闭` was only a UI observation, not a claim that the user manually closed the process.

This is a real **existing-space** App/core Claude entrypoint case. It did not test automatic project placement. Its external evidence files start `app-vps-gem-claude-2316` in the batch evidence directory; the auditor independently checked their hashes and layers.

## Native automatic-Space capability probes

Two separately authorized, sequential probes used an external Rust tool with the official existing-socket JSON-RPC `terminal.run`, `keep=true` and fresh local `herdr.space`/`herdr.tab` IDs. They ran in `/home/gem/crewmoor`, used canonical executables and did not send prompts or tasks. App v0.1.10's [official guide](https://github.com/herdrdev/herdr-app/blob/v0.1.10/defaults/AGENTS.md#L136) and matching source `36725d47c89cd19345994363d6d9cdee5c42c80b` document unknown-ID adoption. The external tool did not invoke CrewMoor's then-existing-space-only entrypoint.

| Probe | Actual process | User statement | Owned stop |
|---|---|---|---|
| Claude `2.1.316`, `vps/t21`, `cm-native-25ecab09` | PID `3688309`, start `181079003`; correct executable/cwd and empty checked markers | Explicitly confirmed a new Space appeared; `可以了，你们关闭吧`. Exact display name/input/reply not separately specified. | One kill/rm, exit 0; native absent and recorded process exited |
| Codex `0.166.0`, `vps/t22`, `cm-codex-25d9316c` | PID `3696391`, start `181202838`; canonical argv plus `--no-daemon`, no shell alias; empty checked markers | Only `观察完成` was supplied in the completion message. No invented itemized visibility/input/reply confirmation. | One kill/rm, exit 0; native absent and recorded process exited |

For both, default core/server/holder/socket were retained. No global layout write, rename or explicit space/tab delete occurred. Single post-stop document snapshots still contained each probe's own space/tab IDs (versions 40 and 42); later native App changes were not polled or guessed. IDs are not display names. Evidence prefixes are `native-auto-space-001` and `native-auto-space-codex-001`; the auditor checked saved files without live reruns.

These probes establish native capability and cleanup facts. They do **not** turn a1 into an automatic-Space implementation or prove the new project entrypoint's real acceptance.

## Accepted project entrypoint and TUI cases

The new implementation records project placement by core-id plus canonical cwd, permits native creation of owned unknown IDs, and reuses only its own recorded IDs. Explicit existing-space mode is retained. Rust tests execute both Agent branches through synthetic native/RPC transports, including project reuse and preservation of external hosts. Exact candidate SHA, invocation-specific target path, full verify log and artifact/config hashes are supplied with independent audit handoff; no real launch was authorized by this code change.

Exact source `bd4d4c975756ff07158699c3869b084d84cadfa0`, tree `c3dabee0e26d01f4d16973776afa19d55b16749f`, was independently audited and verified (387 functions, maximum CRAP 10, 25 quality + 43 business tests). A separate exact-archive build produced v13. Two specifically authorized, sequential real cases completed:

- App project Codex 0.166.0: `vps/t24`, PID 3749679/start 181540415, recorded own project IDs for canonical `/home/gem/crewmoor`. User said `我看到了，已经给它发了一条消息` and `收到回复，观察完成`. GUI visibility/manual input/reply were confirmed; exact Space display name was not separately supplied. Owned stop exit 0, `target-stopped-host-retained`, native terminal absent, recorded process exited, terminal binding released. Project mapping and external host/server/holder/socket were retained unchanged.
- Private TUI Claude 2.1.316: official `agent start`, session `cm-bb433408cd85`, pane `w1:p1`, PID 3750556/start 181542737. Admission and process location were confirmed. Owned pane/workspace/empty-session stop exit 0; recorded Agent/shell/server PID paths, socket and binding absent.

Both cases used one launch, one inspect and one owned stop; checked environment marker key lists were empty. No automatic model task/trust input, Agent-state inference or retry was added. This is not a four-combination live matrix, a real repeated-project-reuse test, or an unknown/timeout-path test.

The dispatcher/publisher confirmed PR #5 merged as `59cc3e319e5cb2755aa4fada8eba887e785cc1a6`, post-merge main CI run 41270646748 attempt 1 succeeded, and Issue #2 closed. No new #2 acceptance is required. Compact retained evidence and cleanup manifest are under `/home/gem/.local/state/crewmoor/crewmoor-issue-2-001/`; temporary binaries/worktree were removed after completion. Old core `cm-0c926f847d55` and all user resources remain HOLD.
