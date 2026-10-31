# Start, locate and end an owned Agent

Issue #2 is limited to request admission, actual Agent process evidence, instance/logical-role/native-handle location and controlled end of owned resources. It does not normalize or wait for ready/idle/working/done, implement Agent feedback, send tasks, manage messages/SQLite/worktrees, or manage App membership. Linux and latest Rust stable/edition 2024 only.

```sh
CARGO_TARGET_DIR="$PWD/target" cargo +stable build --locked --bins
CARGO_TARGET_DIR="$PWD/target" cargo +stable run --locked --manifest-path tools/quality/Cargo.toml -- verify
```

Root `Cargo.toml` is the business crate: fmt, policy, Clippy `-D warnings`, tests, fresh LLVM coverage and every-function CRAP <=10 apply. Every developer/auditor invocation uses its own absolute target, inherited by all Cargo/LLVM children; old shared-target results are invalid. CI uses the same Rust verify entrypoint. Tests use owned Rust transport processes and a private `--noprofile --norc` Bash for shell-boundary checks, never real Agents.

## Small configuration

Edit an example in `examples/`, using a fresh team/state root, absolute cwd/store/runtime binary/Agent executable and exact version strings. Only `agent = "codex"` or `agent = "claude"` is accepted. Other kinds, prompts, subcommands, print/background mode, permission bypass options and unknown arguments are rejected.

| Kind | Checked installed version | Interactive argv |
|---|---|---|
| Codex | `codex-cli 0.166.0` | canonical executable, `--no-daemon`, configured options |
| Claude | `2.1.314 (Claude Code)` | canonical executable, configured options |

Both accept `--model NAME`; only Codex accepts `-m NAME` and `--no-alt-screen`. Claude never receives Codex flags. Version is queried from the configured executable; Native launch resolves that file before recording/sending argv. Kernel executable path must match its canonical path, with exact PID/start ticks and cwd. Startup may read existing credentials/config/plugins/MCP and use upstream network/cost; launch authorization must cover that. No zero-cost claim, installation or proxy/config change is implied.

```text
crewmoor launch /absolute/config.toml /tmp/new-owned-state
crewmoor inspect /tmp/new-owned-state
crewmoor stop /tmp/new-owned-state
```

The state root must not exist, be absolute and <=45 bytes. Exclusive binding prevents another instance with the same team in that store. Keep one binding namespace for teams managed by this CrewMoor installation. A random native session and owner token isolate each instance; native handles are scoped to it. State/logs remain after a successful stop; business cwd and shared caches are not removed.

## Two startup paths

Core starts a new named `HC_SESSION` with private XDG config/state and Unix socket, then `run --bg --no-parent --no-notify --name ROLE --cwd CWD --label ... -- <argv>`. No TCP port, parent team or default-session replacement. Full `get` completes the compact CLI launch response; native owner/team/generation/argv/cwd and the sole terminal inventory must agree. Core status is used only for server/holder resource identity.

TUI creates a private named server, workspace and pane, then records the original shell's kernel fingerprint. It uses `pane run PANE ONE_COMMAND_STRING`, not `agent start` or `agent get`. TUI 0.9.5's `agent start` forces an interactive-ready wait; that semantic gate is intentionally removed. The installed `pane run` helper queues text+Enter, exits 0 without stdout on admission and does not prove execution. Only that known admission path accepts empty stdout. One immutable startup command is sent, no prompt/task.

Before sending, binding/server/socket, native pane/terminal/workspace, owner label, one workspace/tab/pane, original shell fingerprint and unique shell foreground/group must all agree. This TUI implementation requires the recorded shell executable to be Bash. The single command directly invokes the quoted absolute executable with quoted argv, without a PATH lookup or `command` prefix. Rust encodes every argument byte inside Bash ANSI-C hex quotes; spaces, quotes, newlines, terminal control bytes and shell metacharacters stay data and are not sent as raw terminal controls. NUL is rejected. Tests compare Rust argv and `/proc/self/cmdline` after real Bash execution, including malicious-looking synthetic values and a shadowed `command` function. Local shell configuration is trusted: Bash can also define an absolute-path-named function, so direct invocation is not a hostile-shell sandbox. Independent kernel executable/cwd/process checks remain necessary.

Missing shell/foreground evidence withholds input: `request_issued=false`, `accepted=null`, process unconfirmed and a diagnostic. It is not a native rejection or a successful Agent startup. A later stop cannot adopt a shell whose birth fingerprint was never captured.

All native CLI/version calls are bounded at 10 seconds, with up to one additional second to reap a timed-out owned CLI child. Anonymous files capture output without a pipe-reader deadlock. Timeout/failure never resends a startup request. TUI preserves admission as unknown and performs the single process observation; process evidence may succeed independently. No ready wait or process/state polling is added.

Source fixed to TUI [v0.9.5 pane run](https://github.com/herdrdev/herdr/blob/v0.9.5/src/cli/pane.rs#L1039) and [silent admission helper](https://github.com/herdrdev/herdr/blob/v0.9.5/src/cli.rs#L755), plus installed CLI help/bundled schemas. Core uses installed 0.1.15 help/schema and archived native responses; fake responses do not invent public Terminal fields.

## What the receipt proves

- Launch argv is completely built before the final ownership guard and request intent. `request_issued` is saved immediately before the one mutation; a known argument-build failure leaves it false. Failure/timeout after the call begins cannot prove the input was unsent and never allows retry. `accepted=true` confirms native request/CLI admission; `null` means no confirmed admission. Neither implies a running Agent.
- `agent_process` records the first verified Agent PID/start ticks/executable. `process_verified` says the current observation confirmed that process and cwd. Missing evidence sets it false while retaining the earlier fingerprint; a changed process is never silently adopted.
- Config `team`/`role` is CrewMoor's assigned logical identity, bound through its owner token, native ids and process evidence. It is not a claim that an optional native Agent name was reported. `native_name=null` means unknown; core name, if present, must match. TUI does not consume AgentInfo at all.
- `original_shell` is captured before TUI input. `core_holders` contains native holder instance ids and PID/start/executable fingerprints captured before and after the single core Agent request, even if it failed. `null` means unknown; `[]` is a proven empty pre-target inventory.
- `phase=observed` only means the resource observation completed. A receipt with `process_verified=false` is not a confirmed launch. There is no `ready` field or unified Agent semantic state. Raw native observations may still contain uninterpreted protocol fields.

Optional core `holder` may be omitted: required `holders` supplies the single current holder. If the alias is explicitly supplied, including null, it must agree with the inventory. Multiple/changed/non-current holders fail; target access requires exactly one. An empty holder inventory is only valid before target creation/server-only cleanup. Terminal `name` may be missing/null without weakening ownership. Public Terminal has no holder field.

Pane/process data must reference the owned pane; Agent lookup verifies the selected kind's process name (or canonical executable basename), kernel executable/cwd and unchanged fingerprint. Missing shell/foreground/PID fields provide no process proof. They never authorize cleanup.

## Controlled end

Stop needs current native owner/ids and exclusive inventories, original shell plus the recorded Agent process as applicable, holder fingerprints, socket inode and team binding. Present mismatch, extra resources or missing necessary evidence refuses before destructive calls. TUI can close its original empty shell only when no startup request was issued, no Agent fingerprint was recorded and the original shell fingerprint/foreground/group is verified. Once a request was issued, stop requires the persisted Agent fingerprint, `process_verified=true`, and a matching current foreground/kernel executable/cwd. Stop never discovers and adopts a replacement Agent or shell or repairs an unverified observation implicitly.

This implementation conservatively refuses a requested Agent that was never observed, has disappeared, has returned to the original empty shell or currently lacks process evidence. Even a previously verified Agent's observed exit is not a separate automatic cleanup result here. The birth fingerprint and binding remain, the outcome is unknown, and manual recovery may be needed. Empty-shell presence alone never becomes a successful controlled Agent stop.

Native target not-found must confirm removal; timeout is not absence. Server, Agent/ending process, original TUI shell and all recorded holders must have exited, and the API socket must be absent before releasing the binding. PID reuse is distinguished by start ticks. Unknown outcomes retain binding/evidence.

A failure before any target identity exists can end only a strictly verified empty native inventory with persisted server/socket/container/binding ownership and core holder inventory. Partial/unknown identity requires human recovery; no generic recovery framework or force cleanup is provided. A per-state operation lock prevents overlapping CrewMoor operations, not outside user/App changes.

## Environment and real acceptance

Every new native child uses the explicit denylist in `native::sanitize`: parent Codex conversation/tool/build/CI markers, ambient Herdr/core routing/scope markers, and inherited terminal/CI/color markers. New core session/socket selectors are then set for that instance. `PATH`, `HOME`, `CODEX_HOME`, authentication, SSH agent, Cargo/Rustup settings, `HERDR_BIN_PATH` and every proxy/bypass variable stay untouched. No global file, proxy environment or existing service is edited. Local executables/config/shell files remain trusted inputs.

Rust parent/child/descendant tests check concrete synthetic removed/preserved keys. The read-only evidence binary reports only whitelisted marker names for receipt-owned processes, checking fingerprints before/after access; values and unrelated environment entries are not retained or printed.

Automatic tests cover TUI/core × Codex/Claude. After exact-SHA code audit pass, the authorized v9 core/Codex attempt proved actual startup, location, the whitelisted environment boundary and complete owned stop. The TUI attempt withheld startup input because the owned shell still had extra foreground processes; its unused shell/server were safely ended, but no TUI Agent started. Both real attempts are consumed; no retry is authorized. Claude entrypoint real startup/stop is unverified; the user's App Claude is background evidence. App Agents-panel membership, Windows build and Agent ready/working/done are not Issue #2 acceptance gates. No Windows/App change is included. The old core/session and all user objects remain under HOLD.

See [historical evidence](issue-2-runtime-evidence.md), [the consumed bounded plan](issue-2-next-real-acceptance.md) and [actual v9 results and the TUI proposal](issue-2-v9-real-acceptance.md). The proposed shell change has not been implemented or used for a real launch. Further real TUI testing requires a concrete new authorization and exact-candidate audit.
