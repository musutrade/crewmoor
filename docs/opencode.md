# OpenCode v2 in Herdr TUI and App/core

OpenCode uses the same team roles, fixed bindings and `crewmoor message send`
entry as Codex and Claude. Its internal transport is different: one native
`POST /api/session/{bound-sessionID}/prompt`, with the complete CrewMoor envelope
as `text`. It never parses ordinary String content or sends the message to the
currently displayed terminal page.

**Binding A while browsing B still delivers to A.** This explicitly replaces the
earlier proposed OpenCode UI-switch refusal. Browsing B is not session rejection,
halt or adoption. A changed/missing endpoint, server instance, native session or
launch association is refused; the original binding is retained. An uncertain
submission is not automatically repeated. Codex/Claude keep their existing paths.

## Configuration

Select `agent = "opencode"` explicitly in a role or launch config. Generation
discovers the installed executable/version, while task choices stay in the input.
The native Linux npm entry resolves to `opencode.exe`; retain that canonical
executable for process identity. OpenCode v1 is not supported.

```toml
[[roles]]
role = "dev"
cwd = "/absolute/project"
agent = "opencode"
agent_executable = "/absolute/opencode.exe"
agent_version = "opencode v2.2.5"
initial_prompt = "Read the supplied team instructions and wait for assignment."

[roles.opencode]
agent = "ACTUAL_NATIVE_AGENT_PROFILE_ID"

[roles.opencode.model]
providerID = "ACTUAL_PROVIDER_ID"
id = "ACTUAL_MODEL_ID"
variant = "ACTUAL_SUPPORTED_VARIANT"
```

Profile/model/variant are optional explicit native session.create fields, not
role names or root CLI flags. Use actual IDs from the owned endpoint's native
catalog; unknown choices are not silently replaced. Omit variant to use the
native default. Root OpenCode has no `--model`; that argument is rejected.
`initial_prompt` uses native `--prompt` once; the existing single-line/4096-byte
team startup limits apply. Business messages thereafter use CrewMoor only.

`team start` registers each instance and starts it through the existing container
entry. The actual session.create return establishes that instance's immutable
main session. This is an API fact (`source: native-api`, `event: session.create`),
not a fabricated SessionStart. Bind only after the real association exists;
report a missing association once, without polling, restarting or rebinding.
Standalone `launch` configs supply `feedback_registry` and a pre-registered
`feedback_instance`; team start fills these itself.

## Private native endpoint and ownership

A foreground Rust launch holds the official stdin ownership lease of
`opencode.exe serve --stdio --port 0 --hostname 127.0.0.1`. It reads the native
readiness URL once, authenticates with a temporary child-only password, checks
server.info PID/version, creates and GETs the actual main session, and starts
`opencode --server URL --session ID` with the same private authentication.
The original config/HOME/tool/proxy paths are restored in these children;
the task has its own SQLite DB. Provider authentication must be validated for
that actual setup; an idle zero-model probe alone does not establish it.

`--standalone` hides its random credential inside the TUI, so it cannot supply
this external API transport. It is not combined with `--server`. CrewMoor does
not start/restart/discover the user's default shared OpenCode service.

TUI still uses official `agent start --kind opencode`. A workspace-private PATH
entry runs this foreground launch, then the recorded canonical native executable.
After shell initialization, one native `pane run` sets this entry first in that
owned shell's PATH before agent start; user rc files can otherwise prepend the
installed executable ahead of the workspace environment. This startup setting
requires a known POSIX shell (bash, dash/sh, zsh or ksh); other shells are refused.
Only that new workspace's PATH changes; native children retain the initialized
shell's tool PATH with just this launch's private entry removed.
The outer official start receives --standalone and the owned DB environment. The
Rust entry consumes that selector; its actual interactive child receives --server
and the configured --prompt, with no --standalone. Thus an unselected private
entry cannot start the user's shared service or run its startup prompt.
App/core uses the same launch through its existing terminal.run/hc run path,
keeping placement, native address/generation and environment isolation.

The endpoint/password live in the instance's private 0600 file under its 0700
registry directory. Public bindings/receipts contain identity facts and no
password. Launcher, real TUI, native server and container handles are recorded
separately. A TUI exit closes the native lease; a container-ending launcher also
closes it through OS descriptor ownership. Stop verifies the captured native
process exits. A terminal exit alone is not proof that backend work ended.

## Native facts and receipt semantics

For TUI, reuse the **Herdr official OpenCode integration**, currently v13. It is
not an OpenCode-produced plugin. Its pane-local `agent_session` reports a root or
owned subtree, not necessarily the selected child. It can retain that identity
on the home page. Preserve the real report/source; never call it a strict
current-page getter or use it to replace the API-bound session.

For core, OpenCode emits native OSC 7501. terminal.get/events.subscribe/native
wait remain the source of terminal status. Root aggregation may cover several
tabs; child record IDs can be hashes, and clear means removal, not completion.
Facts that cannot be associated with the bound session stay unknown. No fake
Herdr environment or screen/process-based Agent-state detector is introduced.

The session API's native SSE preserves execution/retry/permission/form event names
and raw data for the exact associated main session, using the existing journal
and explicit feedback_view forwarding. Foreign session events are not assigned
to it. The stream is volatile: missing events during disconnection are not
replayed, unlike core's missed+snapshot contract. A gap is recorded as unknown;
there is no recovery framework, synthesized Stop or automatic Issue progression.

Native prompt receipt `id`, `sessionID`, and `delivery` are checked and returned
as `native_prompt`; this is not a core terminal.send turn. A 200 response is
durable native admission. Actual Agent receipt requires an authored response
through CrewMoor; neither admission, execution success nor idle proves business
completion. Ordinary compact envelopes and explicit versioned feedback_view
retain the [#54 reading contract](feedback-view.md).

Protocol basis: [OpenCode 2.2.5 session API](https://github.com/anomalyco/opencode/blob/9bbb8ce18c80f099e0e448775f59d965ead40a21/packages/protocol/src/groups/session.ts),
[server connection/credential selection](https://github.com/anomalyco/opencode/blob/9bbb8ce18c80f099e0e448775f59d965ead40a21/packages/cli/src/services/server-connection.ts),
[native stdio lease](https://github.com/anomalyco/opencode/blob/9bbb8ce18c80f099e0e448775f59d965ead40a21/packages/cli/src/services/standalone.ts),
[Herdr official integration](https://github.com/herdrdev/herdr/blob/ee5bfb5041bb3e4a49d591c72308aae3c81ecdde/src/integration/assets/opencode/herdr-tui-session.js).
Check actual installed help/schema/version against these sources before changing
integration behavior. Per-run real acceptance and remaining gaps belong in the
batch evidence index, separately from Rust regression tests.
