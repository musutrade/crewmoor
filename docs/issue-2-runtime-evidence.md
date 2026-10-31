# Issue #2 runtime evidence — in progress

Batch: `crewmoor-issue-2-001`. Baseline: `50114d4dfa2ad6ec4addc450864b3b39919a6083`.

This report records an actual core launch and an unsuccessful TUI attempt. It is not a declaration that Issue #2 is accepted. Windows App observation of this isolated core and final cleanup are pending user coordination. The existing user's default-core Claude/App observation and CM-7429 exchange are separate background evidence, not this launch acceptance.

## Fixed builds and commands

- Rust/Cargo stable 1.99.1, edition 2024; no MSRV matrix.
- Installed Herdr TUI client 0.9.5; this test's private server reported 0.9.5. The existing user's 0.9.4 server was not modified.
- herdr-core `hc 0.1.15`; actual private core reported 0.1.15.
- `codex-cli 0.166.0`; launched with `codex --no-daemon --no-alt-screen`, without a prompt or model task.
- CrewMoor 0.1.0, first launcher revision `f8fbcfd8ae44158258b30aaf87b36c05c071835d`, artifact SHA-256 `1f3ab062edfc1c944a667bbe3333c8cc4d22a726b91e0194fbcc7a3e84c04017`.
- Corrected compact-core-response decoder revision `7571600714b9f82180e39e13d2e2171de7d708c9`, artifact SHA-256 `c22a68239142e12c220d963236961206da39288296d73318548b271e62c26271`.
- TUI process-group fix revision `06a4f7f9c082f4f49b9de567fdf2a70f98cfde10`, artifact SHA-256 `c1c05f6931fb30d9fa4a8ba66602b4381888d4c7641df8926ef087ffe1691026`; not yet used for a second real TUI startup, pending a separate bounded diagnostic authorization.

Binaries and complete receipts are preserved outside the repository at `/home/gem/.local/state/crewmoor/crewmoor-issue-2-001/`. They are build snapshots, not PATH installations. No credentials or raw model transcript are included in this report.

## Real core launch and location

Executed once:

```text
.../crewmoor-run launch .../core.toml /tmp/cm-i2-core
```

The config pins the installed runtime/Agent versions, uses team `issue2-core`, role/name `cm-core`, cwd `/tmp/cm-issue2-work`, private state/config under `/tmp/cm-i2-core`, and the normal native named-socket directory `/run/user/1000/herdr-core`.

Returned/observed identities:

| Fact | Actual value |
|---|---|
| Session | `cm-0c926f847d55` |
| Native core id | `d5834667f478cf1fc57eb651fdfdd6ba` |
| Socket | `/run/user/1000/herdr-core/cm-0c926f847d55.sock` |
| Core PID / start ticks | `3033912` / `175846958` |
| Holder PID | `3033920` |
| Terminal | `t1` |
| Native creation ms | `1793447385550` |
| Logical name / team | `cm-core` / `issue2-core` |
| Foreground Agent PID / ticks | `3033926` / `175846966` |
| Actual Agent executable | Codex standalone `0.166.0-x86_64-unknown-linux-musl/bin/codex` |
| Process state | `live`, argv `codex --no-daemon --no-alt-screen` |
| Reported state | `reported=true`, OSC app `codex`, state `idle` |

The launch acknowledgment was real, but the initial implementation required `created_ms` in the abbreviated CLI launch response and exited with `missing creation identity`. Its receipt correctly retained `accepted=true`, `ready=unknown`, and the returned `t1`; it did not launch again. A scoped read-only `get t1` confirmed the matching owner/team labels and full native identity. After correcting the decoder, the **same existing process** was successfully located through:

```text
.../crewmoor-run-v2 inspect /tmp/cm-i2-core
```

That returned `phase=observed`, the same core id/terminal/creation identity, `accepted=true`, verified process identity and `ready=idle`. `run` was not repeated. Scoped `ls --all` was separately checked to establish that its output is also a compact summary; stop therefore combines a fully validated `get` with the summary address inventory.

Actual network requests or charges are not measured by the container fields. The user authorized startup credential/config access and possible plugin/MCP/upstream traffic; no model task was sent and zero cost is not asserted.

## Real TUI attempt — not passed

Executed once with the corrected decoder build:

```text
.../crewmoor-run-v2 launch .../tui.toml /tmp/cm-i2-tui
```

Native Herdr created session `cm-c3f49c0b2430`, server PID `3037079` / ticks `175866809`, workspace `w1`, pane `w1:p1`, terminal `term_65f218e2d5e931`, team `issue2-tui`, role `cm-tui`, and the same independent scratch cwd.

The supported `agent start` command timed out waiting for readiness. The receipt retained `accepted=null`, `ready=unknown`, `phase=unknown-outcome`. Later read-only checks found the recorded server and shell PID `3037096` gone and the private socket absent. No reliable Agent PID/ready evidence was obtained; neither process startup nor controlled end is counted as passed. No automatic retry was made, and no existing session was stopped.

The server had inherited the launch client's process group. The implementation now creates a separate group. Rust-only tests confirm a different PGID and that an owned transport server survives the launcher exiting, then ends through its own test socket. This is transport evidence, not proof of a real TUI fix; a single new isolated diagnostic attempt has been proposed to the coordinator, pending authorization. Original failed state/config/logs and binding are retained as failure evidence; unconfirmed resources are not force-deleted.

## Windows App and final controlled end — pending

The user confirmed their Windows core is 0.1.15 and its help supports ordinary SSH `--session`/`--name`/`--temp`. Ubuntu's native session/socket resolution was read and checked; the isolated target has been sent to the coordinator. No Windows connection was modified by the developer.

Proposed user-only temporary connection, filling their existing SSH target locally:

```text
herdr-core machine add <existing-SSH-target> --name cm-issue2-probe --session cm-0c926f847d55 --temp
herdr-core get cm-issue2-probe/t1
```

App inspection must actually locate/peek the target and match its role/cwd/Codex identity. It is not inferred from help or existing default-core success. The core target is intentionally kept alive for that observation. First remove only the temporary Windows machine entry, then coordinate CrewMoor's guarded stop, avoiding automatic reconnect after shutdown. Until that sequence occurs, App acceptance and real core cleanup remain unverified.

## Automated checks

The business crate is the root `Cargo.toml`; complete Rust `verify` includes it, rather than reporting N/A. The latest complete invocation exited 0: quality-tool integration tests 24, business integration tests 17, plus actual fixture tests, with 254 functions passing CRAP <= 10. fmt, policy, Clippy `-D warnings`, tests and fresh LLVM coverage were all included. The suite covers binding conflicts, process/socket/native-handle changes, compact response handling, unknown delivery outcomes, strict stop refusal with extra resources, and transport process lifetime. Tests use Rust doubles and do not launch real Herdr/Codex. The exact candidate is bound by the audit request; post-observation end evidence is still pending. Remote CI has not run.
