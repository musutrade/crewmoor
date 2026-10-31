# Runtime launch prototype (Issue #2)

Linux-only, latest Rust stable / edition 2024. One configured Codex process in one new Herdr TUI or herdr-core instance. No initial prompt, messages, database, team orchestration, worktree management, installer or daemon replacement.

Build and validate:

```sh
cargo +stable build --locked --bin crewmoor
CARGO_TARGET_DIR="$PWD/target" cargo +stable run --locked --manifest-path tools/quality/Cargo.toml -- verify
```

The root `Cargo.toml` is the business crate. Complete verify checks it with fmt, the source/lint policy, Clippy `-D warnings`, tests, fresh LLVM coverage and all-function CRAP <= 10. Tests use a Rust transport executable and injected native replies. They do not start Herdr, Codex or containers; a transport startup failure uses only the Rust fixture. Real runtime acceptance is separate.

Copy and edit `examples/tui.toml` or `examples/core.toml`, setting absolute cwd, store and installed binary, and exact `--version` outputs. Agent support is deliberately Codex-only. Allowed optional launch arguments are `--no-alt-screen` and `--model NAME`; prompts/subcommands are rejected. Core passes `--no-daemon` explicitly. TUI uses its native codex launch path without duplicating that flag: this host's interactive shell already injects it via a codex alias. TUI Agent acceptance remains blocked after two failed real attempts; removing the duplicate is covered by regression, not a third real startup. Startup may still access credentials, MCP/plugins and network, so obtain the user's launch authorization first.

Use different team names and previously nonexistent state roots for the two runtimes:

```sh
cargo run --locked --bin crewmoor -- launch /absolute/tui.toml /tmp/cm-tui
cargo run --locked --bin crewmoor -- inspect /tmp/cm-tui
cargo run --locked --bin crewmoor -- stop /tmp/cm-tui

cargo run --locked --bin crewmoor -- launch /absolute/core.toml /tmp/cm-core
cargo run --locked --bin crewmoor -- inspect /tmp/cm-core
# Stop only after any required App observation and removal of its temporary connection.
cargo run --locked --bin crewmoor -- stop /tmp/cm-core
```

State roots must be absolute and <=45 bytes to fit native Unix socket limits. The store is the CrewMoor binding namespace: team names must be unique inside that store. Exclusive binding creation prevents the same team from launching in a second instance, including a different runtime. Use the same store for all teams managed by this CrewMoor namespace. A stopped instance releases its binding; state and evidence remain. State roots are never overwritten, retried or recursively deleted.

## What the JSON means

- `crewmoor_version`, config versions, owner, team/role and session identify this launch. Keep the tested git SHA and binary SHA-256 alongside the receipt for exact build provenance.
- `server` contains Linux PID, process start ticks and executable path. Socket inode, binding owner and canonical state root are checked before query/stop. A changed or missing identity fails before native calls, preventing core CLI auto-start from reconnecting to an unintended instance.
- `container_id` is the native core id or private TUI session; `handle` is the native terminal address or pane id, with native creation generation or terminal/workspace ids. Never reuse a handle in another container.
- `accepted: true` is a successful native Agent launch response. `null` means there was no confirmed response; an error or timeout does not prove rejection or absence of effects.
- Core 0.1.15 CLI `run` and `ls` return compact summaries, not the full schema objects. After the launch acknowledgment, CrewMoor uses `get` to validate the same container/owner/role/argv/cwd and bind the creation generation. A summary-only receipt can be inspected this way without repeating `run`; unacknowledged requests cannot establish a generation. Stop validates the full target and then checks the compact inventory contains only that address.
- `agent_process` is the observed OS process identity, separate from request acceptance. `null` means unconfirmed. Native observations preserve reported argv, process and terminal facts.
- `ready` is the explicitly reported Agent state, or `unknown`. TUI requires `interactive_ready`; core requires an OSC status from app `codex`. A terminal being live does not imply ready. A trust/auth dialog is not answered automatically.
- An intent is saved before mutation, and successful native creation replies are saved before decoding. Partial/failed launches retain an `unknown-outcome` receipt and binding. Do not launch again to resolve ambiguity; inspect the recorded private instance, and ask for human recovery when strict identity checks cannot succeed.

`operation.lock` prevents concurrent operations on the same state. A crashed client can leave it behind; inspect ownership and active processes before an authorized manual removal. This is a small local receipt/binding mechanism, not a recovery or orchestration system. Configuration, executables and receipt files are trusted local inputs; it is not a hostile-user sandbox.

## Isolation and controlled end

TUI uses its supported `herdr --session <random> server` flow with a private XDG config tree and separate process group, then `workspace create` and `agent start`. Named TUI sessions derive their socket from XDG config and override the ambient socket/config selector, so the caller's Herdr socket and pane markers are removed only in child environments. The private config disables background version/manifest checks. Separate-group survival is tested with a Rust process fixture; actual TUI lifecycle evidence is still required.

Core uses a random `HC_SESSION`, private XDG state/config (`notes=false`) and a private socket. `run --bg --no-parent --no-notify` creates only this Agent. No command changes proxy settings, `~/.codex/.env`, existing runtime configuration or the shared Codex daemon. Agents' normal authentication/config loading remains inherited.

Stop verifies the same server process/socket/binding plus native role, handle, creation generation, cwd/argv and ownership labels (core), or terminal/workspace identity and workspace owner label (TUI). It refuses additional terminals, workspaces, tabs or panes. TUI also requires the recorded Agent foreground PID or the sole original native shell PID/group; an unrecognized replacement or additional foreground process is refused. This permits cleanup of a failed launch that returned to its own empty shell. Native not-found must confirm removal of the terminal/pane, followed by `/proc` exit confirmation for the server and captured ending/Agent identities. Timeouts and transport errors are not absence. PID reuse is distinguished by process start ticks; zombies are terminated programs, not live Agents. State and observations remain, and uncertain outcomes retain the binding. Business cwd/worktrees and shared caches are not removed.

## Windows App observation through existing SSH

Ubuntu's headless test does not include an App binary. Windows App and local core versions/capabilities must be confirmed by the user. Core 0.1.15 documents `machine add TARGET --name NAME --session SESSION --temp`; ordinary SSH transports the generation-1 core protocol over stdin/stdout. No TCP listener or SSH configuration change is needed.

For App acceptance, configure this test's `endpoint_dir` as the Ubuntu user's standard `/run/user/UID/herdr-core`, preserving private state/config. This creates a new random-session Unix socket; it does not change the existing default socket. Check the native remote config has no `socket` override, and that the SSH user resolves the same runtime directory. A scratch nonstandard socket is not automatically found by `--session`.

After CrewMoor has actually launched the target, the coordinator supplies its exact session/core id/address/team/role/cwd/process. Only then the user may add a **new** temporary machine in Windows, filling their existing SSH target locally (no credentials are requested):

```text
herdr-core machine add <existing-SSH-target> --name <new-temporary-name> --session <actual-session> --temp
herdr-core get <new-temporary-name>/<actual-terminal-address>
```

Do not use `--install` or replace an existing entry. On Windows, `--via` appearing in help does not establish support; use ordinary SSH. App discovery of the temporary entry and its target must still be observed, not inferred from CLI success. The App may show a terminal without an Agent sidebar entry if that Agent does not report OSC status. The user can select/peek at the exact terminal and compare its name, cwd and displayed program; a peek is viewing, not a model task. Do not send input, close the program or create extra panes as an observation shortcut.

After the user confirms observation, first remove **only the temporary Windows entry** using `herdr-core machine rm <new-temporary-name>`, then coordinate CrewMoor stop. Removing that entry leaves the remote program running until the controlled stop; disconnecting first avoids a reconnect auto-starting a new core after shutdown.

Existing user-created App targets, including prior default-core Claude observations/message/close evidence, are background capability evidence. They do not satisfy CrewMoor's independent startup/App acceptance and are never controlled or cleaned by these commands.

## Evidence

Real TUI/core launch and cleanup evidence, actual App observation, simulated tests, and CI are reported separately. See the batch evidence report when available. No unexecuted App or remote CI step is called passed.

Sources: [TUI automation](https://herdr.dev/tui/docs/agent-automation/), [core command reference](https://github.com/herdrdev/herdr-core/blob/master/docs/commands.md), and the installed binaries' help/schema (TUI 0.9.5, core 0.1.15, Codex 0.166.0 at implementation).
