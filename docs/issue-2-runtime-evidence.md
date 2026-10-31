# Issue #2 runtime evidence — acceptance blocked

Batch `crewmoor-issue-2-001`, repository `musutrade/crewmoor`, baseline `50114d4dfa2ad6ec4addc450864b3b39919a6083`, branch `issue/2-runtime-launch`, developer worktree `/home/gem/crewmoor-issue-2-001`.

There is one real isolated core/Codex launch, user-confirmed App terminal display, and two unsuccessful real TUI attempts. The authorized TUI diagnostic attempt was subsequently cleaned up with the repaired stop path. TUI Agent launch is not accepted; the argument fix has not had a third real test. Core cleanup is on an explicit user hold because that core now also contains user-owned App terminals. The original core inherited developer metadata; it is not evidence of a clean environment boundary. Issue #2 is not ready to publish or close. Automatic tests and upstream source inspection do not replace missing real lifecycle checks.

## Versions and build provenance

- Rust `1.99.1 (58cf8f9c3 2026-10-13)`, Cargo `1.99.1 (5f94df478 2026-08-27)`, edition 2024, cargo-llvm-cov 0.9.0. Latest stable channel, no MSRV matrix.
- Installed TUI client and both private test servers: `herdr 0.9.5`. The user's existing 0.9.4 TUI server was not changed.
- Ubuntu CLI/private core and the user's Windows CLI: `hc 0.1.15`. Windows App binary version/build has not been independently fixed; CLI version alone does not identify the App build.
- Agent: `codex-cli 0.166.0`, actual Linux executable under `0.166.0-x86_64-unknown-linux-musl/bin/codex`. Core argv `codex --no-daemon --no-alt-screen`, no prompt/task. No developer-created Claude startup.
- CrewMoor package 0.1.0; exact build snapshots below. The final candidate and its complete validation log are bound by the audit handoff, not inferred from the package version.

| Snapshot | Source revision | SHA-256 | Actual use |
|---|---|---|---|
| `crewmoor-run` | `f8fbcfd8ae44158258b30aaf87b36c05c071835d` | `1f3ab062edfc1c944a667bbe3333c8cc4d22a726b91e0194fbcc7a3e84c04017` | Single core launch |
| `crewmoor-run-v2` | `7571600714b9f82180e39e13d2e2171de7d708c9` | `c22a68239142e12c220d963236961206da39288296d73318548b271e62c26271` | Same core inspection; TUI attempt 1 |
| `crewmoor-run-v3` | `06a4f7f9c082f4f49b9de567fdf2a70f98cfde10` | `c1c05f6931fb30d9fa4a8ba66602b4381888d4c7641df8926ef087ffe1691026` | Build snapshot; not another launch |
| `crewmoor-run-v4` | `88db1cb6be82b1101c654760e79207f33d2c4eb1` | `7bb9cda231f8f24894f9ed446902b8dafa841bde1d9e0aa9d2c4470b36e735a1` | Authorized TUI diagnostic attempt 2 |
| `crewmoor-run-v5` | `ba83e42644ff59012a23baad84f4646adb7f4460` | `4a07f9498a922f4a6e80f7b8084e4162aa8302e9fbbeee89444b1335f2a9f2b8` | Read-only core inspection; TUI attempt 2 cleanup only |

Snapshots, configs, receipts and diagnostic evidence remain outside git at `/home/gem/.local/state/crewmoor/crewmoor-issue-2-001/`; they are not PATH installations. State roots retain private runtime logs. No credentials or raw model conversation are uploaded.

## Authorization and isolation

The user authorized one real Codex startup in each of two separate test instances, without a task, including existing authentication/config access and possible plugins/MCP/upstream traffic. After attempt 1, `TUI_RETEST_AUTHORIZED` explicitly allowed one diagnostic retest with pinned v4, `tui-r2.toml` and fresh `/tmp/cm-i2-tui2`; on failure, stop and do not create a third attempt. `TUI_RETEST_RESULT` reported the duplicate-argument failure. Earlier audit material saying the retest was still pending was stale; the retest was authorized. This report replaces that stale chronology.

Every launch has a fresh random session, private state/config and distinct team/role. Core uses a named Unix socket, not TCP. TUI uses private XDG paths and, since the process-group correction, a separate process group. No existing runtime, proxy, SSH configuration or shared Codex daemon was changed. Actual network requests/charges were not measured; no model task was sent, and zero cost is not asserted. Developer metadata inheritance was incomplete in those original builds; see the environment defect below.

## Real core launch and location

Executed once:

```text
/home/gem/.local/state/crewmoor/crewmoor-issue-2-001/crewmoor-run launch /home/gem/.local/state/crewmoor/crewmoor-issue-2-001/core.toml /tmp/cm-i2-core
```

| Identity | Observed value |
|---|---|
| Team / role / cwd | `issue2-core` / `cm-core` / `/tmp/cm-issue2-work` |
| Session | `cm-0c926f847d55` |
| Core id | `d5834667f478cf1fc57eb651fdfdd6ba` |
| Socket | `/run/user/1000/herdr-core/cm-0c926f847d55.sock` |
| Server PID / start ticks | `3033912` / `175846958` |
| Holder PID | `3033920` |
| Terminal / creation ms | `t1` / `1793447385550` |
| Codex PID / start ticks | `3033926` / `175846966` |

The native launch acknowledgment established `accepted=true`, but the first decoder incorrectly required `created_ms` in the compact CLI `run` reply. Launch exited with `missing creation identity`, preserving `t1` and `ready=unknown`. Scoped full `get` established matching owner/team/name/cwd/argv. Corrected v2 `inspect /tmp/cm-i2-core` bound this same process's generation without another `run`. It reported `phase=observed`, verified process identity, `live`, `reported=true`, app `codex`, `ready=idle`, turn 1. CLI `ls --all` is also compact; stop combines a validated full `get` with address inventory.

Evidence: `evidence/core-launch.stderr`, `core-get-recovery.json`, `core-inspect-v2.json`, `core-stop-inventory-schema.json`, `core-after-app-v5.json`, and `/tmp/cm-i2-core/record.json`. Request acceptance, process existence and explicit readiness are separate layers. Unknown/error was not relabeled success.

## TUI attempt 1 — failed, failure evidence retained

```text
/home/gem/.local/state/crewmoor/crewmoor-issue-2-001/crewmoor-run-v2 launch /home/gem/.local/state/crewmoor/crewmoor-issue-2-001/tui.toml /tmp/cm-i2-tui
```

Team `issue2-tui`, role `cm-tui`, session `cm-c3f49c0b2430`, server PID `3037079` / ticks `175866809`, initial shell `3037096`, workspace `w1`, pane `w1:p1`, terminal `term_65f218e2d5e931`.

`agent start` timed out after 30 seconds. Receipt: `accepted=null`, `ready=unknown`, `phase=unknown-outcome`. Later read-only observations found server/shell gone and socket absent; no confirmed Codex PID or ready state was obtained. This is neither a passed Agent launch nor a verified controlled stop. The original process group was shared with the client; the subsequent separate-group fix addresses that lifetime risk, but the first failure's visible output did not establish its complete cause.

No automatic retry or session stop was performed. `/tmp/cm-i2-tui`, logs/receipt and the `issue2-tui` binding remain as failure evidence; unconfirmed resources were not force-deleted. Evidence includes `tui-launch.stderr`, `tui-process-info.json`, `tui-visible.txt` and private runtime logs.

## Authorized TUI diagnostic attempt 2 — failed, controlled cleanup passed

The pinned v4 SHA-256 was checked before the single authorized launch:

```text
/home/gem/.local/state/crewmoor/crewmoor-issue-2-001/crewmoor-run-v4 launch /home/gem/.local/state/crewmoor/crewmoor-issue-2-001/tui-r2.toml /tmp/cm-i2-tui2
```

| Identity | Observed value |
|---|---|
| Team / role | `issue2-tui-r2` / `cm-tui-r2` |
| Session | `cm-9198ac8072f2` |
| Server PID / ticks / PGID | `3054019` / `175981510` / `3054019` |
| Original shell PID / ticks | `3054036` / `175981521` |
| Workspace / pane / terminal | `w1` / `w1:p1` / `term_65f21d28a4e5f1` |

Native log chronology (UTC, 2026-10-31): server listening `12:12:10.993949`, workspace/Agent launch around `12:12:11`, timeout recorded by `12:12:41.259892827`. During the request, scoped process/visible diagnostics showed original bash and Codex's error: `the argument '--no-daemon' cannot be used multiple times`. The shell returned to its prompt. The private server survived launcher exit, confirming separate-group lifetime, but Codex startup/ready remained unconfirmed (`accepted=null`, `ready=unknown`).

Read-only shell inspection identified `alias codex` and `--no-daemon` at `/home/gem/.bashrc:142`; the file was not modified. CrewMoor now omits its explicit TUI flag, retaining the core flag exactly once. Rust regression verifies these distinct argv. This fix has not been validated by another real TUI launch: no third launch is authorized or performed.

After strict server/socket/binding, owner/workspace, tab/pane/terminal and sole native shell/foreground-group checks, repaired v5 performed the following stop. The shell matched earlier r2 diagnostic evidence; v5 itself did not persist a separate original-shell fingerprint at launch. The subsequent lifecycle fix does persist and compare it; that guarantee is not retroactively attributed to v5.

```text
/home/gem/.local/state/crewmoor/crewmoor-issue-2-001/crewmoor-run-v5 stop /tmp/cm-i2-tui2
```

Cleanup started `12:33:34Z`, workspace close completed `12:33:34.620112`, subsequent pane get returned native not-found, and server exited `12:33:34.926062`. Command exited 0; final receipt is `phase=stopped`, `ready=not-running`, `accepted=null`, ending shell `3054036` / ticks `175981521`. Kernel checks confirmed server and shell exited; socket and `issue2-tui-r2` binding were absent. Timeout/transport failure would not qualify as absence. No Agent was restarted for cleanup. State and logs remain as evidence.

Evidence: `tui-r2-launch.stderr`, `tui-r2-process-during-start.json`, `tui-r2-visible-during-start.txt`, `tui-r2-pane-before-cleanup.json`, `tui-r2-workspaces-before-cleanup.json`, `tui-r2-shell-wrapper-markers.txt`, `tui-r2-stop-started.txt`, `tui-r2-stop-v5.json`, and `/tmp/cm-i2-tui2/config/herdr/sessions/cm-9198ac8072f2/herdr-server.log`.

## Windows App — terminal display observed; user resources protected

The user confirmed Windows `hc 0.1.15` and `machine add` support for SSH target, `--name`, `--session`, `--temp`. Session-specific SSH routing was checked separately from App display. No TCP port or existing SSH/default-core configuration was changed. The user actually connected through machine alias `vps-cm` to `cm-0c926f847d55`; earlier `cm-issue2-probe` was only a proposed name. Developer did not add/remove a Windows entry.

The user created App space `codex-cm` with an extra shell, then `terminal.show vps-cm/t1` displayed original Codex 0.166.0/cwd. Dispatcher read-only checks confirmed the same core/server/holder, terminal creation ms, Codex PID/argv and CrewMoor labels; turn 1/idle remained unchanged, only size changed from 200×120 to 139×35. Connection, original process location and actual terminal display are verified. External Codex did not appear in Agents. A later user-created Claude in App space `cm-issue2-work` did appear. This does not establish that space membership alone determines the panel.

Dispatcher's named observation at `READONLY_USER_CLAUDE_CONFIRMED` (not a continuously polled inventory):

| Terminal | Ownership / observation |
|---|---|
| `t1` | Original CrewMoor Codex, PID `3033926`, idle |
| `t3` | User App bash, PID `3066881`, space `codex-cm` |
| `t7` | User Claude, PID `3077260`, shell `3077179`, cwd `/tmp/cm-issue2-work`, reported/idle, turn 0 |
| `t2/t4/t5/t6` | Ended user shell records; not CrewMoor-owned |

That observation had three live terminals and two Agents. User Claude labels: `herdr.space=spe21a797a2d7ae37cbddc8f780fa6f7ae`, `herdr.tab=tb005876b579c7be38e161c21eee567d01`, `where=cm-issue2-work / 1`. Later environment investigation also mentioned user `t8`; this table is a timestamped observation, not a claim that no later objects exist. These are dispatcher/user observations, not developer mutations or automatic test evidence.

The user ended validation and subsequently explicitly held **all cleanup in this core, including t1**. No core kill/rm/server-stop was executed. Core/holder, terminals, state/binding and Windows entry remain; extra user resources would also make the whole-instance stop refuse before mutation. Do not bypass refusal or assign user resources to CrewMoor. No third TUI run, input message, keep/show/label change or extra startup is part of the remaining authorization.

Earlier default-core Claude observation, the authorized CM-7429 exchange and user close action remain background capability evidence. They neither satisfy this launch's acceptance nor authorize further action on those targets.

## Environment inheritance defect and future-launch correction

Original `native::command` used `Command::new` with ambient inheritance, removing only part of the runtime metadata. It missed `CODEX_SESSION_ID`, `CODEX_TOOL_CALL_ID`, `CODEX_CI`, `CODEX_VERSION` and `NO_COLOR`. Dispatcher white-list inspection found the same developer session/tool-call metadata in server `3033912`, holder `3033920`, original Codex and App-created shell/Claude ancestry. Developer independently confirmed only these key names in the server, without printing full environment/credentials. This is a launch isolation defect, so the original core/App evidence is not a clean-environment acceptance.

The user personally confirmed that t7 Claude was started from Windows. The developer did not execute any command creating t7 or starting Claude. t1 and the user's shell share holder ancestry; Claude descends from its shell. Matching environment values do not prove t1 created that shell. Both private `core.log` and `holder.log` were zero bytes when inspected; the logs cannot independently attribute a terminal.run request or prove its absence. No unrelated user messages/transcripts were read.

Upstream core `4a3591aa835c197f57a5d1b8e8951a75e2a79faa` obtains service environment via `platform::program_env`, filters a limited set of identity variables, merges caller overrides, then adds terminal id/socket/scope. Holder passes that map to Unix `spawn_pty`, which clears its own ambient map and installs the merged map. Thus an already-contaminated service map can propagate despite PTY env_clear. [run.rs](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/api/run.rs#L213), [platform](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/platform/mod.rs#L65), [holder](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/holder.rs#L263), [Unix spawn](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/platform/unix.rs#L290). This explains the observed path; it is not proof of the caller that created t7.

Future child commands now use an explicit metadata/routing/CI/color denylist, also for version probes, before native server startup. It is neither env_clear nor a CODEX_* prefix purge. The exact keys and preservation rules are documented in [runtime usage](runtime-launch.md#child-environment-boundary). Synthetic Rust parent→server→holder→Agent transport probes verify absent keys, fresh core routing selectors and preserved tool/auth/proxy settings for both runtime paths. They neither launch real Agents nor repair a running service. The existing core is untouched under HOLD. The user authorized one fresh TUI and core attempt; neither has run, and execution requires independent pass for the repaired exact SHA/artifacts.

## Read-only App source findings and limits

Inspected upstream `herdrdev/herdr-app` revision `3130e416ce3d1d39c83f64413728ea070a845424`; it is **not established as the user's installed App build**.

- `publish()` enumerates formal space/tab/pane terminals and requires `Model::status`; `agents()` returns that snapshot. Status needs a known Agent and state word. Space membership alone is insufficient. [window](https://github.com/herdrdev/herdr-app/blob/3130e416ce3d1d39c83f64413728ea070a845424/src/window.rs#L1543), [model](https://github.com/herdrdev/herdr-app/blob/3130e416ce3d1d39c83f64413728ea070a845424/src/model.rs#L497), [mods](https://github.com/herdrdev/herdr-app/blob/3130e416ce3d1d39c83f64413728ea070a845424/src/mods.rs#L957).
- `terminal.show` records a temporary swap or focuses an existing pane; it does not add a formal pane. `keep_terminal` compares terminal address prefix with the active space's machine string, not a core id. [keep](https://github.com/herdrdev/herdr-app/blob/3130e416ce3d1d39c83f64413728ea070a845424/src/window.rs#L2363), [show](https://github.com/herdrdev/herdr-app/blob/3130e416ce3d1d39c83f64413728ea070a845424/src/window.rs#L2893).
- Layout ids use that machine string. Labels plus live state participate in adoption; temporary entries can be excluded from layout documents. These checks differ from SSH session routing. [mirror](https://github.com/herdrdev/herdr-app/blob/3130e416ce3d1d39c83f64413728ea070a845424/src/mirror.rs#L57), [documents](https://github.com/herdrdev/herdr-app/blob/3130e416ce3d1d39c83f64413728ea070a845424/src/window.rs#L2182), [adoption](https://github.com/herdrdev/herdr-app/blob/3130e416ce3d1d39c83f64413728ea070a845424/src/window.rs#L4376).

The user's `terminal.keep vps-cm/t1` machine-mismatch error does not reveal the then-active space machine or whether the alias matched that build's internal name. No keep was executed for diagnosis. A future authorized diagnostic would obtain the App build and read-only space/machine/pane identities, compare exact address prefix and active space machine, then decide whether to adopt the existing terminal. This is not implemented GUI orchestration or a current instruction to mutate the user's layout.

## Automated gates and remaining acceptance

All old shared-target success claims for `88db1cb`, including the reported 254-function/24+17-test result, are **invalid and withdrawn**: shared Cargo target mixed checkout binaries/profiles and produced another checkout's coverage source. No global Cargo/proxy configuration was changed; the shared target was not cleaned or deleted. Unknown coverage sources must continue to fail.

Final validation must run at the handoff's exact SHA with this invocation-level target; every nested Cargo/LLVM command inherits it:

```sh
CARGO_TARGET_DIR=/home/gem/crewmoor-issue-2-001/target cargo +stable run --locked --manifest-path tools/quality/Cargo.toml -- verify
```

Root `Cargo.toml` is the business crate, **not N/A**. Verify covers tool, fixture and business fmt, policy, Clippy `--all-targets --all-features -- -D warnings`, real tests/LLVM coverage and all-function CRAP <=10. A Rust regression checks a Cargo subprocess's actual target against the explicit inherited absolute value. Results/log and exact SHA accompany the audit request; auditor uses its own separate target. Foreign checkout mappings are never ignored.

Independent audit of `21df653d9880ab00bc689e1baeaff8a3d7675352` passed code/automatic checks only: 297 functions, maximum CRAP 9.318851, 25 quality and 23 business integration tests, using the auditor's separate target. Real acceptance remained blocked. Follow-up code now persists the original TUI shell, records/verifies native core holder identity, checks holder exit and socket absence, and supports strictly empty server-only cleanup. Missing metadata in the held old core is not retroactively filled for cleanup. The new SHA requires another independent audit.

Audit of `8eb336b841a0a5dc76876b916dc7f76cccc63c3c` found a blocking API assumption despite valid isolated automatic results (336 functions, maximum CRAP 9.0, 25 quality/33 business tests): its fake invented a public Terminal holder field absent from core 0.1.15's bundled schema and the archived real get response. Rejected v6 was never used for a real launch. The repair removes that assumption, retains StatusResult's single current holder/native process checks, requires exclusive terminal inventory for target access, and preserves holder exit confirmation. The same two-attempt authorization is retained pending a new exact-SHA audit; no attempt was consumed.

The expanded Rust suite has 25 quality and 36 business integration tests, plus actual fixture tests. Existing shell replacement, multiple/changed/unknown holder, surviving holder, empty/nonempty inventory, socket absence and environment regressions remain. New regressions exercise inspect/stop with a holder-free Terminal, reject an invented holder field, and check consumed fake objects against required/allowed field sets exported from the installed core 0.1.15 and TUI 0.9.5 schemas. Relevant core status/holder/core-info and TUI Agent/pane/process/workspace fake fields were checked and completed. CLI compact run/list fields also match the archived real responses. The key-level contracts are not a complete JSON Schema validator or real runtime acceptance. Schema provenance hashes are in `tests/data/*-fields.json`; no user content is stored there. The read-only Rust receipt probe emits only whitelisted marker names; tests use owned Rust processes and synthetic session values. Tool policy/CRAP thresholds were not relaxed.

Remaining real gates: successful TUI Agent launch/ready after the fix, clean-environment real launch evidence, verified core controlled stop (currently forbidden by HOLD/user resources), and independent audit of final SHA. Remote CI/required checks have not run or been confirmed. App terminal display passed; Agents-panel membership of external t1 did not. No incomplete item is closed by green local verify.

The original core's observed holder `3033920` is absent from its old receipt's exit confirmation. No real core stop has occurred, so its complete service cleanup remains unverified. The retained authorization for the [prepared plan](issue-2-next-real-acceptance.md) is conditional on independent pass for its repaired exact SHA/artifacts; the held old core is never used to test this path.
