# Issue #9 existing-session acceptance

Batch: `crewmoor-issue-9-001`. Baseline:
`dfd2e6e9848f38fb2f236da7c72fd4f892e186a4`.

The user authorized one request and one actively authored reply between the
existing dispatcher and worker in the same Herdr TUI instance. No additional
Agent/model/server is launched. The CLI is Herdr 0.9.5 and the existing server is
0.9.4; no upgrade/restart is performed.

## Tested source and artifact

The source is `149090041adda1d05cf1685fbf7adfc0dbe2f474`, tree
`abd2dcc5a85e5019671367f306df47330c95e8b4`. The subsequent acceptance-document
commit changes only this evidence file; Rust, tests, transport and CLI are
identical to that tested source.

The exact binary is
`/home/gem/.local/state/crewmoor/crewmoor-issue-9-001/artifacts/crewmoor-message`,
SHA-256 `69b3d94126dbc721a528fd972a4ecd422c28e270e44af45c65935c59e6c197c8`.
Built with stable Rust `1.99.1 (58cf8f9c3 2026-10-13)`, Cargo `1.99.1`, using
the isolated target `/home/gem/.cache/crewmoor-issue-9-dev-target`.

The unchanged repository `verify` entry passed with exit 0: fmt, policy,
Clippy with `-D warnings`, real test coverage and CRAP <= 10. Its 500 measured
functions include the quality tool, fixture and business targets; maximum CRAP
is 10 (existing App request), while the new message functions reach at most 9.
All 25 quality tests and 66 business tests passed, including 8 new message
transport tests. Evidence: `developer-verify-002.log` in the batch directory,
SHA-256 `402130251286cf5d0ccf8d286ae6a2700fa0559e5115cd2632e4cf693a218d78`.
The log's `QUALITY GATE FAILED: invalid command; see --help` is the existing
quality CLI negative acceptance test's deliberate subprocess output; the full
verify succeeded. An earlier developer run failed on an inline `?` in a print
macro; that was fixed before the successful run and source commit.

## Actual identities

The same existing socket `/home/gem/.config/herdr/herdr.sock` was bound with
device `64512`, inode `2228277`, team `issue9-team`, workspace `wV`.

| Role | Agent | Pane | Terminal | Native session | Explicit feedback instance |
|---|---|---|---|---|---|
| dispatcher | Codex 0.166.0 | wV:p1 | term_65f3286f4852b114 | 01a19627-0399-7b92-9786-14624bb038b9 | 46f606135d954c2eb674fb5d6903dd96 |
| worker | Codex 0.166.0 | wV:p2 | term_65f3288e9b5d9115 | 01a19627-777e-7030-9c1a-ef2775886c9a | a5ba63e681b740c3bea886ad8231d17e |

Both native sessions were verified before creating explicit Issue #4 contexts.
No feedback marker was injected into the existing sessions. Binding read the
actual Agent identities, never their semantic/detected status. The private
binding and `message-bind-receipt.json` remain in the batch directory.

## Three separate observed facts

1. **Request accepted:** the dispatcher executed exactly once:

   ```text
   /home/gem/.local/state/crewmoor/crewmoor-issue-9-001/artifacts/crewmoor-message message send /home/gem/.local/state/crewmoor/crewmoor-issue-9-001/message-binding.json /home/gem/.local/state/crewmoor/crewmoor-issue-9-001/message-request.json
   ```

   `message-request-receipt.json` records `transport: accepted` for
   `crewmoor-issue-9-001-request-001`, dispatcher → worker. The request asks for
   an active reply and carries challenge `crewmoor-message-9-001`.

2. **Worker actually received and actively replied:** the request arrived in
   the existing worker conversation. The worker read it, authored
   `message-reply.json`, and executed the same binary/entry/binding once with
   that reply file. `message-reply-receipt.json` records native transport
   acceptance. Reply `crewmoor-issue-9-001-reply-001`, worker → dispatcher,
   has `reply_to: crewmoor-issue-9-001-request-001` and echoes the challenge.
   This is an Agent-authored response, not a Rust fixture or automatic reply.

3. **Dispatcher actually received the reply:** the dispatcher confirmed in its
   existing conversation that team, sender, recipient, reply correlation,
   challenge and worker native session all matched, then proactively returned
   `REAL_ROUNDTRIP_CONFIRMED`. Its separately authored
   `dispatcher-confirmation.json` preserves that observation. No third test
   message was sent.

After sending, the worker separately authored one legitimate Issue #4 `progress`
report about those observed receipt/submission facts. The saved
`message-worker-feedback-view.json` shows `source: agent-report`,
`latest_native_event: null` and `agent_state: unknown`. No SessionStart or hooks
state was fabricated, and transport did not generate business reports.

## Coverage and retained resources

Real coverage is **Codex → Codex → Codex in this one existing Herdr TUI
instance**, one request and one active reply. Automated Codex/Claude fixture
coverage remains synthetic. Other actual Agent combinations, herdr-core/App,
hooks-event collection, state forwarding (#6) and requirement intake (#8) were
not exercised or added.

No pane, Agent, server, model service or user checkout was created/closed for
the live exchange. The original checkout and its untracked AGENTS.md were left
alone. No proxy, authentication, `.codex/.env`, Hook installation/trust or
session configuration changed. Development used an independent worktree.
Rust fixture children exited and temporary test directories were removed by
their owners. The fixed binary, explicit contexts, binding and short message
evidence remain under the batch directory for independent review; they do not
constitute a background process or queue. The development checkout is retained
for audit and the dispatcher's later explicit publication handoff.
