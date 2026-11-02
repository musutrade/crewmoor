# Issue #6, step 3: Hook Stop forwarding

This step automatically forwards a bound worker's `Stop`, `SessionEnd` and session-rejected
events to the fixed dispatcher via the existing message channel. Issue #6 remains **OPEN**
until the dispatcher's handling of forwarded halt events is covered by a live acceptance.

---

## How forwarding works

After the existing local ingest, the callback checks whether the triggering instance is a
bound worker (a `binding-ref.json` written by `team bind` is present). Only bound workers
are forwarded; the dispatcher's own events are never forwarded.

| Event | Local record | Forward? | Path |
|---|---|---|---|
| `Stop` | native-hook | Yes | Normal: full sender + recipient check |
| `SessionEnd` | native-hook | Yes | Normal: full sender + recipient check |
| `session-rejected` | session-rejected | Yes | Halt: recipient (dispatcher) check only |
| `SessionStart` | native-hook | No | — |
| `UserPromptSubmit` | native-hook | No | — |
| All others | native-hook | No | — |

**Record-first, forward-after.** The callback forwards exactly the record that its own
append wrote while holding the journal lock. It never re-reads the latest journal line. If
this call wrote nothing (for example the lock was held or the input was refused), nothing is
forwarded. A session rejection written by an explicit `report`/`report-to` is never forwarded
as a Hook halt.

**Unconfirmed forward.** If forwarding fails or times out, the callback still returns `{}`,
does not retry, writes one line to stderr, and appends one journal record:

```json
{"source":"native-hook-unconfirmed","event":"Stop","native_session_id":"<record session>",
 "native":{"forwarded_source":"native-hook","forwarded_received_ms":1793585462096,
           "halt":false,"error":"submission-unconfirmed"}}
```

`error` is `submission-unconfirmed` when the native prompt call was made and its outcome is
unknown; it may already have been delivered. It is `not-submitted` when the send stopped
before the prompt call. Free-form error text is only written to stderr, never to disk. This
record is not a session fact: `require_unchanged` and the session association ignore it.

**Time bound.** Codex and Claude run each CrewMoor Hook with a 2 s timeout. All native
calls of one forward share a 1.5 s deadline; a call past it is not started, and the forward
is recorded as unconfirmed. The callback checks its context before reading stdin, and reads
at most 1 MiB. On the core runtime the deadline sets the socket read and write timeouts, so
a peer that keeps sending small chunks can extend one call beyond it.

**Halt path.** When a worker's session changes (e.g. `/clear`), the journal appends a
`session-rejected` record and the normal `message::send` route would refuse it (the worker's
`require_unchanged` fails). The halt path skips only the sender session check. It still requires
`from` to be the bound worker and `to` its dispatcher, and marks the message
`"system_halt_notification": true`.

Every rejected Hook appends its own `session-rejected` record, so after `/clear` each Hook
of the new session (for example `SessionStart`, `UserPromptSubmit` and `Stop`) sends one halt
notification. There is no system deduplication; the dispatcher writes its halt report once.

### Message content

Every forwarded message has `from = worker_role`, `to = dispatcher_role`,
`reply_to = null`, and `content` is JSON:

```json
{
  "feedback_instance": "<worker-uuid>",
  "task": "<task-id>",
  "agent": "codex|claude",
  "evidence": "native|synthetic",
  "record": { /* the journal Event that was written */ },
  "system_halt_notification": false
}
```

For halt notifications `system_halt_notification` is `true` and `record.source` is
`"session-rejected"`.

### Binding-ref

`team bind` writes `<registry>/instances/<worker-uuid>/binding-ref.json`:

```json
{"binding": "/absolute/path/to/roles/worker/binding.json"}
```

This is the only way the callback locates the binding. If the file is absent, the hook
continues local-only (no forwarding, no error).

---

## Dispatcher skill convention

These rules govern how the dispatcher processes forwarded Hook events.  They are
skill-layer policy and do not modify the `herdr-team-flow` global skill.

1. **Stop is a notice.** A forwarded `Stop` or `SessionEnd` tells the dispatcher the
   worker completed a turn. If there is nothing to do (e.g. no queued work), do **not**
   reply. Replying would prompt the worker and trigger another Stop.

2. **One reply per notice.** If the dispatcher does need to reply (assign next task,
   confirm receipt), send **at most one** reply per forwarded notice.

3. **Halt stops dispatch.** When a `system_halt_notification: true` message arrives,
   the dispatcher must:
   - Stop sending new work to the affected worker immediately.
   - Report the halt to the user with the worker role, the new session id, and the
     associated (old) session id from `record.native.associated_session`.
   - Await user decision before re-engaging that worker.
   - Write the halt report once, even when several halt notifications arrive.

4. **Halt reply once.** The dispatcher may send one acknowledgement to the user for a
   halt event; it does not reply to the worker.

### Anti-loop guarantee

- The dispatcher has no binding-ref written for it, so its own `Stop` events are
  never forwarded.
- Workers can receive at most one message per forwarded notice (rule 2 above).
- After a halt, no further work is dispatched until user action (rule 3 above).

---

## Scope limits

The following are **outside** this step:

- `StopFailure` (Codex has no such event; add when needed).
- Automatic recovery or retry after halt.
- Forwarding `SessionStart` or `UserPromptSubmit`.
- Any change to `herdr-team-flow`.
- Core team entry (#16) or queue/broker infrastructure.

---

## Evidence

`tests/feedback_hook_forward.rs` covers:

- Only bound workers forwarded; unbound instance is skipped.
- Event filter: Stop and SessionEnd forwarded; SessionStart/UserPromptSubmit not.
- Halt exception: session-rejected forwarded with `system_halt_notification: true`.
- Transport failure: the Stop is recorded, one `native-hook-unconfirmed` record is added,
  exactly one attempt is made, and a later normal send still works.
- An exhausted forwarding deadline starts no native call and records `not-submitted`.
- A failed append under a held lock forwards nothing and never resends an old rejection.
- A Hook appended while the binding is read does not replace the forwarded Stop.
- An agent-report rejection is never forwarded as a Hook halt.
- Halt sends keep the fixed worker→dispatcher direction (outsider and dispatcher senders are
  refused).
- Dispatcher events never forwarded (no binding-ref written for dispatcher by `team bind`).
- Subprocess integration: `crewmoor feedback hook` returns `{}\n`, records the Stop and forwards it;
  a failed forward prints one stderr line and adds one unconfirmed record.

These are synthetic. Real acceptance is documented in
[docs/issue-6-step3-real-acceptance.md](issue-6-step3-real-acceptance.md) once the live
run completes.
