# Bounded startup acceptance — exact revised audit pending

Batch `crewmoor-issue-2-001`. The user retained authorization for one new isolated TUI Codex and one new isolated core Codex startup/check/owned stop. Neither attempt has been consumed. Rejected v6/v7 must not run. Execute only after independent pass for the exact simplified candidate and the v8 artifact/config hashes in the external `next-real-plan.json`. Implementation now supports Codex and Claude, but this authorization does not expand to Claude or four attempts.

The minimum acceptance is: report request admission accurately, prove actual Agent PID/start/executable/cwd, locate instance/logical-role/native handle and safely end only owned resources. Ready/idle/working/done and App Agents membership are not criteria; Agent feedback belongs to another issue. No feedback protocol or messages are implemented.

## Pinned inputs and commands

Expected installed versions: TUI 0.9.5, core 0.1.15, Codex 0.166.0, Rust stable 1.99.1/edition 2024. CrewMoor 0.1.0 is bound to the audit's exact SHA, not merely its package version. TUI requires its recorded original Bash shell; version/type must be recorded during preflight. Artifacts are snapshots, not PATH installations. No new software, downgrade, compatibility fallback or proxy/config changes.

| Runtime | Agent | Team / role | State | cwd | External config |
|---|---|---|---|---|---|
| TUI | Codex | `issue2-tui-r3` / `cm-tui-r3` | `/tmp/cm-i2-tui3` | `/tmp/cm-i2-tui3-work` | `tui-r3.toml` |
| core | Codex | `issue2-core-r2` / `cm-core-r2` | `/tmp/cm-i2-core2` | `/tmp/cm-i2-core2-work` | `core-r2.toml` |

Before execution verify all exact hashes/versions, unused state roots/team bindings, and create only those empty cwd directories. Sessions/native handles come from actual receipts. New core uses only its private named Unix socket; no TCP, SSH/default-session change or Windows/App mutation.

With artifact directory `/home/gem/.local/state/crewmoor/crewmoor-issue-2-001`:

```text
.../crewmoor-run-v8 launch .../tui-r3.toml /tmp/cm-i2-tui3
.../crewmoor-run-v8 inspect /tmp/cm-i2-tui3
.../crewmoor-evidence-v8 environment-evidence /tmp/cm-i2-tui3
.../crewmoor-run-v8 stop /tmp/cm-i2-tui3

.../crewmoor-run-v8 launch .../core-r2.toml /tmp/cm-i2-core2
.../crewmoor-run-v8 inspect /tmp/cm-i2-core2
.../crewmoor-evidence-v8 environment-evidence /tmp/cm-i2-core2
.../crewmoor-run-v8 stop /tmp/cm-i2-core2
```

Each launch has one startup mutation. TUI now uses official `pane run` on its proved original empty Bash shell, with one Rust-encoded `command <absolute executable> <argv>` string. Bash hex quoting transports whitespace/quotes/control bytes/metacharacters as data. Codex receives `--no-daemon` exactly once and `--no-alt-screen`; no opening task/prompt. Core uses direct argv with that same canonical executable. There is no Agent ready wait or state polling. Native CLI calls have a 10-second limit plus at most one second to reap a timed-out owned CLI child.

Admission and process are separate: TUI exit 0/empty stdout acknowledges input admission only; a failure/timeout leaves `accepted=null` and never resends. One process observation may still prove the exact Agent. If it does not, report unknown/unconfirmed; do not add waits, loops, another process or another launch. Optional native display fields do not become fake identity or semantic state.

The read-only Rust evidence probe reads only guarded receipt-owned PIDs, verifies fingerprints before/after access and reports whitelisted parent-marker names, skipping values. It does not print credentials/full environment. Keep all receipt, admission/error, native owner/id, original-shell/holder and actual process evidence. Synthetic tests establish tool/auth/proxy preservation separately.

## End only proved resources

Stop requires the receipt's server/socket/binding, exact native ids/owner and exclusive inventory. Missing original shell, Agent PID/foreground or holder proof refuses; no discovery/adoption during stop. Unknown admission does not by itself prevent stop when all ownership/process evidence is complete. Missing optional display name does not weaken or block that proof.

Success must show native target not-found, actual server/Agent/ending/original-shell/holder exit as applicable, API socket absence and binding release. Preserve state/logs. A failure without any target may end only its verified empty inventory using recorded server/socket/container/holder identity. Unknown metadata, replacement, extra resources or timeout preserves binding/evidence and is reported; no force cleanup or retry.

## Scope and unverified combinations

Startup may access existing authentication/config/plugins/MCP and upstream network/cost, as authorized for these two Codex attempts. No zero-cost claim. The old core `cm-0c926f847d55`, its t1 and all user-created objects/Windows entries remain under HOLD, and are not queried, changed or used as cleanup tests. No new App observation is needed for these Linux attempts.

| Combination | Automatic evidence | Revised real evidence |
|---|---|---|
| TUI × Codex | Rust transport + Bash argv/kernel checks | Authorized once, pending exact audit |
| core × Codex | Rust transport/ownership/exit checks | Authorized once, pending exact audit |
| TUI × Claude | Rust transport + Bash argv/kernel checks | Not run, not authorized |
| core × Claude | Rust transport/ownership/exit checks | Not run, not authorized |

Claude is pinned to installed `2.1.314 (Claude Code)` and its canonical executable, interactive argv without Codex flags, optionally `--model`. If additional real Claude acceptance is requested, a separate concrete two-instance proposal is supplied with the audit handoff; it is not executed or counted as authorized. The user's App-created Claude does not satisfy either CrewMoor Claude entrypoint test. Remote CI/required checks remain separate from local Rust tests and real startup evidence.
