# App project placement

This core mode launches Codex or Claude on an existing, confirmed App connection. Choose a project cwd and that connection's target profile; no manual Space creation or space/tab ID lookup is needed. CrewMoor records its own project placement IDs and sends official keep/adoption labels with `terminal.run`. The App creates unknown IDs or reuses their existing layout. Explicit existing-space binding remains supported. There is no GUI automation, connection creation, hooks or Agent status adapter.

## Versioned contract

Inspected core: installed `hc 0.1.15`, offline `hc schema`, source [4a3591aa](https://github.com/herdrdev/herdr-core/tree/4a3591aa835c197f57a5d1b8e8951a75e2a79faa). User-reported Windows App: `0.1.10`; the matching [v0.1.10 official AGENTS](https://github.com/herdrdev/herdr-app/blob/v0.1.10/defaults/AGENTS.md) resolves to `36725d47c89cd19345994363d6d9cdee5c42c80b`. The earlier master reference was `0c1a102`; the matching tag also explicitly supports unknown-ID creation. Installed binary provenance beyond the user's version report is not independently established.

The official chain is equivalent to:

```text
HC_SESSION=EXPLICIT_SESSION HC_SOCKET=EXPLICIT_SOCKET hc run --bg --no-parent --no-notify --keep \
  --name ROLE --cwd CWD --label crewmoor-owner=OWNER --label crewmoor-team=TEAM \
  --label herdr.space=LOCAL_SPACE --label herdr.tab=LOCAL_TAB -- /absolute/agent [args]
```

App code reconciles labelled live terminals into the machine's layout ([v0.1.10 source](https://github.com/herdrdev/herdr-app/blob/36725d47c89cd19345994363d6d9cdee5c42c80b/src/window.rs#L1954)). Unknown IDs become unnamed spaces/tabs; an existing tab receives a new pane while keeping its focus. `--keep` supports adoption. A machine registered with `--temp` is excluded from App layout; use a persistent user-confirmed connection. CrewMoor neither creates nor changes that connection.

**The implementation does not execute that CLI chain in App mode.** Core's [CLI call](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/cli.rs#L214) uses `connect_or_start`, which could start an external host. Instead a small existing-socket transport uses the official [plain JSON-RPC protocol](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/client.rs#L212): one JSON-RPC 2.0 request/reply per connection. Only `server.status`, read-only `doc.get` for `herdr-app`, and `terminal.run/get/kill/rm` are mapped. No fallback, retry, service start/stop or layout write is allowed. Read/write timeouts are ten seconds and replies are bounded to 1 MiB. Offline `--version` preflight still checks the installed binaries without starting a service.

## Project configuration

Save an already-confirmed target once, using [app-target.toml](../examples/app-target.toml). These are connection facts, not values guessed from a machine alias. For subsequent projects select that file through `app_target` and set `cwd` in the [Claude](../examples/app-project-claude.toml) or [Codex](../examples/app-project-codex.toml) startup configuration:

```toml
cwd = "/absolute/project"
app_target = "/absolute/crewmoor/targets/vps.toml"
```

The target file contains session/socket/core-id/machine. **Omit both space_id and tab_id** for project placement. The ordinary entry remains `crewmoor launch CONFIG FRESH_STATE`; inspect/stop use that same state. Target configuration is resolved before connecting; the receipt keeps the resolved snapshot, so editing a profile does not retarget an existing receipt. Supplying both `app_target` and an inline `[app]` is rejected.

`store/projects/index.json` records a project UUID and its local space/tab UUIDs, keyed by the confirmed core-id plus canonical cwd. A short file lock and atomic replacement keep concurrent creation from losing that association; contention/error withholds the request without waiting or recovery. IDs are persisted before the single Agent request and also returned in the receipt's `project` field. Automatic terminals carry a `crewmoor-project` label in addition to owner/team. Different aliases/symlink spellings of the same core/project reuse the record; different canonical directories or core identities get separate records.

Only this ownership record causes reuse. CrewMoor does not scan display names, directory labels or old native-probe spaces to adopt a same-named user Space. New IDs are checked for collision before recording, and unknown IDs are deliberately passed to native adoption. A zero-version missing document is treated as empty, as the App does. Invalid/duplicate records are not discarded to manufacture a new association. Preserve the store: deleting it loses the mapping. If the App has removed an empty project layout, the recorded IDs can be recreated by native adoption on a later explicitly requested launch. Stop never deletes the project record or explicitly deletes its Space/tab.

Space IDs are not display names. The App derives unnamed labels from its first terminal's project/folder, while unnamed tabs are numbered. No display-name promise is made from an ID. No rename is implemented. If a precise name is later required, the matching version documents `herdr-app do space.rename --id <qualified-space-id> --name <name>`; that would be a separately coordinated client action on the known project Space, not a reason to build a GUI layer.

## Explicit existing-space configuration

Add an `[app]` table to a core configuration, as in [the non-runnable template](../examples/app-claude.toml). No default core, space or tab is inferred:

```toml
[app]
session = "user-session"
socket = "/absolute/existing/core.sock"
core_id = "user-confirmed-core-id"
machine = "user-machine"
space_id = "existing-local-space-id"
tab_id = "existing-local-tab-id"
```

Use the stable session/socket already connected by the App. `server.status` must match the configured core id, installed core version and recorded server fingerprint. StatusResult has no session field: the session is explicit routing intent associated with that socket by the user, not invented server evidence. `machine` is the App client's connection name, not a request to route the server to another machine.

Space/tab IDs are machine-local. An ID prefixed with exactly the configured `machine/` is accepted and that prefix alone is removed; another prefix or an additional slash is rejected ([v0.1.10 mapping](https://github.com/herdrdev/herdr-app/blob/36725d47c89cd19345994363d6d9cdee5c42c80b/src/mirror.rs#L65)). Both IDs are required in this explicit mode; a half-specified pair is rejected. A read-only `doc.get {name: "herdr-app"}` checks that the selected space contains that tab. Missing explicit targets withhold the request. This stricter **explicit-mode policy is not a native App limitation**: automatic project mode intentionally permits new owned IDs. The layout is neither stored in the receipt nor written by CrewMoor. Do not delete/reassign selected IDs during launch.

Existing host environment cannot be repaired by sanitizing a client. Before `terminal.run`, only the explicit developer metadata/CI/color marker keys in `app::HOST_MARKERS` are checked in server/holder environments; no values, credentials or full environment are printed or stored. Contamination or unreadable/changed process identity withholds startup. Authentication, CODEX_HOME, tools, PATH/HOME and all proxy settings are preserved. This is a marker check, not a claim that arbitrary host environment is clean. Use a user-managed uncontaminated host; the old held core is never a target.

## Location, input and stop

The receipt records the external session/core, `server_owned=false`, `binding_scope=terminal`, returned local address and the client-qualified `app_terminal` such as `user-machine/t7`. Configured App target is an intended route, not proof that a Windows UI adopted it. Codex uses its canonical executable plus `--no-daemon` once; Claude uses its own canonical executable and only its supported optional `--model`. No prompt/task is sent. Admission and kernel executable/cwd/process facts remain separate; all Agent semantic status is unknown.

Stop verifies the returned terminal's owner/team/generation/launch tuple, calls native kill/rm on that address and confirms target absence and recorded Agent exit. It **never stops the external server/holder or removes its socket**, even if empty. `target-stopped-host-retained` and released terminal binding report these facts separately. Unknown terminal handle/removal/exit retains the terminal binding and evidence. No other terminal, space/tab or Windows entry is removed.

## User acceptance — stop here until coordinated

1. Choose the project cwd and a previously confirmed persistent target profile. Internal socket/core-id facts can be obtained read-only by the coordinator; the user does not have to find Space/tab IDs. No password/token is needed. Old held resources are not test targets.
2. After exact-candidate audit and specific real-start permission, launch once. The user finds the new/reused project Space, verifies the receipt's same Agent/address and types directly. Record the actually observed display name separately. No `terminal.show`, manual Space creation or Agents-panel gate is needed.
3. Once the user confirms observation/input is finished, run owned stop once. Confirm that terminal/Agent ended while the host, user terminals and space/tab remain. No extra launch is authorized by failure or unknown.

See [the evidence layers](issue-2-current-evidence.md): a1/v11's explicit App Claude case has user-confirmed display/input and owned stop; two later external native auto-Space probes are separate. The new automatic **CrewMoor entrypoint** still requires its own exact-candidate review and newly authorized real acceptance. Synthetic tests and native probes do not replace it.
