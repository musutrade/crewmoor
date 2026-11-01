# Issue #6, first step: forward one authored report

This step explicitly forwards an Agent/skill-authored business report from the
fixed worker to the fixed dispatcher. Issue #6 remains **OPEN**. Native Hook
event forwarding and the second step (dispatcher follow-up handling) are not
implemented here; #8 is outside scope. PRs for this step must not auto-close #6.

Use an existing [message binding](messages.md) and its two
[feedback contexts](agent-feedback.md). The worker and dispatcher stay fixed
in one team and one runtime instance. No new registration mechanism, role discovery or message
protocol is introduced. The same message path supports its existing TUI/core
routes; the separately coordinated live coverage for this step is stated in
[the acceptance record](issue-6-step1-acceptance.md).

```text
crewmoor feedback report-to BINDING SESSION agent|skill TYPE SUMMARY
```

For example, an actual worker intentionally reports a fact it can substantiate:

```text
crewmoor feedback report-to /absolute/binding.json WORKER_NATIVE_SESSION agent progress "Implementation and local checks completed; receipt confirmation pending"
```

The binding supplies the worker's feedback registry/instance and role, plus
the fixed dispatcher role and native recipient identity. The caller does not
choose arbitrary sender/recipient roles. The current stored worker context
must equal the bound context; the existing #4 checks validate report source,
type, bounded summary and the native session association. Local role/session
claims retain the existing trusted-input model, not caller authentication.

The existing report path constructs and appends one `Event` to its journal.
Only after append succeeds does this same call serialize **that exact returned
record**, including `received_ms`, `source`, `event`, `native_session_id`,
cropped `native` metadata and intentional `summary`. It does not reread a
`latest` event, select an unrelated report or wait for another event.

The unchanged Message envelope has a generated message id, `from` equal to
the bound worker role, `to` equal to the dispatcher and `reply_to: null`.
Its content is JSON containing `feedback_instance`, `task`, `agent`, `evidence`
and the exact `record`. `message::send` performs the existing identity/instance
checks and invokes the native transport once. No person or test driver copies
the report into another message. Original native Hook payloads, transcripts,
unrelated history and context cwd are not added to that content; #4's existing
privacy filtering and intentionally supplied business summary remain intact.

On success, stdout separates `recorded: true` from the derived `delivery`
receipt. Delivery `transport: accepted` only means native transmission was
accepted. Actual receipt requires the dispatcher to confirm the received
message id, worker instance/session, event time/type/source and summary against
the recorded fact. That confirmation ends this first-step acceptance; no
dispatcher decision or next task is automatically requested.

If the context or #4 report validation fails, no report/message is produced.
Once a record is appended, a forwarding error explicitly says that feedback
was recorded but forwarding is unconfirmed, and **not to repeat report-to or
replay that report**. The original record remains available for inspection;
there is no automatic resend, replay command, pending queue or delivery cursor.
Even an uncertain send might already have delivered text. Running report-to
again would create another report, so callers must not retry the operation to
recover an unknown result.

The ordinary `feedback report` and user-level Hook callbacks remain local and
do not forward implicitly. No callback binary, global Hook definition or trust
configuration is changed by this feature. It adds no watcher, daemon, broker,
recovery mechanism or new semantic state. `agent_state` stays unknown; an
authored `complete` remains the author's assertion, and native Stop still means
only a response turn ended. No terminal output, container state or process
liveness is used as Agent state.

The native TUI chain remains `herdr agent get` then `herdr agent prompt`, using
the existing explicit socket and fixed session identities. Verified against
the [official v0.9.5 automation reference](https://raw.githubusercontent.com/herdrdev/herdr/v0.9.5/docs/next/website/src/content/docs/agent-automation.mdx),
installed CLI `0.9.5` help/schema and actual identity returns; the current server
is `0.9.4` and is retained. Core uses the already implemented #11 native path;
this step does not claim new actual core or Hook-forwarding coverage.

`tests/feedback_forward.rs` exercises exact new-record forwarding, unrelated
history exclusion, fixed-source/session validation, recorded-but-unconfirmed
delivery without retry, and the public CLI. These fixtures are synthetic and
do not replace the actual Agent-authored report and dispatcher receipt.
