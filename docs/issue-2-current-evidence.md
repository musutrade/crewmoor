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

## Current project entrypoint and remaining acceptance

The new implementation records project placement by core-id plus canonical cwd, permits native creation of owned unknown IDs, and reuses only its own recorded IDs. Explicit existing-space mode is retained. Rust tests execute both Agent branches through synthetic native/RPC transports, including project reuse and preservation of external hosts. Exact candidate SHA, invocation-specific target path, full verify log and artifact/config hashes are supplied with independent audit handoff; no real launch was authorized by this code change.

The new automatic entrypoint still needs its own precise-candidate real acceptance after review and explicit permission. TUI actual startup/location/owned-stop success is still pending; historical 769/v9 TUI withheld input, while its separate core/Codex case passed. The [minimum proposal](issue-2-minimal-acceptance.md) keeps these gaps distinct. New candidate CI must be checked separately by the publisher; PR #5 stays draft with no implied merge/Issue closure. Old core `cm-0c926f847d55` and all user resources remain HOLD.
