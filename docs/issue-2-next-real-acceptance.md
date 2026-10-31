# Next real acceptance plan — authorized scope, audit pending

Batch `crewmoor-issue-2-001`. The user authorized these two single attempts and retained that scope after the v6 audit failure; neither launch was consumed. **Do not run rejected v6 or any replacement until the exact revised SHA/artifacts receive independent code-audit pass.** The coordinator's retained authorization covers implementation corrections with unchanged scope/effects; no repeated permission request is needed after that pass. The current core `cm-0c926f847d55`, all its terminals and the user's Windows machine/space entries remain under HOLD. Do not reuse them, retrofit their receipts, send tasks, restart them or test cleanup on them.

The coordinator receives the exact candidate SHA, binary SHA-256 values and config paths in the external `next-real-plan.json` accompanying the audit request. Do not execute a different build. Planned versions: CrewMoor 0.1.0 at that SHA, installed TUI 0.9.5, core 0.1.15, Codex 0.166.0, Rust stable 1.99.1/edition 2024. Version mismatch stops the attempt; no installation, downgrade or compatibility fallback. Building/copying evidence artifacts does not start a runtime.

## Two separate, single attempts

After the revised exact candidate receives independent audit pass, verify its artifact/config hashes and versions under the retained authorization, then create only the two empty scratch cwd directories below. Require unused state roots and team bindings; launch each runtime once. Sessions/native ids come from actual receipts, not guessed Windows discovery names.

| Runtime | Team / role | State root | cwd | Prepared config |
|---|---|---|---|---|
| TUI | `issue2-tui-r3` / `cm-tui-r3` | `/tmp/cm-i2-tui3` | `/tmp/cm-i2-tui3-work` | external `tui-r3.toml` |
| core | `issue2-core-r2` / `cm-core-r2` | `/tmp/cm-i2-core2` | `/tmp/cm-i2-core2-work` | external `core-r2.toml` |

Both use the same existing CrewMoor binding namespace, unique team names and private XDG roots. Core uses a fresh named Unix socket under `/run/user/1000/herdr-core`; no TCP listener, SSH/proxy/global configuration change or default-session replacement. TUI omits an explicit `--no-daemon`; core includes it once. No model task, opening message, trust/auth answer, plugin installation or automatic retry.

Commands, with the pinned artifact directory `/home/gem/.local/state/crewmoor/crewmoor-issue-2-001`:

```text
.../crewmoor-run-v7 launch .../tui-r3.toml /tmp/cm-i2-tui3
.../crewmoor-run-v7 inspect /tmp/cm-i2-tui3
.../crewmoor-evidence-v7 environment-evidence /tmp/cm-i2-tui3
.../crewmoor-run-v7 stop /tmp/cm-i2-tui3

.../crewmoor-run-v7 launch .../core-r2.toml /tmp/cm-i2-core2
.../crewmoor-run-v7 inspect /tmp/cm-i2-core2
.../crewmoor-evidence-v7 environment-evidence /tmp/cm-i2-core2
.../crewmoor-run-v7 stop /tmp/cm-i2-core2
```

One bounded read-only inspect after startup may establish readiness; do not loop or relaunch to replace unknown/error. TUI requires native interactive-ready/Agent status; core requires live/reported Codex OSC status. Request acceptance, kernel process identity and ready are separately saved; any layer lacking evidence stays unknown/error. Failure ends that runtime's attempt and is reported, with only the same strictly owned failed instance eligible for guarded cleanup. It does not create another attempt.

The Rust evidence binary reads only processes identified by the task's guarded receipt, checks fingerprints before/after access, and extracts a fixed list of parent-session/tool/CI/color marker **names**, skipping values. Server/Agent plus original shell or holder must have no such markers. It never prints complete environment or credentials. Synthetic tests separately establish auth/tool/proxy preservation; real testing does not dump those values. Native terminal routing/identity variables generated for the new PTY are expected and remain distinct from forbidden parent metadata.

## Controlled end evidence

Before stop, save core/session id, team/role/cwd, native handles, owner labels, exact Agent/server/holder or original-shell fingerprints and exclusive inventories. The receipt must already own those identities from launch. On replacement, extra resources, unknown metadata, transport error or timeout, refuse and preserve binding/evidence. No forced signals to unrelated PIDs, no manual deletion of target resources, no new Agent startup for cleanup.

Successful stop must prove native target not-found, server/Agent/ending/original-shell/holder process exit as applicable, primary socket absence and binding release. Keep stopped state/logs and diagnostic artifacts. A failed launch with no target can end only a verified empty native inventory using its persisted server/socket/container/holder ownership. Where identity was never captured, report manual recovery required rather than inventing it.

## App and authorization remain separate

Retained user authorization covers existing authentication/config loading and possible plugins/MCP/upstream network/cost for these two single attempts and cleanup of only their own resources; no zero-cost claim. Execution still requires independent pass for the exact revised SHA/artifacts. Installation, proxy/config changes, extra attempts and Windows/App mutations remain outside this scope.

Windows App build is still unknown. Old t1 terminal display is user-verified; external t1 Agents-panel membership is not. If a new App observation is requested, first report the actual new session/native target and obtain the App build/machine/active-space identity without credentials. Do not assume `--temp` implies App layout discovery or that aliases equal internal machine ids. Do not create a space, terminal, Agent or membership label for the user automatically. Keep the observation target alive until the user finishes. Extra App resources cause stop refusal and require user coordination; they are never reassigned to CrewMoor. No Windows mutation is included in this plan's two Linux launch attempts.

Remote CI/required checks and independent audit bind the new candidate separately. Successful Rust tests do not satisfy any missing real or App acceptance item.
