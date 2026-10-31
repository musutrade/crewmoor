# Bounded v9 startup plan — executed, no retries authorized

Batch `crewmoor-issue-2-001`. Independent audit passed source `769f966023231f65abf3ff2d4b14aed4a67b22a8` and its exact v9 artifacts. The user-authorized new TUI Codex and core Codex attempts were each executed once on 2026-10-31; both authorizations are consumed. Core startup/owned stop passed. TUI withheld input while the owned shell foreground was busy and cleaned up; no TUI Agent started. See [actual results](issue-2-v9-real-acceptance.md). Rejected v6/v7/v8 must not run. These historical commands are not authorization to repeat them. Claude real startup remains untested and unauthorized.

The minimum acceptance is: report request admission accurately, prove actual Agent PID/start/executable/cwd, locate instance/logical-role/native handle and safely end only owned resources. Ready/idle/working/done and App Agents membership are not criteria; Agent feedback belongs to another issue. No feedback protocol or messages are implemented.

## Pinned inputs and commands

Expected installed versions: TUI 0.9.5, core 0.1.15, Codex 0.166.0, Rust stable 1.99.1/edition 2024. CrewMoor 0.1.0 is bound to the audit's exact SHA, not merely its package version. TUI requires its recorded original Bash shell; version/type must be recorded during preflight. Artifacts are snapshots, not PATH installations. No new software, downgrade, compatibility fallback or proxy/config changes.

| Runtime | Agent | Team / role | State | cwd | External config |
|---|---|---|---|---|---|
| TUI | Codex | `issue2-tui-r3` / `cm-tui-r3` | `/tmp/cm-i2-tui3` | `/tmp/cm-i2-tui3-work` | `tui-r3.toml` |
| core | Codex | `issue2-core-r2` / `cm-core-r2` | `/tmp/cm-i2-core2` | `/tmp/cm-i2-core2-work` | `core-r2.toml` |

Before execution, exact hashes/versions and unused state roots/team bindings were checked; only the two new empty cwd directories were created. Sessions/native handles came from receipts. The new core used only its private named Unix socket; no TCP, SSH/default-session change or Windows/App mutation.

With artifact directory `/home/gem/.local/state/crewmoor/crewmoor-issue-2-001`:

```text
.../crewmoor-run-v9 launch .../tui-r3.toml /tmp/cm-i2-tui3
.../crewmoor-run-v9 inspect /tmp/cm-i2-tui3
.../crewmoor-run-v9 stop /tmp/cm-i2-tui3

.../crewmoor-run-v9 launch .../core-r2.toml /tmp/cm-i2-core2
.../crewmoor-run-v9 inspect /tmp/cm-i2-core2
.../crewmoor-evidence-v9 environment-evidence /tmp/cm-i2-core2
.../crewmoor-run-v9 stop /tmp/cm-i2-core2
```

Each runtime had exactly one launch invocation. TUI's original empty-shell guard withheld `pane run`, so no startup input was sent; the planned TUI environment probe was skipped because no Agent existed. When its guard permits input, the implementation sends one Rust-encoded `<absolute executable> <argv>` string without `command`, PATH or alias resolution. Bash hex quoting keeps control bytes/metacharacters as data; local shell configuration remains trusted. Core used direct canonical argv with `--no-daemon` exactly once and `--no-alt-screen`, without a task/prompt. No Agent ready wait or state polling was used. Native CLI calls have a 10-second limit plus at most one second to reap a timed-out owned CLI child.

Admission and process are separate: TUI exit 0/empty stdout acknowledges input admission only; a failure/timeout leaves `accepted=null` and never resends. One process observation may still prove the exact Agent. If it does not, report unknown/unconfirmed; do not add waits, loops, another process or another launch. Optional native display fields do not become fake identity or semantic state.

The read-only Rust evidence probe reads only guarded receipt-owned PIDs, verifies fingerprints before/after access and reports whitelisted parent-marker names, skipping values. It does not print credentials/full environment. Keep all receipt, admission/error, native owner/id, original-shell/holder and actual process evidence. Synthetic tests establish tool/auth/proxy preservation separately.

## End only proved resources

Stop requires the receipt's server/socket/binding, exact native ids/owner and exclusive inventory. Missing original shell, Agent PID/foreground or holder proof refuses; no discovery/adoption during stop. Unknown admission does not by itself prevent stop when all ownership/process evidence is complete. Missing optional display name does not weaken or block that proof. After TUI request intent is persisted and the call begins, a missing/unverified/disappeared Agent or empty shell refuses cleanup and retains the recorded fingerprint/binding. Previously observed Agent exit is conservatively refused too; no separate known-exit cleanup path is implemented. Empty-shell cleanup is limited to a never-issued request with no recorded Agent and a fully verified original shell. Argument construction failures remain known-unsent; native call failures never authorize resend.

Success must show native target not-found, actual server/Agent/ending/original-shell/holder exit as applicable, API socket absence and binding release. Preserve state/logs. A failure without any target may end only its verified empty inventory using recorded server/socket/container/holder identity. Unknown metadata, replacement, extra resources or timeout preserves binding/evidence and is reported; no force cleanup or retry.

## Scope and unverified combinations

Startup may access existing authentication/config/plugins/MCP and upstream network/cost, as authorized for these two Codex attempts. No zero-cost claim. The old core `cm-0c926f847d55`, its t1 and all user-created objects/Windows entries remain under HOLD, and are not queried, changed or used as cleanup tests. No new App observation is needed for these Linux attempts.

| Combination | Automatic evidence | Revised real evidence |
|---|---|---|
| TUI × Codex | Rust transport + Bash argv/kernel checks | Input withheld; no Agent; unused shell/server cleanup passed |
| core × Codex | Rust transport/ownership/exit checks | Actual process/location/marker check/complete stop passed |
| TUI × Claude | Rust transport + Bash argv/kernel checks | Not run, not authorized |
| core × Claude | Rust transport/ownership/exit checks | Not run, not authorized |

Claude is pinned to installed `2.1.314 (Claude Code)` and its canonical executable, interactive argv without Codex flags, optionally `--model`. If additional real Claude acceptance is requested, a separate concrete two-instance proposal is supplied with the audit handoff; it is not executed or counted as authorized. The user's App-created Claude does not satisfy either CrewMoor Claude entrypoint test. Remote CI/required checks remain separate from local Rust tests and real startup evidence.
