# Minimal Agent startup and owned cleanup

Issue #2 selects Herdr TUI or core, cwd, team/role and Codex or Claude. It records request admission separately from actual process evidence, returns the native target, and ends only resources it created. All Agent semantic status is `unknown`: hooks belong to another Issue. Container ready/idle/working values, terminal output and process existence never become Agent status or business completion.

## Official interfaces first

The [official TUI LLM index](https://herdr.dev/llms.txt) currently pins stable 0.9.5. Installed help/skill/schema and versioned [automation documentation](https://github.com/herdrdev/herdr/blob/v0.9.5/docs/next/website/src/content/docs/agent-automation.mdx) were checked before this simplification. Core is installed `hc 0.1.15`; its own `--skill` documents run/get/kill/rm. Online defaults may be newer than installed software.

| Runtime | Minimal native chain |
|---|---|
| TUI 0.9.5 | private server → workspace create → returned root pane → agent start → one process-info → pane close → empty owned workspace close |
| core 0.1.15 | private named Unix-socket core → run → returned terminal → get → kill/rm |
| App on core 0.1.15 | confirmed target + project cwd → recorded project IDs (or explicit existing IDs) → run with keep/adoption labels → get → owned kill/rm; host retained |

TUI calls `agent start ROLE --kind codex|claude --pane RETURNED_PANE --timeout 5000 -- AGENT_ARGS`. Its existing shell-busy handling and internal readiness wait are reused. CrewMoor does not add a busy loop, ready gate or status adapter. This installed CLI has a two-second busy-retry window; that is not an end-to-end two-second deadline. A slow/unavailable shell may still produce an unknown launch.

The official start request runs the kind's executable name plus passed arguments ([server implementation](https://github.com/herdrdev/herdr/blob/v0.9.5/src/app/agents.rs#L185)); it has no executable-path override. Configuration pins an executable/version for preflight and checks the resulting kernel executable/cwd once. An unexpected executable is not adopted.

On this machine the existing Codex alias already supplies `--no-daemon`. The TUI example passes only `--no-alt-screen`; it does not repeat the alias flag. A plain installation without that alias can explicitly configure `--no-daemon`. Core invokes the configured canonical executable directly and adds `--no-daemon` once. Claude never receives Codex flags. Only model selection, these supported Codex flags and Issue #15's single Codex `initial_prompt` are allowed; no other task/prompt, shell startup mutation or authentication command is generated. TUI rejects control bytes as its official API does. The container performs argument quoting; CrewMoor no longer builds shell command strings.

## Build and use

Linux, latest Rust stable, edition 2024; no MSRV fallback:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo +stable build --locked --bins
CARGO_TARGET_DIR="$PWD/target" cargo +stable run --locked --manifest-path tools/quality/Cargo.toml -- verify
```

Every invocation uses its own absolute target directory, inherited by all Cargo/LLVM children. Root `Cargo.toml` is a business crate, not N/A. Fmt, zero-suppression policy, Clippy `-D warnings`, actual tests/LLVM coverage and per-function CRAP ≤10 all remain required. No shared target is cleaned.

Choose an example from `examples/` and pin the installed versions. Current inspected versions are TUI 0.9.5, core 0.1.15, Codex 0.166.0 and Claude 2.1.316. Both kinds accept `--model`; only Codex accepts `-m`, `--no-alt-screen` and `--no-daemon`.

```text
crewmoor launch /absolute/config.toml /tmp/fresh-state
crewmoor inspect /tmp/fresh-state
crewmoor stop /tmp/fresh-state
```

State must be new, absolute and ≤45 bytes for Unix socket paths. Private mode binds each team to one randomly named instance. Optional [App binding](app-launch.md) selects a confirmed existing target (inline or `app_target` file) and owns only its terminal. With both space/tab IDs omitted, core-id + canonical cwd identifies the persisted project placement; no manual Space setup or ID lookup is needed. Explicit existing IDs remain supported. Keep one store per installation. The receipt records team/role, project ownership/IDs, container/native handle, admission and verified process facts; configured role is not invented native evidence.

A startup intent is saved before the single native startup call. Native/version calls have a ten-second bound plus at most one second to reap their own timed-out CLI. Native error/timeout is unknown admission and never triggers another CrewMoor startup call. TUI performs one process-info observation even after the CLI reports a startup error; core uses its returned address and a full get. Neither command exit 0 nor `accepted=true` proves an Agent process. `process_verified` is only resource evidence. No terminal content, unrelated foreground argv or native semantic status is stored.

## External TUI host (Issue #15)

With `host = { socket, server_version }`, a TUI launch uses the user's existing
server instead of a private one: no boot, `server_owned=false`,
`binding_scope=workspace`, and the owner binding is `<store>/<team>.<role>.json`
so several roles of one team share the server. With `feedback_instance`,
`workspace create` adds `--env CREWMOOR_FEEDBACK_INSTANCE=...`; on an App/core
target the same key goes into `terminal.run` `env`. Codex may take one
`initial_prompt`. Stop closes only the own pane and empty own workspace and never
calls `session stop`. See [team launch](team-launch.md).

## End only this target

Before a mutation, the stored team binding and server/socket identity must still match. TUI closes only its returned pane after matching pane/terminal/workspace ids. If its returned workspace had no additional pane/tab and is now empty, it also closes that workspace and checks the resulting inventory. User-added panes/tabs keep their workspace intact. TUI does not require the original shell to remain alive, the Agent to remain in a unique foreground, or the entire instance to be exclusive. Core checks its returned address, creation generation and owner/team/launch tuple, then calls native kill/rm for that terminal. There are no raw Agent PID signals.

After target removal, recorded process exit is checked. Only an empty native instance may be stopped, with recorded service/holder exit and socket absence confirmed before binding release. If the user has added resources, only our target is closed: `phase=partial-stop`, instance/binding retained, no server/session stop. Unknown removal, timeout, changed native identity or unconfirmed exit retains evidence/binding. There is no generic recovery or adoption of existing panes; do not retry an unknown operation blindly.

The preceding whole-instance stop applies only to private mode. App mode always retains its external host, even if empty. `server_owned=false`, `binding_scope=terminal` and `phase=target-stopped-host-retained` distinguish confirmed own-terminal removal/recorded Agent exit from a host that remains available. The terminal binding is then released; host/holders/socket and all user layout/resources are left alone. A host remaining alive does not mean the Agent remains alive.

`phase=stopped` describes owned resource cleanup, not Agent task completion or proof that an Agent ever launched. The Agent's prior fingerprint remains as historical evidence; `process_verified=false` after stop and semantic `agent_status=unknown` are explicit. State/cwd/logs are retained, not deleted automatically. This minimal record format is for new instances, not a migration/recovery tool for old held records.

## Environment and acceptance

Future children retain user authentication, tools, HOME/PATH, Cargo/Rustup and all proxy/bypass settings. The explicit native denylist removes inherited developer conversation/tool identifiers, CI/color markers and old routing selectors before establishing fresh instance routing. It does not clear the whole environment, purge all CODEX variables, edit `~/.codex/.env`, alter shell/Orca configuration or restart existing services.

Rust transport tests cover both runtimes × both Agents, official response shapes, unknown admission without resend, target mismatch refusal, partial cleanup with extra user resources, exit/socket evidence, environment inheritance and minimal evidence storage. Synthetic alias tests never execute `gh auth token`.

[Evidence layers](issue-2-current-evidence.md) distinguish 769/v9 history, a1's explicit App Claude case, external native probes, and the accepted bd4/v13 App project Codex and private TUI Claude cases. Issue #2 was accepted and merged; its historical proposals do not authorize another launch. [Issue #4 feedback](agent-feedback.md) is separate from the launch receipt, whose Agent semantic status remains unknown. Old core/user resources remain HOLD.
