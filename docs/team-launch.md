# Team start, bind and stop

Issue #15 adds three thin entries that join Issue #2 launch, Issue #4 feedback
registration and Issue #9 message binding. They add no waiting loop, ready gate,
queue, retry, recovery or role replacement.

```text
crewmoor team start /absolute/team.toml /absolute/state
crewmoor team bind  /absolute/state/teams/TEAM WORKER_ROLE
crewmoor team stop  /absolute/state/teams/TEAM
```

## Generate factual fields from the existing environment

```text
crewmoor team generate /absolute/template.toml
crewmoor team generate /absolute/template.toml --output /absolute/new-team.toml
```

Choose task inputs in [the TUI template](../examples/team-generate-tui.toml) or
[the core template](../examples/team-generate-core.toml) first. `team`, `task`,
`runtime`, `feedback_registry`, `evidence`, `dispatcher`, role names, existing
absolute `cwd`, Agent kinds, `agent_args`, `initial_prompt` and any role `tab_id`
come from that file. The generator does not select a model, invent a startup
prompt, canonicalize business cwd choices or create a role. Existing complete
team configurations also work as templates; only factual version/path fields
are replaced.

Omit `runtime_binary` to look up `herdr` (TUI) or `hc` (core) on the current PATH;
omit a role's `agent_executable` to look up its explicit Agent kind (`claude` or
`codex`). An explicit executable is a file path, relative to the invocation cwd
or absolute. It must exist and be executable: no PATH/default fallback occurs
for an explicit hint. The selected file is canonicalized, and that same actual
binary's `--version` supplies `runtime_version` / `agent_version`. Supplied stale
version strings are replaced. User authentication, tool and proxy configuration
are preserved; only inherited Agent/container routing markers are removed by the
existing native-query helper.

For TUI, `[host].socket` is a mandatory explicit selection of an existing server.
The generator calls `status server --json` on that endpoint and uses its actual
`version` and `socket` for `host.server_version` / `host.socket`, checking the
returned socket against the selected endpoint. The client version stays separate:
a client `herdr 0.9.5` with a server `0.9.4` generates those distinct values. No
server version is inferred from the client executable or terminal/process state.

For core, `app_target` must name an existing absolute profile. Create one
separately with `app-target generate` if needed; team generation never creates or
rewrites that file. It reuses the existing-socket `server.status` and
`doc.get {name: "herdr-app"}` discovery, checks `core.id` against the selected
profile, and uses existing App/layout validators for explicit placement and role
tabs. Profile bytes, explicit session, App machine/ID prefix and space/tab choices
are kept. The machine routing name need not equal `server.status.core.name`;
session is not a server status field. A stale core identity fails for explicit
profile regeneration. The present launch/bind contract uses the same version for
core client and server, so generation **rejects** a core mismatch rather than
claiming compatibility or adding another version field.
Generation also reuses the launch host guard: the selected core server and its
reported holders must have valid process identities and no developer routing
metadata. A refusal emits no configuration and does not alter the existing host.

Explicit `space_id` must already exist. The target's `tab_id` and role-level
overrides may name existing or new tabs; all use the same placement validator
as launch. `team start` checks the selected space before writing team state,
including teams with no role tab overrides. The connected App creates unknown
tabs through native adoption labels and removes empty new tabs after their
terminals end. Typing a different valid ID creates an extra tab. ID syntax and
exact configured machine-prefix checks remain, and CrewMoor never writes layout
documents. Project placement with both target IDs omitted is unchanged.

Discovery and ordinary launch-option validation finish before any output.
Stdout mode contains valid reviewable TOML only; errors go to stderr.
`--output` uses `create_new`, refusing existing files and symlinks. No team state,
hidden target sidecar, Agent, service or layout is created. Discovery is a
point-in-time observation; `team start` still rechecks actual versions and
identities. Review the generated file, then pass it directly to `team start`.

## Team directory

`team start` creates `<state>/teams/<team>/` once and prints its absolute path.
An existing directory is refused, never overwritten. Restarting a team uses a
new state directory; the old one stays as evidence. Hand the returned path and
role locations to the team-owned dispatcher during startup handover. There is no
environment variable or discovery.

| Path | Writer | Content |
|---|---|---|
| `team.json` | `team start` | Snapshot of the team configuration |
| `roles/<role>/context.json` | `team start` | The role's Issue #4 feedback context |
| `roles/<role>/launch/record.json` | Issue #2 launch | Launch record: handle, admission, process facts |
| `<team>.<role>.json` | Issue #2 launch | Owner binding of that role's launch record |
| `roles/<worker>/binding.json` | `team bind` | That worker's pair binding with the dispatcher |

## Human entry and one dispatcher

The user-facing Agent is the `dispatcher` role started and recorded by this
team. The launcher selects that role in returned `roles[]`, hands over its
`handle`, `feedback_instance` and `team_dir`, and ends its startup role. The user
opens that TUI pane or App/core terminal and enters requirements directly.
Human input is ordinary native input, without a second Agent, a self-addressed
Message or a new binding. Agent-to-Agent business traffic uses
`crewmoor message send` through the worker pair bindings.

The shortest existing chain is:

1. `team start CONFIG STATE`: register and launch the dispatcher first, then workers;
   read returned role/handle/instance identities. Admission or a located process
   is not hooks state, readiness or business completion; an unknown start outcome
   must not be blindly repeated.
2. Supply the team directory, CLI path and exact binding paths to the dispatcher.
   A Codex or OpenCode `initial_prompt` may carry startup instructions through native argv,
   before later team messages. Keep workers waiting for assignments.
3. After both native associations exist (SessionStart for Codex/Claude, actual
   session.create for OpenCode), run
   `team bind TEAM_DIR WORKER_ROLE` once per worker. Report a missing session or
   binding once, without adding a polling loop or ready gate.
4. The user continues directly in the dispatcher pane. The dispatcher authors
   a flat request with a unique `id` and sends it through
   `TEAM_DIR/roles/WORKER_ROLE/binding.json`. The worker actively replies through
   that same binding with a new `id` and `reply_to` equal to the request ID.
   The transport validates identifiers, not a conversation history.
5. Bound worker Stop/SessionEnd events are forwarded notices, not business
   completion. They need no generic acknowledgement. The dispatcher's own events
   stay local; there is no upstream Agent. Hook automatic issue continuation is
   paused pending [#51](https://github.com/musutrade/crewmoor/issues/51); manual
   issue commands remain available.

Do not retain an outer coordination Agent, import an existing session or bypass
the fixed bindings with native `herdr agent prompt` / `hc send`; those are
transport implementation details. If either role's native session changes,
normal sends involving it refuse, `feedback show` exposes the rejection and the
original binding remains unchanged. Report the affected role to the user; do not
rebind, restart or adopt its replacement automatically. A worker halt notice can
reach an unchanged dispatcher; a changed dispatcher has no upstream forwarding
promise.

The versioned [crewmoor-team-flow source](../skills/crewmoor-team-flow/SKILL.md)
defines the protocol, reply correlation and Stop/halt handling. It is a reviewable
repository artifact, not an automatic global-instructions update. To synchronize
an authorized reviewed revision locally, record exact installed paths and hashes,
back up each existing file outside its skill directory, compare with the reviewed
source, copy that file into the existing skill directory, then verify identical
bytes. Preserve the backups for restoration; do not replace files changed since
the plan. This does not create a symlink or edit other skills, memory or settings.

## Configuration

Team and role names follow the same rule (1..24 lowercase letters, digits or `-`)
and are checked before anything is written, and again whenever `team.json` is
loaded, so a role name can never point outside its team directory.

`runtime` is declared once for the whole team; a role cannot set its own runtime
(`deny_unknown_fields` rejects it). One team uses one container. `team start`
accepts `runtime = "tui"` with an existing `host`; see
[examples/team-tui.toml](../examples/team-tui.toml). `runtime = "core"` uses an
existing `app_target`; see [examples/team-core.toml](../examples/team-core.toml)
and [App launch](app-launch.md). Both use the same team start/bind/message chain
and native Hook identities. Runtime creation differs: TUI uses workspace/Agent
creation; App/core uses its existing socket and `terminal.run` with per-role env.

```toml
team = "issue15-smoke"
task = "issue-15-smoke"
runtime = "tui"
runtime_binary = "/home/gem/.local/bin/herdr"
runtime_version = "herdr 0.9.5"
feedback_registry = "/home/gem/.local/state/crewmoor/feedback"
evidence = "native"
dispatcher = "lead"

[host]
socket = "/home/gem/.config/herdr/herdr.sock"
server_version = "0.9.5"

[[roles]]
role = "lead"
cwd = "/absolute/project"
agent = "claude"
agent_executable = "/home/gem/.local/bin/claude"
agent_version = "2.1.316 (Claude Code)"

[[roles]]
role = "worker"
cwd = "/absolute/project"
agent = "codex"
agent_executable = "/home/gem/.local/bin/codex"
agent_version = "codex-cli 0.166.0"
agent_args = ["--no-alt-screen"]
initial_prompt = "You are the worker. Wait for a team message before acting."
```

`feedback_registry` must be the registry that the user-level Hook callback was
installed with, because the global callback only reads that registry.

## Start: one workspace per role in the user's default server

The team runs in the user's existing Herdr server (`host`). This is the same
server used by the #9 and #6 real acceptance and by the env probe. `team start`
launches the dispatcher first, then each worker. For each role it:

1. registers a feedback context without a session id;
2. calls Issue #2 launch with `host` and `feedback_instance` set;
3. launch runs `workspace create --cwd CWD --label OWNER --no-focus --env CREWMOOR_FEEDBACK_INSTANCE=INSTANCE`,
   then the official `agent start ROLE --kind KIND --pane PANE --timeout 5000 -- ARGS [INITIAL_PROMPT]`.

In host mode, CrewMoor does not boot, own or stop the server. It checks
`status server` for the configured `server_version` and `socket`, and records the
socket path and inode as the instance identity. `server_owned=false` and
`binding_scope=workspace`. The private-server launch path (no `host`) is
unchanged.

A failed role stops `team start` with that role named. Already-started roles stay
recorded; use `team stop` on the same directory.

### Codex initial prompt

Codex creates its session only when the first prompt arrives, so an idle Codex has
no SessionStart to bind to. For Codex, `initial_prompt` passes exactly one
startup task through native `codex [PROMPT]`. It must be 1..4096 bytes, have no
control characters and not start with `-`. Other positional arguments are still
rejected. Claude takes no initial prompt. OpenCode uses its native --prompt on
the actual interactive child, after its API main session has been established.

## Bind: one pair per worker, after the native session association

`team bind TEAM_DIR WORKER` binds one worker with the dispatcher and writes
`roles/<worker>/binding.json` once. If either role has no session registered by
its own SessionStart Hook, bind fails immediately. There is no wait or retry;
run bind again later by hand. The dispatcher's own events are not forwarded.
Several workers get separate pair bindings; there is no registry.

Bind also checks that the bound pane, terminal, workspace and Agent process are
exactly the ones recorded by this team directory's launch records.

## Identity rules

- **Agent identity comes from the Agent.** The Hook-registered session (or an
  explicit registration session) is authoritative.
- **Location uses process identity.** TUI uses `pane process-info` to find the
  single foreground process of the Agent kind, then checks PID, start time,
  executable and cwd. Core checks terminal generation, argv, PID and cwd. A new
  TUI binding must supply each peer's `agent_executable` and always locates the
  process; only bindings saved before #15 may lack a process identity.
- **Herdr `agent_session` is an extra check.** When present it must equal the
  registered session. When absent (idle Codex), the bound process identity is
  required instead. Bindings saved before #15 still load; they keep their earlier
  `agent_session` check.
- Marker injection uses the Herdr env capability that was measured. If a future
  container without env support causes trouble, go back to Agent-native means
  (Claude `--settings`, Codex `-c`) rather than per-container adapters. Neither is
  implemented here.

OpenCode v2 uses [its fixed main-session API](opencode.md): an API-created session
and private endpoint are pinned to the role; browsing another UI page leaves that
binding unchanged. Its Herdr root/subtree report is not a strict page getter.
Model/profile/variant are native session options, and startup text uses --prompt,
not an unsupported root --model. The following session-change Hook rules retain
their existing meaning for Codex/Claude.

## Session change halts the team

A different session for a registered role is an exception. CrewMoor does not
correct it, adopt the new session or replace the role. When a Hook event or
explicit report arrives with a different session, the journal appends one record:

```json
{"source":"session-rejected","event":"SessionStart","native_session_id":"NEW",
 "native":{"source":"clear","from":"native-hook","associated_session":"OLD"}}
```

The check reads the journal without taking the journal lock, so a Hook append is
never refused because of it. It uses only completed (newline-terminated) records;
an unfinished last line is left for the next read, while a corrupt completed
record or a read error is an error. Once the Agent's own Hook has produced such a record for a role, `message bind`,
`team bind` and every `message send` (and so `report-to` forwarding) involving
that role are refused with "team halted". The refusal is read-only: the saved
binding and the associated session are kept, and the new session is never
adopted. This holds even when the container has no session field and the Agent
process is unchanged (Codex `/clear` keeps its process). A rejected explicit
report alone does not halt messaging; it is the reporter's input, not a Hook fact.

`feedback show` returns it as `latest_session_rejection`; `agent_state` stays
`unknown` and the record is not a business report. Exit is recorded by the
Agent's own `SessionEnd`. Recovery is the user's decision: `team stop`, then start
again with a new team directory. Context compaction keeps the session id and is
normal operation.

The Hook callback reports rejections only if the installed callback binary is a
#15 build. An older installed callback still refuses the event but writes no
record.

## Stop

`team stop TEAM_DIR` calls the existing Issue #2 stop once for each launched role.
Roles already ended are skipped. In host mode stop closes only the role's own pane
and, when it is empty, its own workspace. It never calls `session stop`. A role
whose stop fails is reported; the others are still attempted, and evidence is
retained.

## Container capabilities

Container differences are still `match Runtime` branches in existing modules.
New #15 code calls those per-container functions and adds no new scattered
branch. A third container should first provide the capabilities below; a shared
interface can be extracted then.

The following table preserves the historical #15 capability snapshot, including
its server 0.9.4 / core 0.1.15 versions and then-unverified core path. It does not
declare current installed versions or acceptance. Current teams pin their actual
runtime/host versions (TUI CLI/server 0.9.5 in the 2026-11-03 #40 batch); use the
current runtime chains and examples above.

| Capability | Herdr TUI 0.9.5 (server 0.9.4) | herdr-core 0.1.15 |
|---|---|---|
| Create and start with env | `workspace create --env K=V`, then `agent start --pane` | App RPC `terminal.run` with `env` object. The `hc run` CLI has no env flag, so core markers need an App/core target. **Not real-accepted.** |
| Return a locating handle | `pane_id`, `terminal_id`, `workspace_id` from `workspace create` | `address` from `run`, plus `created_ms` from `get` |
| Process identity | `pane process-info` foreground PID → `/proc` start time, exe, cwd | `terminal.get` PID → `/proc` start time, exe, cwd |
| Send text | `agent prompt PANE TEXT` | `terminal.send` |
| End only own resources | `pane close`; `workspace close` when it is empty; host never stopped | `terminal.kill` / `terminal.rm`; owned private server only |
| Instance identity | Host: `status server` socket + version, socket inode. Private: own server PID + socket inode | `server.status` `core.id` + version, socket inode |

**Launch command difference.** TUI `agent start --kind` runs the canonical
executable name in the pane's interactive shell, so user aliases apply. On this
machine Codex runs as `codex --no-daemon -a on-request --sandbox danger-full-access`;
the permission and sandbox flags come from the user's shell, not from CrewMoor.
Core runs the configured executable directly, and CrewMoor adds `--no-daemon`.
The launch record keeps the actual process executable.

## Evidence

`tests/team.rs` covers markers on both runtimes, directory refusal, dispatcher-first
start, bind before and after SessionStart, process mismatch, rejection records,
host-mode stop without `session stop`, the unchanged private path, per-role
runtime refusal and the initial-prompt rules. `tests/message.rs` covers saved
bindings without the new fields and the optional `agent_session`. These are
synthetic. The real TUI runs (real-002 at `f2d339a`, plus the earlier real-001) are recorded in [Issue #15 real acceptance](issue-evidence-index.md#issue-15).

## Real smoke program

`tools/smoke` is a separate Rust crate, outside the root package, CI and
`verify`. It runs only when `CREWMOOR_SMOKE_APPROVED` equals the SHA-256 of the
approved `plan.json`. It refuses a crewmoor binary whose hash differs from the
plan. It runs the real acceptance sequence once: team start, an early bind before the
worker has a session (the worker starts without an initial prompt, so this bind
must be refused), the worker's first prompt, a bounded wait for its SessionStart
record, bind, one report-to and one dispatcher reply
(authored and sent by the Agents), compaction, `/clear` followed by one prompt and
one lead→worker send whose refusal must be the Hook halt, `feedback show`, and
`team stop`. Each external call's argv, exit code and output go to a fresh evidence
directory with `SHA256SUMS`. Nothing is retried. Team stop is always attempted
once if a team directory was created. Waits read only Hook journals and files
written by the Agents. They never read terminal output or container state.
The driver derives the team directory `<state>/teams/<team>` from the approved
config before start, decoding the team name with the same `toml` parser as the
real launch; a config without one decodable single-component name is refused
before start. It refuses to run if that directory already exists. When it
created the directory, the driver keeps the path even if `team start` fails part
way, so the single stop still runs. It never scans other teams.
