# Audited v9 real attempts — core passed, TUI input withheld

Batch `crewmoor-issue-2-001`, repository `musutrade/crewmoor`. Tested source: `769f966023231f65abf3ff2d4b14aed4a67b22a8`, after its independent `CODE_AUDIT=pass`. These are actual Linux runtime attempts, separate from automatic transports. Ready/working/done and App membership are excluded. No opening task, prompt, input retry or additional launch was sent. Both authorized launch invocations are consumed; further real TUI testing needs a specific new authorization and exact-candidate audit.

## Fixed inputs

| Input | Verified identity |
|---|---|
| CrewMoor | `0.1.0`, exact source SHA above |
| TUI / core | `herdr 0.9.5` / `hc 0.1.15` |
| Agent | `codex-cli 0.166.0` |
| Bash | `5.3.9(1)-release`, kernel executable `/usr/bin/bash` |
| Rust | stable `rustc 1.99.1`, `cargo 1.99.1`, edition 2024 |
| Runtime snapshot SHA-256 | `314a497119c49d04fa23c24659a66812240163d2d222f1e9fecf289cd07b5533` |
| Evidence snapshot SHA-256 | `d4430ab483c9b886558890bbdb8429769e3f0117dc598243bc33462861bf36fa` |
| Authorized plan SHA-256 | `2e098fadf5f495c595c6801fd2e1eb3ccad017490924fc1b5d00485b6db075db` |
| TUI config SHA-256 | `eef70a91ad2383d0c46f2d9d3610faa45165cc451486d992d135c94d5a6a85b6` |
| Core config SHA-256 | `f06870542351e526b479109d9141f116127fcfa4cc7d5b87ab644b81ff7c223a` |

The artifacts/configs and external evidence directory are under `/home/gem/.local/state/crewmoor/crewmoor-issue-2-001`. Preflight confirmed all hashes and CLI versions, absent state/cwd/team-binding paths, and a clean exact source checkout. Only the planned two cwd directories were created. The Rust capture driver used a create-new marker before each operation, direct artifact argv, per-operation stdout/stderr/times and receipt snapshots; it does not retry an operation. Its retained source is `evidence/real-v9-driver.rs`. No installation, proxy/user configuration, TCP, Windows or old-core change occurred.

## TUI r3 — withheld input, owned shell/server ended

Executed once:

```text
.../crewmoor-run-v9 launch .../tui-r3.toml /tmp/cm-i2-tui3
.../crewmoor-run-v9 inspect /tmp/cm-i2-tui3
.../crewmoor-run-v9 stop /tmp/cm-i2-tui3
```

Launch ran at `2026-10-31T15:15:44.056Z`–`15:15:44.244Z`; stop ran at `15:16:02.278Z`–`15:16:02.643Z`. Each command exited 0, but launch exit 0 is not Agent startup proof.

| Resource | Recorded identity |
|---|---|
| Team / role / cwd | `issue2-tui-r3` / `cm-tui-r3` / `/tmp/cm-i2-tui3-work` |
| Session / owner | `cm-511bf7191b86` / `511bf7191b864e10b72063aad79cfb66` |
| Server PID / start ticks | `3312476` / `177082819` |
| Original Bash PID / start ticks | `3312493` / `177082830` |
| Workspace / pane / terminal | `w1` / `w1:p1` / `term_65f2462f8c3301` |

The pre-send process-info snapshots contained original Bash `3312493`, additional Bash `3312495` and `gh-real` `3312496` with argv `auth token`, in foreground group `3312493`. The sole-original-foreground guard correctly withheld input: `request_issued=false`, `accepted=null`, `agent_process=null`, `process_verified=false`, diagnostic `startup withheld: original empty shell unconfirmed`. No `pane run` was issued. These observations establish no specific caller/parent chain or cause for the extra processes.

The single subsequent inspect showed only the original Bash in that foreground group. This did not initiate an Agent request. Stop used the audited never-issued/no-Agent original-shell path, validating native ids/owner/exclusive inventory, original PID/start/executable, shell foreground/group, server/socket and binding. It returned `phase=stopped`. Native pane-not-found and process/socket checks are mandatory in that audited stop path; the independent retained Rust post-check additionally confirmed server/Bash `/proc` entries, API socket and team binding absent. State/logs/cwd remain as evidence.

**Result:** unused-runtime cleanup passed; actual TUI Agent startup was not performed and did not pass. The planned environment probe was skipped because there was no Agent. No further TUI launch/input is authorized by the consumed plan.

## Core r2 — actual Codex, location, marker boundary and full stop passed

Executed once:

```text
.../crewmoor-run-v9 launch .../core-r2.toml /tmp/cm-i2-core2
.../crewmoor-run-v9 inspect /tmp/cm-i2-core2
.../crewmoor-evidence-v9 environment-evidence /tmp/cm-i2-core2
.../crewmoor-run-v9 stop /tmp/cm-i2-core2
```

Launch ran at `2026-10-31T15:16:15.436Z`–`15:16:15.639Z`; stop ran at `15:16:28.633Z`–`15:16:28.755Z`. All commands exited 0.

| Resource | Recorded identity |
|---|---|
| Team / role / cwd | `issue2-core-r2` / `cm-core-r2` / `/tmp/cm-i2-core2-work` |
| Session / owner | `cm-e85bd34aeebc` / `e85bd34aeebc4bf8bdb0548e6ae7f3d1` |
| Core id | `aaf8da9613eaf343c1cee2aad3927dbd` |
| Terminal / created ms | `t1` / `1793459775612` |
| Server PID / start ticks | `3313050` / `177085962` |
| Holder PID / start ticks | `3313058` / `177085969` |
| Holder native instance | `dbb25c98bf27bb70c3c749592bea88ab` |
| Codex PID / start ticks | `3313065` / `177085972` |
| Unix socket | `/run/user/1000/herdr-core/cm-e85bd34aeebc.sock` |

Launch and the one additional inspect reported `accepted=true`, `request_issued=true`, `process_verified=true`, matching native role `cm-core-r2`, unchanged native generation/labels/cwd/argv and server/holder/Agent fingerprints. The kernel Agent executable matched `/home/gem/.codex/packages/standalone/releases/0.166.0-x86_64-unknown-linux-musl/bin/codex`; argv was that path, `--no-daemon`, `--no-alt-screen`. No task/prompt was supplied.

The guarded evidence probe reported `present_markers=[]` for server, holder and Codex. It read only whitelisted marker names after verifying fingerprints; it retained no environment values or unrelated entries. This confirms the named parent-metadata/CI/color boundary, not every possible environment variable. Synthetic tests separately cover preservation of tool/auth/proxy settings. Startup could access existing credentials/config/plugins/MCP and upstream network/cost as authorized; actual traffic/charges were not measured, and zero cost is not claimed.

Stop validated sole terminal inventory, native owner/team/generation/argv/cwd, unique current holder fingerprint, Agent/server and socket/binding identity. Its audited path requires native terminal removal/not-found, all recorded process exits and socket absence before binding release. Receipt `phase=stopped` retained the Agent as `ending_process`; the final false `process_verified` means it is no longer running, not a failed launch. Independent Rust post-check confirmed all three recorded PIDs absent, API socket absent and binding absent. The holder-exit evidence belongs to this fresh core only.

**Result:** core Codex minimum real startup/location/environment-marker/controlled-stop acceptance passed. Auditor subsequently independently read the recorded evidence and agreed. No live old-core query or mutation occurred.

## Evidence and retained resources

External `evidence/v9-{tui,core}-{launch,inspect,stop}.{meta,stdout,stderr,record.json}` retain commands/timing/status and receipt snapshots. `v9-core-environment.stdout` contains only the whitelisted marker report. `v9-{tui,core}-post.txt` and `real-v9-post.rs` retain the independent post-stop process/socket/binding check. Raw terminal messages and token values are not part of this report.

Both fresh instances' recorded server/shell/Agent/holder processes, primary sockets and bindings were confirmed absent after stop; their state/cwd/logs are preserved for audit. The old `cm-0c926f847d55` core, t1 and every user App/Windows resource remain under HOLD and were neither queried nor operated on. That core's old holder `3033920` exit remains unverified.

## TUI initialization source and proposed correction

Only extra foreground processes and later return to the original shell were observed; their precise creator/source is unknown. Targeted file inspection found a `gh auth token` command substitution in `.bashrc:7` assigning `GITHUB_MCP_TOKEN`, and related text in the Codex alias at line 142. This is a potential source, not proof it created the observed PIDs. The `gh` wrapper points to `/home/gem/.local/libexec/gh-real`; its name is not a caller attribution. Orca having a similar call also supplies no parent-chain proof. No token command was rerun, no token/full sensitive configuration was printed, and no investigation waited for recurrence.

Direct execution was checked first. Installed workspace-create help and schema accept cwd/label/env/focus, not executable argv. Internal runtime argv-spawn methods are not proof of a public endpoint. Private config supports `terminal.default_shell` and `shell_mode`; it selects an executable, not a shell command with arguments. [Config model](https://github.com/herdrdev/herdr/blob/v0.9.5/src/config/model.rs#L234), [shell construction](https://github.com/herdrdev/herdr/blob/v0.9.5/src/pane.rs#L2002).

Two limited options are proposed, neither implemented or executed here:

1. A dedicated Rust launcher selected only in the new instance's private `default_shell`, which execs `/bin/bash --noprofile --norc -i` and preserves the allowed parent environment/cwd. It avoids evaluating user shell startup files, but needs another artifact and may omit variables created only by those files. No user configuration or HOME/proxy rewrite would be permitted.
2. The smaller candidate is a bounded pre-send resource check for the already-owned shell: at most eight observations within a two-second wall deadline, each native call bounded by remaining time. Every observation must revalidate binding/server/socket/native owner/exclusive inventory and the recorded original Bash PID/start/executable/cwd. Only the same shell as sole foreground and group permits the one request. Unknown/mismatch aborts; busy may be rechecked only before any input, and deadline expiry leaves request unissued. The authoritative empty-shell check, not a fixed sleep, proves the send condition. There is no Agent ready/state loop, post-request retry or additional launch.

The bounded check is preferred if a direct public execution interface cannot be established: it does not assume why the shell was busy, preserves user configuration and adds no shell artifact. Proposed Rust transport regressions cover transient busy→empty, permanently busy/deadline, unknown foreground, replaced shell, owner/socket changes and native-call timeout, asserting zero input on failure and exactly one input on success. Production sole-foreground, identity and stop guards must remain intact. This is a reviewable proposal, not a recovery framework or new real-test permission.

Overall Issue acceptance remains blocked by missing TUI Agent startup evidence; real Claude entrypoints remain untested, and remote CI/required checks have not run. Code audit/automatic checks, the core real result and remaining gaps remain separate.
