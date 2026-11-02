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

**Record-first, forward-after.** The local record is always written before forwarding is
attempted. A forwarding failure is suppressed; the callback always returns `{}` and never
retries. An uncertain send might already have delivered text; do not repeat the hook event
to recover an unknown result.

**Halt path.** When a worker's session changes (e.g. `/clear`), the journal appends a
`session-rejected` record and the normal `message::send` route would refuse it (the worker's
`require_unchanged` fails). The halt path skips the sender session check and sends to the
dispatcher only, marking the message `"system_halt_notification": true`.

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
- Transport failure marks forwarding unknown; event record still exists; callback returns `{}`.
- Dispatcher events never forwarded (no binding-ref written for dispatcher by `team bind`).
- Subprocess integration: `crewmoor feedback hook` returns `{}\n`; Stop event recorded.

These are synthetic. Real acceptance is documented in
[docs/issue-6-step3-real-acceptance.md](issue-6-step3-real-acceptance.md) once the live
run completes.
