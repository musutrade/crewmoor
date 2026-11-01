# One message round trip in one existing Herdr instance

Issue #9 adds a message entry for one fixed dispatcher/worker pair. Both roles use
the same local Herdr TUI socket. Only Codex and Claude identities are accepted,
using the existing AgentKind and Issue #4 feedback contexts. This entry does not
launch, register with Herdr, rename, move or close an Agent, pane or server.

The shortest official chain is `herdr agent get PANE` to locate the existing
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

The separately authorized actual existing-session exchange and its exact
identities/results are recorded in [Issue #9 real acceptance](issue-9-real-acceptance.md).
