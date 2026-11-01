# One message round trip in one existing Herdr instance

Issue #9 adds a message entry for one fixed dispatcher/worker pair; Issue #11 adds
the existing herdr-core socket path. Both roles use the same local runtime socket.
Only Codex and Claude identities are accepted,
using the existing AgentKind and Issue #4 feedback contexts. This entry does not
launch, register with Herdr, rename, move or close an Agent, pane or server.

For TUI, the shortest official chain is `herdr agent get PANE` to locate the existing
Agent, then `herdr agent prompt PANE TEXT`. The recipient authors a reply and
calls the same `crewmoor message send` entry in the other direction. Herdr
returns `.result.agent` with `pane_id`, `terminal_id`, `workspace_id`, Agent kind
and `agent_session` (`kind: id`, `value: NATIVE_SESSION`). A successful prompt
returns `type: agent_prompted`. These are location and transport facts.

Checked against the [official stable LLM index](https://herdr.dev/llms.txt),
[v0.9.5 Agent automation](https://raw.githubusercontent.com/herdrdev/herdr/v0.9.5/docs/next/website/src/content/docs/agent-automation.mdx),
installed `herdr --skill`, `herdr agent prompt --help`, `herdr api schema --json`
and actual `agent get`/`agent prompt` returns. On this machine the CLI is 0.9.5
and the running server is 0.9.4; the server was retained. The schema describes the
CLI build, while the observed server returns establish the actual supported
identity and prompt fields. No newer schema-only field is required.

## Bind the existing pair once

Reuse each role's existing [feedback context](agent-feedback.md). When an
existing session has no feedback marker, `crewmoor feedback register REGISTRY
CONFIG` may create an explicit context with its **already verified**
`native_session_id`, team, role and Agent kind. This creates no Agent and does
not manufacture a SessionStart, hooks state or business report. No Hook
installation, trust change, session restart or environment edit is needed.

Put this configuration in a private local JSON file, with actual absolute paths,
feedback instance UUIDs and exact returned pane handles:

```json
{
  "runtime_binary": "/absolute/path/to/herdr",
  "socket": "/absolute/path/to/herdr.sock",
  "feedback_registry": "/absolute/feedback-registry",
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

## Evidence

`tests/message.rs` uses a Rust CLI transport fixture with fields checked against
the installed schema. It exercises fixed identity binding, both message
directions, literal text, request/reply correlation, changed identities/socket,
native rejection and uncertain submission, environment isolation and the public
CLI. These are synthetic transport tests; no real Agent is involved.

The separately authorized actual existing TUI session exchange and its exact
identities/results are recorded in [Issue #9 real acceptance](issue-9-real-acceptance.md).

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
native session association is operator-supplied; core only verifies terminal
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
[Issue #11 acceptance](issue-11-real-acceptance.md); current App version is
unknown and GUI observation is a separate untested path.
