# Readable forwarded feedback

Hook forwarding and `feedback report-to` deliver a JSON envelope with an explicit
`feedback_view`. Native Stop Hook notifications use compact single-line JSON,
matching ordinary message formatting while retaining the complete view and String
content. Authored report-to, SessionEnd and halt forwarding keep their expanded
format. Only the explicit native Stop producer selects the compact form; ordinary
content that mentions Stop is never inspected. Read `feedback_view.fact` for the
complete feedback, and `message` for its id, roles and reply correlation:

```json
{
  "team": "example-team",
  "feedback_view": {
    "version": 1,
    "fact": {
      "feedback_instance": "worker-instance",
      "task": "example-task",
      "agent": "codex",
      "evidence": "native",
      "record": {
        "received_ms": 1793836800000,
        "source": "native-hook",
        "event": "Stop",
        "native_session_id": "worker-session",
        "native": { "stop_hook_active": false },
        "summary": null
      },
      "system_halt_notification": false
    }
  },
  "message": {
    "id": "example-notice",
    "from": "worker",
    "to": "lead",
    "reply_to": null,
    "content": "{\"feedback_instance\":\"worker-instance\",\"task\":\"example-task\",\"agent\":\"codex\",\"evidence\":\"native\",\"record\":{\"received_ms\":1793836800000,\"source\":\"native-hook\",\"event\":\"Stop\",\"native_session_id\":\"worker-session\",\"native\":{\"stop_hook_active\":false},\"summary\":null},\"system_halt_notification\":false}"
  }
}
```

This is an expanded shape example for documentation, not literal Stop wire or a
real receipt. Actual Stop wire has no formatting newlines. The fact and the legacy
content are serialized from the same exact record returned by this append, not
from the latest journal entry. The view preserves the full existing fact:
instance, task, Agent, evidence and all recorded event fields. It does not expose
the original unfiltered Hook input. Hook facts retain
`system_halt_notification`; authored reports retain their original shape without
that field. `Stop` means a response turn ended, not business completion. This
view adds no dispatch, completion decision or recovery behavior.

## Version and compatibility

`feedback_view.version = 1` identifies this additive presentation extension.
`feedback_view.fact` is an object equal to the JSON value in the legacy
`message.content`. The old String content, message fields and team association
remain available with their existing semantics. Keeping both copies costs extra
prompt bytes and leaves the escaped legacy content visible below the readable
object; readers can use the first object without unpacking that string.

Consumers selecting `team` and `message` can keep using the existing content.
Consumers that reject unknown envelope fields must add the optional
`feedback_view` field before consuming these updated feedback envelopes; such
strict consumers are not automatically compatible. A migrating reader should
use the view only when its version is understood, and otherwise retain the
legacy message path. There is no new legacy-output switch. Saved old envelopes
without a view remain valid. The `message send` input schema is unchanged and
still accepts only its existing flat fields, including String content.

Only the feedback producer requests a view. Ordinary `message send` and the
public Rust `message::send` do not parse or inspect content for JSON, even when
it resembles a feedback fact, a view, malformed JSON or plain text. They keep
their compact envelope with no `feedback_view`. The copied fact is data: a
JSON-looking authored summary stays a summary String.

The same serialized feedback envelope is sent through the existing TUI
`agent prompt` or core `terminal.send`; routing, session checks, halt routing,
native parameters and single-attempt submission semantics are unchanged.
Receipts include that same view, but `transport: accepted` still proves only
native acceptance. Actual dispatcher receipt and readability require a real
recipient's confirmation. Serialization/fixture tests cannot prove that.

## Choice of layer

Pretty-printing only the previous envelope leaves its nested JSON escaped.
Changing every content to an object would change ordinary text semantics, while
guessing a feedback type from JSON-looking content would misclassify text.
The explicit feedback presentation preserves the legacy fact and ordinary
message schema, with the additive-field migration and duplicate-byte tradeoff
described above. It requires no container changes or recipient-side decoder.

`tests/feedback_forward.rs` checks complete-view equality, literal Chinese,
quotes, backslashes, newlines, JSON-looking and invalid/nonstructured summaries.
`tests/feedback_hook_forward.rs` checks the same view equality on native Hook
and halt paths. `tests/message.rs` verifies ordinary content is never decoded;
`tests/core_message.rs` checks authored and Hook views with the existing native
core send parameters. These are synthetic tests. Actual receipt evidence is
kept with the task's batch records, outside this documentation.
