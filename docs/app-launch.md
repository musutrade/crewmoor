# Existing App target

This optional core mode launches Codex or Claude into an **existing user-selected** App space/tab. It uses official adoption labels rather than `terminal.show`'s temporary swap. It creates only a terminal; no space/tab, machine connection, GUI automation, hooks or Agent status adapter is implemented. Actual client display/input remains unverified for this candidate.

## Versioned contract

Inspected core: installed `hc 0.1.15`, offline `hc schema`, source [4a3591aa](https://github.com/herdrdev/herdr-core/tree/4a3591aa835c197f57a5d1b8e8951a75e2a79faa). Inspected App: [official AGENTS](https://github.com/herdrdev/herdr-app/blob/0c1a10269b1f8bb0c8e6c18aec4c084757104c39/defaults/AGENTS.md), revision `0c1a10269b1f8bb0c8e6c18aec4c084757104c39`. This is a reference revision, **not a verified Windows installation build**. The user must confirm the installed App's generated AGENTS/contract before real acceptance.

The official chain is equivalent to:

```text
HC_SESSION=EXPLICIT_SESSION HC_SOCKET=EXPLICIT_SOCKET hc run --bg --no-parent --no-notify --keep \
  --name ROLE --cwd CWD --label crewmoor-owner=OWNER --label crewmoor-team=TEAM \
  --label herdr.space=LOCAL_SPACE --label herdr.tab=LOCAL_TAB -- /absolute/agent [args]
```

App code reconciles labelled live terminals into the machine's layout ([source](https://github.com/herdrdev/herdr-app/blob/0c1a10269b1f8bb0c8e6c18aec4c084757104c39/src/window.rs#L1870)). `--keep` supports adoption. A machine registered with `--temp` is excluded from App layout; use a persistent user-confirmed connection. CrewMoor neither creates nor changes that connection.

**The implementation does not execute that CLI chain in App mode.** Core's [CLI call](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/cli.rs#L214) uses `connect_or_start`, which could start an external host. Instead a small existing-socket transport uses the official [plain JSON-RPC protocol](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/client.rs#L212): one JSON-RPC 2.0 request/reply per connection. Only `server.status`, read-only `doc.get` for `herdr-app`, and `terminal.run/get/kill/rm` are mapped. No fallback, retry, service start/stop or layout write is allowed. Read/write timeouts are ten seconds and replies are bounded to 1 MiB. Offline `--version` preflight still checks the installed binaries without starting a service.

## Explicit configuration

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

Space/tab IDs are machine-local. An ID prefixed with exactly the configured `machine/` is accepted and that prefix alone is removed; another prefix or an additional slash is rejected ([official local mapping](https://github.com/herdrdev/herdr-app/blob/0c1a10269b1f8bb0c8e6c18aec4c084757104c39/src/mirror.rs#L61)). Display names are not IDs. A read-only `doc.get {name: "herdr-app"}` checks that the selected space contains that tab before startup. Unknown/unavailable layout withholds the request, because the App would otherwise create unknown labelled IDs. The layout is neither saved in the receipt nor written by CrewMoor. A concurrent user layout change remains outside this single-call preflight; do not delete/reassign the selected space/tab during launch.

Existing host environment cannot be repaired by sanitizing a client. Before `terminal.run`, only the explicit developer metadata/CI/color marker keys in `app::HOST_MARKERS` are checked in server/holder environments; no values, credentials or full environment are printed or stored. Contamination or unreadable/changed process identity withholds startup. Authentication, CODEX_HOME, tools, PATH/HOME and all proxy settings are preserved. This is a marker check, not a claim that arbitrary host environment is clean. Use a user-managed uncontaminated host; the old held core is never a target.

## Location, input and stop

The receipt records the external session/core, `server_owned=false`, `binding_scope=terminal`, returned local address and the client-qualified `app_terminal` such as `user-machine/t7`. Configured App target is an intended route, not proof that a Windows UI adopted it. Codex uses its canonical executable plus `--no-daemon` once; Claude uses its own canonical executable and only its supported optional `--model`. No prompt/task is sent. Admission and kernel executable/cwd/process facts remain separate; all Agent semantic status is unknown.

Stop verifies the returned terminal's owner/team/generation/launch tuple, calls native kill/rm on that address and confirms target absence and recorded Agent exit. It **never stops the external server/holder or removes its socket**, even if empty. `target-stopped-host-retained` and released terminal binding report these facts separately. Unknown terminal handle/removal/exit retains the terminal binding and evidence. No other terminal, space/tab or Windows entry is removed.

## User acceptance — stop here until coordinated

1. Confirm installed Windows App build/generated AGENTS and a persistent machine connection to an uncontaminated stable core. Supply only the session/socket/core id, machine name and an existing space/tab's local IDs; no password/token is needed. Do not reuse any held core or user test target.
2. After exact-candidate audit and specific real-start permission, launch once with the pinned artifact/config. User opens the existing selected space/tab, verifies the receipt's same Agent/address and types a short response directly. Do not use `terminal.show`, create a temporary space, or treat Agents-panel status as acceptance.
3. Once the user confirms observation/input is finished, run owned stop once. Confirm that terminal/Agent ended while the host, user terminals and space/tab remain. No extra launch is authorized by failure or unknown.

Historical App observation only proved session connectivity and temporary display of old t1 via `terminal.show`; it did not prove formal adoption or direct response for this mode. Synthetic tests are not a substitute for these client steps.
