# Historical minimum real-acceptance proposal — completed

The two bd4/v13 cases below were subsequently specifically authorized and completed. PR #5 merged and Issue #2 closed; see [final evidence](issue-2-current-evidence.md). The following proposal is retained as history and grants no new launch permission.

Implementing/reviewing project placement grants no new real launch. Earlier App and native probes are complete; their permissions cannot be replayed. Exact new SHA/artifact/config hashes and full commands are supplied outside the repository after automatic validation. Execute only after that exact candidate passes independent review and the user explicitly chooses the named action(s). No automatic task, input, trust/auth answer or retry.

The smallest complementary proposal is **two separate launch operations**, not an automatic four-combination matrix:

1. **Project entrypoint, core/App Codex**, cwd `/home/gem/crewmoor`, confirmed existing `vps` target profile, fresh state `/tmp/cm-i2-proj1`, team `issue2-project-core`, role `cm-project-codex`. Omit space/tab IDs. Verify one admitted request and actual canonical Codex `0.166.0` process/argv/cwd; record project ownership and new native handles. The user checks the same terminal in the App's new/reused project Space, reports the actual display name, and types a short response directly. After explicit observation completion, only that receipt's owned terminal is stopped. Existing default host, user terminals and layouts remain; no explicit space/tab delete.
2. **Private TUI Claude**, cwd `/home/gem/crewmoor`, fresh state `/tmp/cm-i2-tcl2`, team `issue2-tui-final`, role `cm-tui-final`. Official TUI `0.9.5` workspace-create → returned pane → agent-start with kind Claude `2.1.316` and no Codex flags. Reuse the official startup wait; one process observation, no custom busy/ready layer. End only its returned pane/workspace and an empty private session; extras mean partial retention.

Both Agents are implemented and automatically tested. These two actions minimize new real effects while addressing automatic entrypoint plus TUI gaps; they are not a claim that every Agent/runtime combination has new-head real evidence. An additional App/Claude or TUI/Codex test would require its own named permission. Existing a1 App/Claude evidence and the two native probes may be cited only in their [original layers](issue-2-current-evidence.md).

For each approved action, check artifact/config hashes, current binary versions and unused state/team binding before one launch. Startup may read existing credentials/config/plugins/MCP and contact upstream services with possible account usage; user-chosen input may also use the model. No installation, proxy/shell/.env change, automatic opening prompt, new core for App mode or old-resource operation is included. User decisions on native prompts remain with the user.

Expected command shape, using the pinned future artifact:

```text
crewmoor launch EXACT_CONFIG FRESH_STATE
crewmoor inspect SAME_STATE
crewmoor stop SAME_STATE
```

Launch admission, recorded kernel process facts, user GUI observation and owned-stop result are separate. At most one additional inspect is proposed, no progress polling. Whitelisted marker checks expose names only. Agent semantic status stays unknown. Native failure/timeout/unknown never authorizes another launch; a lost address keeps evidence/binding and is reported without guessing cleanup targets.

Automatic project IDs persist under the selected store after stop; later launches reuse their own mapping. A same-named user Space is not adopted, and old native-probe IDs are not imported silently. Unknown IDs are allowed specifically because official adoption supports them. No precise display name is promised from an ID. Official rename, if actually needed, is only a separate minimal client option in [App usage](app-launch.md), not implemented or executed here.

No proposed state or real target is created by this document. Old `cm-0c926f847d55`, original user terminals such as t1n, existing gem/1 and Windows objects remain HOLD. CI and draft PR #5 are the publisher's responsibility; local verification/real case results do not authorize publication, merge or closing Issue #2.
