# User-level Agent feedback

Issue #4 provides a local Rust entry for Codex/Claude native Hooks and separately authored Agent/skill reports. It does not inspect terminals, transcripts, logs, process liveness or container status. `feedback show` always reports `agent_state: unknown`; it exposes the actual latest native event and latest business report separately. `Stop`, `SessionEnd`, tool failure and a model turn ending never manufacture business completion.

## Verified native capabilities

Checked on 2026-11-01: Codex `0.166.0`, Claude Code `2.1.316`, Herdr `0.9.5`, Orca `1.4.240`. These are observations, not an old-version compatibility matrix. Latest stable Rust/edition 2024 and the existing full quality gates apply.

Read the [official Codex Hooks guide](https://learn.chatgpt.com/docs/hooks) and [Claude Hooks reference](https://code.claude.com/docs/en/hooks) before configuring. Local Codex `app-server generate-json-schema --experimental` confirms native event metadata and user/session/project sources; this generation does not launch an Agent. Claude `--help` and its installed event enum were checked against the official reference. Online docs may lead installed releases.

| Agent | Events collected by this implementation | Meaning/limits |
|---|---|---|
| Claude | SessionStart, UserPromptSubmit, Stop, SessionEnd | Native session/prompt/response/termination facts, not ready/working/task done |
| Claude | StopFailure, Notification, PermissionRequest, PostToolUseFailure | Native error/notification/permission/tool-failure facts; no promise that all quota/network failures emit them |
| Codex | SessionStart, UserPromptSubmit, Stop, SessionEnd, Interrupt, PermissionRequest | Native facts only; no Claude-style StopFailure/Notification event is assumed |

Codex's local schema also lists pre/post-model, pre/post-tool, compaction and subagent events; they are not collected here. Claude has additional events, including TaskCompleted for its own task machinery. Neither native TaskCompleted nor SubagentStop is interpreted as CrewMoor business completion. Subagents do not enroll a new main instance. A repeated SessionStart for the same session may be recorded with its source; it does not create another instance or mean ready. A changed session is rejected rather than silently adopted.

All listed events have automated synthetic tests or a supported schema path; **actual Agent-triggered events and model-authored business reports remain unverified until separately authorized and run**. The first synthetic chain used Claude, then Codex was added. No new model call is authorized by installation.

## Native user-level installation

The only install scope is user-level: `$CODEX_HOME/hooks.json` (default `~/.codex/hooks.json`) and `$CLAUDE_CONFIG_DIR/settings.json` (default `~/.claude/settings.json`). There is no session/project Hook-config loading mode.

The installed Herdr scripts identify Claude integration v11/Codex v10 and explicitly tell users to add independent hooks alongside them. Their reporting is gated by Herdr identity/environment. Existing user files contain both Herdr and Orca entries. Orca 1.4.240's installed scripts are `~/.orca/agent-hooks/{claude,codex}-hook.sh`; they gate on Orca pane/endpoint metadata and exit without it. Their HTTP/spool transport is not copied. CrewMoor follows only the coexistence/owned-handler pattern and uses a Rust file record directly.

Build into a dedicated target and put the binary at a durable path before installing. Each plan prints only the exact CrewMoor handler changes and target path; it does not print other settings. Its private `before.json` is an exact backup, `after.json` is the proposed full file, and `plan.json` contains the reviewable owned changes. These files can contain private existing configuration and must not be published.

```text
crewmoor feedback hooks plan claude install /absolute/registry /absolute/durable/crewmoor /absolute/new-plan-dir
crewmoor feedback hooks apply /absolute/new-plan-dir
```

Use `codex` for the second native file. Installation appends one marked handler for each collected native event. Existing Herdr/Orca handlers, shared matchers, unrelated settings and permissions are preserved. Reinstall/update uses the same commands with a fresh private plan directory; identical installation does not duplicate or rewrite the native file. Uninstall plans use `uninstall` and remove only the reserved exact command prefix `: crewmoor-feedback-v1; exec ` together with the CrewMoor status label; they leave unrelated handlers and shared group metadata intact.

```text
crewmoor feedback hooks plan claude uninstall /absolute/registry /absolute/durable/crewmoor /absolute/new-remove-plan
crewmoor feedback hooks apply /absolute/new-remove-plan
crewmoor feedback hooks rollback /absolute/new-plan-dir
```

Apply refuses a changed native file or a modified proposal; rollback refuses later edits. Writes use a private temporary file, atomic rename and preserved permissions. This is a short config edit, not a config-management daemon; avoid concurrent external config edits during apply. No trust-state, `.env`, proxy, shell or service configuration is changed.

Codex's `features.hooks` is enabled on this machine. New/changed definitions require native `/hooks` review and trust before they run. CrewMoor does not write `[hooks.state]`, reuse an old trusted hash or pass `--dangerously-bypass-hook-trust`. If managed policy blocks user hooks, status stays unknown. Any native review/reload or user action must be completed explicitly; no existing service is restarted automatically.

## Task and instance ownership

Global Hook installation is not permission to collect every session. Register a task using an example JSON with real team/role/task, Agent version, canonical cwd and evidence class:

```text
crewmoor feedback register /absolute/registry /absolute/registration.json
```

This creates one private UUID instance and returns its context. Set only the routing metadata `CREWMOOR_FEEDBACK_INSTANCE=<returned-instance>` when starting that task's Agent through its native environment capability. This key selects a previously registered task, not another Hook config. Agent/model startup still requires its own authorization. Issue #2 launch receipts and current containers are not modified by registration.

Without that marker, the global callback returns `{}` without reading stdin or opening session records. An invalid/unregistered marker or wrong Agent kind is rejected before stdin is read. For a registered marker, the payload's canonical cwd and native session must match. If the context has no pre-known session, only its first main SessionStart can bind the native ID; no prompt/Stop event guesses it. A pre-known session can also carry an explicit Agent/skill report even if no Hook arrived. This is local trusted task routing, not cryptographic attestation of the caller.

The callback reads native JSON on stdin, keeps only event name/native session and bounded source/reason/turn/tool/error-kind metadata, and appends a short JSON record with receipt time. It drops prompt, last assistant message, transcript paths, tool inputs, background task contents and all other fields. Failed/unassociated feedback returns neutral `{}` with no decision, replay or completion output. Failures may print a generic diagnostic; work is not replayed. A busy local record lock fails this one callback rather than starting a queue or recovery loop.

Error handling is event-specific: `PostToolUseFailure.error` is free-form tool output and is never recorded; only `error_present` is kept. For `StopFailure`, only the documented kinds `rate_limit`, `overloaded`, `authentication_failed`, `oauth_org_not_allowed`, `account_on_hold`, `billing_error`, `invalid_request`, `model_not_found`, `server_error`, `max_output_tokens`, `cloud_credential_error`, `unknown` are stored. Missing/unrecognized kinds become the fixed value `unknown`; error details and rendered error text are discarded. An `error` field on any other event is ignored.

## Explicit business reports and viewing

Agents and skills report only facts they can substantiate; an explicit `complete` report is their assertion, not a conclusion inferred by CrewMoor. Summaries are supplied intentionally and must not contain credentials or full conversations.

```text
crewmoor feedback report /absolute/registry INSTANCE NATIVE_SESSION agent progress "Implementation complete; validation pending"
crewmoor feedback report /absolute/registry INSTANCE NATIVE_SESSION skill complete "Requested change validated and handed off"
crewmoor feedback show /absolute/registry INSTANCE
```

Allowed report types are `ready`, `progress`, `complete`, `blocked`; sources are `agent` or `skill`. Unknown/mismatched session, unsupported native event or missing evidence does not become successful business completion. Registration distinguishes operator-declared `synthetic` and `native` evidence; a declaration alone does not prove a real model ran. Audit/acceptance must identify the actual native invocation and authored report separately. No queue, continuous monitoring, workflow state machine, container-specific semantic state or automatic task retry is involved.
