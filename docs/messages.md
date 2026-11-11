# One message round trip in one existing Herdr instance

Issue #9 adds a message entry for one fixed dispatcher/worker pair; Issue #11 adds
the existing herdr-core socket path. Both roles use the same local runtime socket.
Codex, Claude and explicit OpenCode v2 identities are accepted,
using the existing AgentKind and Issue #4 feedback contexts. This entry does not
launch, register with Herdr, rename, move or close an Agent, pane or server.
OpenCode uses a [fixed native-session API transport](opencode.md), in both
runtimes: binding A while browsing B still delivers to A. It does not fall back
to the UI page or replace the binding. The following historical container prompt
and Hook descriptions apply to the Codex/Claude path unless stated otherwise.

For TUI, the shortest official chain is `herdr agent get PANE` to locate the existing
Agent, then `herdr agent prompt PANE TEXT`. The recipient authors a reply and
calls the same `crewmoor message send` entry in the other direction. Herdr
returns `.result.agent` with `pane_id`, `terminal_id`, `workspace_id`, Agent kind
and `agent_session` (`kind: id`, `value: NATIVE_SESSION`). A successful prompt
returns `type: agent_prompted`. These are location and transport facts.

Checked against the [official stable LLM index](https://herdr.dev/llms.txt),
[v0.9.5 Agent automation](https://raw.githubusercontent.com/herdrdev/herdr/v0.9.5/docs/next/website/src/content/docs/agent-automation.mdx),
installed `herdr --skill`, `herdr agent prompt --help`, `herdr api schema --json`
and actual `agent get`/`agent prompt` returns. That historical Issue #9 check used
CLI 0.9.5 and server 0.9.4, retaining that server. It is not a statement of the
current host version: team configuration must pin the actual installed host
(CLI/server 0.9.5 in the 2026-11-03 #40 batch). The schema describes the CLI build;
observed server responses establish supported identity and prompt fields.

## Team-owned pairs

For a team, use [team start and team bind](team-launch.md): the user works
directly in the returned dispatcher pane, and each worker has one fixed pair
binding. Reuse that binding for requests and replies; do not bind it again.
Do not register or adopt an unrelated existing session to supply an outer
dispatcher. The native prompt/send operations below describe the implementation,
not an additional business channel for team members.

The lower-level `message bind` API remains available for already owned, recorded
pairs with [feedback contexts](agent-feedback.md). It does not create or adopt
Agents. For that API, use actual returned identities and verified task ownership:

Put this configuration in a private local JSON file, with actual absolute paths,
feedback instance UUIDs and exact returned pane handles:

```json
{
  "runtime_binary": "/absolute/path/to/herdr",
  "socket": "/absolute/path/to/herdr.sock",
  "feedback_registry": "/absolute/feedback-registry",
  "team_config": "/absolute/state/teams/sample/team.json",
  "dispatcher": {"instance": "DISPATCHER_FEEDBACK_UUID", "handle": "w1:p1"},
  "worker": {"instance": "WORKER_FEEDBACK_UUID", "handle": "w1:p2"}
}
```

```text
crewmoor message bind /absolute/config.json /absolute/new-binding.json
```

Binding loads the existing contexts, requires one team and distinct roles and
panes, and checks each actual Agent kind and native session. It captures the
canonical socket path/device/inode and the returned pane/terminal/workspace
identity, following Issue #2's explicit container/handle rule. The private binding
is created once and never overwrites an existing file. This is a fixed pair,
without dynamic role discovery or a registration framework.

### Identity after Issue #15

Bind now reads the session registered by the Agent's own SessionStart Hook
(`native-session.json`), or the explicit registration session; an unregistered
role fails immediately. For TUI each peer must supply `agent_executable`; bind
locates the pane's single Agent-kind foreground process via `pane process-info`
and stores its kernel identity, re-checked before and after every send. Once a
role's Hook has recorded a rejected (changed) session, bind and send for that
role are refused and the binding is kept unchanged. Herdr's
`agent_session` is an additional check: when present it must equal the
registered session; when absent, the bound process identity is required. Saved
bindings without `native_session`/`process` remain readable. Core has no
container session field and keeps its existing terminal/process checks.
For OpenCode, the native API-created main session and private endpoint/process
identity are additionally pinned. Herdr's reported root/subtree is retained as a
source fact, not a current-page condition. A send returns the native prompt
`id/sessionID/delivery` separately from a core terminal turn; native admission
still requires an authored CrewMoor response to establish actual receipt.

## Send and actively reply

The sender/skill creates a short JSON message. IDs are caller-chosen nonempty
identifiers of up to 256 bytes; `reply_to` names the original request. Content
must be nonblank and at most 16384 UTF-8 bytes.

```json
{
  "id": "issue-9-request-001",
  "from": "dispatcher",
  "to": "worker",
  "reply_to": null,
  "content": "Read this request and actively reply through the same message entry."
}
```

```text
crewmoor message send /absolute/binding.json /absolute/request.json
```

The delivered prompt is JSON containing `team` and `message`. To answer, the
recipient creates its own message with `from: worker`, `to: dispatcher`, a new
`id`, `reply_to: issue-9-request-001` and its authored content, then invokes the
same send command with that reply file. The Agent or skill knows the shared
binding/binary paths from the task instructions. CrewMoor never generates the
business answer. Callers may also use the public Rust `message::send` function.

Ordinary messages keep this exact compact envelope and String content, including
JSON-looking text. Only explicit Hook/report-to forwarding adds a versioned
[`feedback_view`](feedback-view.md) alongside `message`, with the complete fact
as a readable object and the legacy content preserved. That feedback envelope
is expanded except for explicit native Stop Hook notices, which use the ordinary
compact single-line format while retaining the full view/content. Consumers rejecting unknown envelope fields need the
documented additive-field migration. No content-based type detection occurs.

Before prompting, the entry checks the same socket instance and both fixed
identities. It uses literal argv, with inherited Agent/container routing markers
removed and only the bound Herdr socket selected; authentication, tool settings
and proxy configuration stay with the user. It calls `agent prompt` once,
without a lifecycle wait or automatic resend. A receipt's `transport: accepted`
means the native prompt was accepted; it does not prove receipt or completion.
The returned identity is checked again. An error or changed post-submit identity
is unconfirmed and may already have delivered input. Inspect the original
session before any deliberate retry. There is no deduplication guarantee.

This is local trusted role attribution, like the existing explicit feedback
entry; it does not authenticate which human/process authored a message. The
binding and JSON files are trusted local inputs. Herdr does not offer an
expected-session condition on prompt submission: checks narrow the identity
change window but are not atomic with delivery. No queue, recovery protocol,
cross-container routing, state forwarding (#6) or requirement intake (#8) is
introduced. Neither terminal output nor Herdr detection state is interpreted
as hooks state. Business progress and hooks retain their existing feedback
entry and semantics.

## Recipient constraints in the same submission

Configure `message_constraint` on any team role independently of `initial_prompt`:

```toml
[[roles]]
role = "builder"
# Other required role fields as usual.
message_constraint = """Use the task's agreed verification scope.
Report the checks you actually ran."""
```

`team generate` preserves the user text; `team start` saves it in the running
`TEAM_DIR/team.json`, without passing it to startup argv. That running JSON file
is the only effective source for messages. Editing the original TOML template
does not update an existing team.

The caller still supplies only the original business Message:
`id`, `from`, `to`, `reply_to`, `content`. The public Rust entry, CLI and existing
transports build one Envelope, reading the current configuration once before
native submission and selecting the **bound receiving role**:

```json
{
  "team": "sample",
  "recipient_constraints": "User-configured builder text",
  "message": {"id":"task-1","from":"lead","to":"builder","reply_to":null,"content":"Business original"}
}
```

Both lead → builder and builder → lead use the receiver's own text. There are no
hardcoded role names or new routes. Nonblank text is kept verbatim, including
Chinese, newlines, quotes, backslashes and JSON-looking strings. Missing, empty
or all-whitespace values always produce `"recipient_constraints": ""`. No default
rules are generated. The caller cannot set this field in Message. Strict
Envelope consumers must accept the new String field alongside `team`, `message`
and optional `feedback_view`; do not reinterpret ordinary content as JSON.

The business Message stays unchanged and is submitted **once**, together with
its constraints. The independent B generation, second send, mode switches and
partial-success receipt have been removed. The receipt contains the single
`envelope`, `transport` and applicable native receipt. `accepted` is native
admission, not proof of reading, execution or compliance. Failed/unconfirmed
submissions keep the existing semantics and are never automatically retried.
This field is ordinary prompt data, not a system message or a Hook rule.

Startup `initial_prompt` remains a once-only native launch argument. It is never
replayed by bind, update or send. First and later messages use the same rules.
Hook/report feedback and halt notifications preserve their `feedback_view`,
compact Stop presentation, routing and session checks while receiving the same
recipient field from the common envelope entry.

### Existing binding source

New `team bind` bindings record `config.team_config` as the explicit absolute
`TEAM_DIR/team.json`. Direct `message bind` callers supply that field in their
configuration. A pre-#71 binding remains readable, but cannot send until its
source is explicitly supplied:

```text
crewmoor message source /absolute/existing-binding.json /absolute/TEAM_DIR
```

This one-time operation validates the source against the fixed pair and atomically
adds only the source to the existing binding, preserving its sessions, native
identities, routing and any legacy `worker_constraint` text. That old snapshot is
retained as data only and never determines a new envelope. No Agent restart or
native rebind is needed. A missing source produces instructions for this command;
missing/unreadable/malformed configuration, mismatched team or absent/ambiguous
bound roles fail before submission, with no fallback to an empty constraint or
snapshot and no background recovery.

### Save corrected text safely

Place the authorized text in a UTF-8 file (an empty file clears the constraint),
then use the minimal update command:

```text
crewmoor team constraint /absolute/TEAM_DIR builder /absolute/new-text.txt
```

The equivalent library call is `team::update_constraint(team_dir, role, text)`.
It parses the running document against the team schema, checks the role and
constraint length, changes only that role's field, stages a private file in the
same directory and atomically renames it over `team.json`, reusing the existing
atomic replacement helper. Unrelated fields are retained. Do not truncate-write
the live document. This changes only message text, with no changes to sessions,
launch arguments, routing or Hook rules. Coordinate multiple writers: concurrent
saves are last-writer-wins and do not merge edits.

A concurrent send sees a complete old or new configuration; a send starting its
read after a successful save sees the new value. Previously sent messages and
active work are not modified, and no corrective message is sent automatically.
There is no watcher, cache or rebind.

Content and each nonblank constraint retain their separate 16384 UTF-8 byte
limits. The final serialized TUI argument is also checked against Linux's
`MAX_ARG_STRLEN` (`PAGE_SIZE * 32`, including NUL): JSON escaping can expand two
otherwise legal fields beyond this limit. Oversize text fails before native
submission, with no truncation, splitting or discarded constraints. Core uses
its existing socket JSON transport and OpenCode its fixed session API; neither
passes this text as a CLI argument. Team configuration and receipts retain the
text, so keep secrets out of it.

`tests/message.rs` covers both directions, custom roles, serialization, per-send
reads, source migration/errors, atomic saves, length boundaries and single-send
outcomes. `tests/team.rs` covers new source binding and startup isolation;
`tests/core_message.rs` and `tests/opencode_flow.rs` cover their existing protocol
paths. These are synthetic tests; real native admission, actual Agent receipt,
constraint compliance and Hook enforcement remain separate evidence.

## Evidence

`tests/message.rs` uses a Rust CLI transport fixture with fields checked against
the installed schema. It exercises fixed identity binding, both message
directions, literal text, request/reply correlation, changed identities/socket,
native rejection and uncertain submission, environment isolation and the public
CLI. These are synthetic transport tests; no real Agent is involved.

The separately authorized actual existing TUI session exchange and its exact
identities/results are recorded in [Issue #9 real acceptance](issue-evidence-index.md#issue-9).

## Existing herdr-core / App connection

App is a client of core. Issue #11 adds core message implementation, not App UI
automation. Use the same `message bind` and `message send` entries with an
explicit core configuration. Omitted `runtime` continues to mean `tui`, so
existing TUI configurations and saved bindings remain readable.

```json
{
  "runtime": "core",
  "core_id": "ACTUAL_CORE_ID",
  "core_version": "0.1.15",
  "runtime_binary": "/absolute/path/to/hc",
  "socket": "/absolute/existing/core.sock",
  "feedback_registry": "/absolute/feedback-registry",
  "dispatcher": {
    "instance": "DISPATCHER_FEEDBACK_UUID", "handle": "t1",
    "agent_executable": "/absolute/path/to/claude"
  },
  "worker": {
    "instance": "WORKER_FEEDBACK_UUID", "handle": "t2",
    "agent_executable": "/absolute/path/to/claude"
  }
}
```

The shortest native equivalent is `hc get ADDRESS`, `hc send ADDRESS TEXT`,
then `hc get ADDRESS` again. The implementation directly reuses the existing
`app::exchange` helper on the **core** socket, never an App window/control socket.
It uses native `server.status`, `terminal.get` and `terminal.send`. No runtime
CLI is executed in this branch: an unavailable socket fails without CLI
`connect_or_start`, service creation or fallback. `runtime_binary` is retained
as configuration provenance, while `server.status.version` establishes the
running core's actual version.

The installed `hc 0.1.15 --skill`, command help and `hc schema` were checked
after reading the [App's official Agent instructions](https://raw.githubusercontent.com/herdrdev/herdr-app/master/defaults/AGENTS.md),
[core entry](https://herdr.dev/core/) and
[core command reference](https://github.com/herdrdev/herdr-core/blob/master/docs/commands.md).
Version-specific source
[`4a3591aa` client.rs:230](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/client.rs#L230)
and
[server.rs:129](https://github.com/herdrdev/herdr-core/blob/4a3591aa835c197f57a5d1b8e8951a75e2a79faa/crates/herdr-core/src/server.rs#L129)
confirm one newline JSON-RPC 2.0 request/reply per socket connection, with the
request id echoed. Connections start in plain mode; only `hello` with
`framed: true` upgrades to frames. An independent read-only probe confirmed
this helper against the actual default core's `server.status` and
`terminal.get`. Reusing a helper named `app` does not select an App socket.

Bind requires the configured core id/version, canonical socket device/inode,
two exact local native addresses and explicit feedback contexts. It records
each terminal's `created_ms`, launch `argv`, canonical cwd and the kernel
PID/start-time/executable identity using Issue #2's process helpers. The supplied
Agent executable and context's cwd must match the actual process and terminal
launch. Cross-machine addresses and aliases are rejected. Before each send,
both fixed roles, core id/version and socket are checked again; successful
submission is followed by another check. No ownership of existing resources is
granted by bind, and the message entry never stops them.

Core has **no native Agent session field**. `Terminal.identity` describes the
terminal program/version, not an Agent session. The feedback context's explicit
native session association for Codex/Claude is operator-supplied; core only verifies terminal
and kernel process facts. It cannot independently prove whether the Agent
switched conversations inside that process. A new Claude test can use its
native `--session-id` and actual Issue #4 SessionStart to establish that
association without inventing fields, hooks or semantic state.

`terminal.send` receives the unchanged message envelope as `text`, with
`force: false` and `wait: false`. `if` and `caller` are omitted: no lifecycle
gate is added and native blocked-dialog protection is retained. Unlike the
CLI, this direct call does not derive a caller from HC_TERMINAL_ID or a parent;
there is no send/turn wait to associate. `SendResult.turn` is preserved as
`native_turn` in the receipt. The optional `status`, `started` and `wait` fields
are not used to infer receipt, readiness or business completion. After any
post-submit error (including malformed response, timeout, EOF or identity
change), delivery remains unconfirmed and no resend occurs. Core errors retain
their native kind and message for diagnosis.

`tests/core_message.rs` uses a real local socket with a synthetic Rust JSON-RPC
server, plus actual owned fixture processes for the kernel identity checks.
It covers both directions, exact framing/parameters, rejected identities and
executable/session associations, unavailable endpoints, native rejection,
uncertain delivery and the public CLI. These are not real Agent exchanges.
The separately coordinated real core exchange is recorded in
[Issue #11 acceptance](issue-evidence-index.md#issue-11); current App version is
unknown and GUI observation is a separate untested path.
