# Issue #6 second-step acceptance

Batch `crewmoor-issue-6-step2-001`; baseline and requested target revision:
`d30ba0ff337308c8de6a5739f09986a8a6095fe2`.

This records one actual Agent-layer handling sample using the existing
`feedback report-to` and `message send` entries. The worker calculated a
specified Git blob's SHA256, reported it once, and the dispatcher personally
received it, independently recalculated it and decided to accept the sample.
The dispatcher sent one correlated judgement reply; the worker personally
received and checked it, then confirmed through ordinary Herdr collaboration.
The coordinator subsequently saved `coordinator-confirmation.json` and
reported the milestone to the user-facing agent. **Issue #6 remains OPEN.**

Only this document is added to the repository. Production code, tests,
configuration, dependencies and the prior acceptance document are unchanged.
No hash comparison or business judgement was added to the transport.

## Specified object and independent calculations

| Field | Actual value |
|---|---|
| File | `docs/issue-6-step1-acceptance.md` |
| Revision | `d30ba0ff337308c8de6a5739f09986a8a6095fe2` |
| Git blob | `ced0ff74956e082da7c0be094892648006949471` |
| Blob type / bytes | `blob` / `7531` |
| Worker SHA256 | `e9a6f10d00e9395bf8844e8dce4846779f4082c465fd0731119be1b2192bda86` |
| Dispatcher SHA256 | `e9a6f10d00e9395bf8844e8dce4846779f4082c465fd0731119be1b2192bda86` |

The actual worker invoked the retained Rust tool's `calculate` command once.
In its independent checkout, it resolved the exact revision and blob, checked
type and size, extracted the blob bytes and hashed those bytes with
`sha256sum`. The command sequence was:

```text
git rev-parse --verify d30ba0ff337308c8de6a5739f09986a8a6095fe2^{commit}
git rev-parse --verify d30ba0ff337308c8de6a5739f09986a8a6095fe2:docs/issue-6-step1-acceptance.md
git cat-file -t ced0ff74956e082da7c0be094892648006949471
git cat-file -s ced0ff74956e082da7c0be094892648006949471
git cat-file blob ced0ff74956e082da7c0be094892648006949471
sha256sum
```

The last command received the exact extracted bytes on stdin. Every command's
argv, cwd, exit code, stdout and stderr is recorded under `worker-computation/`;
the driver exited 0. `worker-calculation.json` binds the path/revision/blob,
byte count and digest. This was not a hash copied from the document's tables,
the working-tree file or another actor. The value was not separately sent to
the dispatcher before the progress report.

Only after actually receiving the progress report did the dispatcher author
and run its separate Rust calculation under `dispatcher-computation/`, using
its own Git and SHA256 commands in `/home/gem/crewmoor`. Its retained command,
object, byte and exit evidence matches the requested object and digest.
`coordinator-decision.json` records its Agent-authored `accept` decision and
reason: revision, path, blob, byte count and independently calculated hash
matched. The common repository/object identity does not replace that separate
calculation, and no worker calculation output was used as its hash input.

## Fixed actors and existing contract

The pair reused its existing Herdr TUI socket
`/home/gem/.config/herdr/herdr.sock`, device `64512`, inode `2228277`, workspace
`wV`, team `issue6-step2`. Herdr CLI `0.9.5` / server `0.9.4` and both Codex CLI
`0.166.0` sessions were retained. Backend model identity was not independently
verified; no model selection or new Agent/pane/service was started.

| Role | Pane / terminal | Native session | Explicit feedback instance |
|---|---|---|---|
| dispatcher | wV:p1 / term_65f3286f4852b114 | 01a19627-0399-7b92-9786-14624bb038b9 | 0e4f9de4dc0f42c0882e0471c7761ca7 |
| worker | wV:p2 / term_65f3288e9b5d9115 | 01a19627-777e-7030-9c1a-ef2775886c9a | e8ec3111892d4162a2e96466ca6a0c34 |

The coordinator reviewed the frozen binaries, Rust tool source and plan,
then ran `prepare` once, exit 0, creating only the two explicit task contexts
and fixed binding. The tool supports calculation/preparation and has no send
or judgement capability. No user-level Hook/callback installation was changed.

The existing Message contract permits dispatcher → worker routing and a
non-self `reply_to`; the transfer code does not implement conversation tracking
or decide whether a hash is correct. Both agents checked those business
relations themselves. No new message schema or production adapter was needed.

## Recorded facts, acceptance and actual receipt

After the coordinator's `GO_REPORT_ONCE`, the worker called the frozen
`feedback report-to` once with its own previously calculated summary, source
`agent`, type `progress`. The system appended that exact Event and transferred
it using the existing message channel. `live/report-to.exit` is 0, stderr is
empty, and its derived stdout receipt has `recorded: true` and
`delivery.transport: accepted`.

The actual transfer is `f09600dd6cc14c2e94609d60cb63ad5a`, worker → dispatcher,
`reply_to: null`. The sole journal Event has
`received_ms: 1793532956215`, `source: agent-report`, `event: progress`, the
worker session, `native: {}` and a JSON summary containing the specified
file/revision/blob/bytes/SHA256 and `challenge: feedback-6-step2-001`.
The dispatcher personally received that envelope and checked it against the
binding, journal's single line and receipt before independently calculating.
Transport acceptance alone was not treated as receipt or task success.

The dispatcher then authored one Message,
`crewmoor-issue-6-step2-001-decision-001`, dispatcher → worker,
`reply_to: f09600dd6cc14c2e94609d60cb63ad5a`. Its content states that the
sample was accepted and ended, names the exact file/revision/blob/byte count,
reports its independently computed hash and preserves the challenge. It sent
that reply once through the unchanged `message send` entry; execution and
derived stdout are retained under `live/dispatcher-reply-*`.

The worker actually received this reply in its own conversation. It checked
the fixed team/from/to and new reply id, matched `reply_to` to its saved
transfer id, and compared the file/revision/blob/bytes/hash/decision/challenge
with its own calculation and report receipt. Only then did it save
`worker-receipt-confirmation.json` and proactively notify the coordinator
through ordinary Herdr collaboration. `worker-received-decision.json` is a
saved copy of that received envelope, not a raw transport capture. The
coordinator read the worker's confirmation and saved the final
`coordinator-confirmation.json`.

There was one test report and one judgement reply. No recalculation/reprepare
or report resend occurred, and worker confirmation was not a third test
report/Message. The saved receipts are derived CrewMoor stdout; no missing raw
Herdr prompt/RPC reply was reconstructed. Actual receipt rests separately on
each agent's firsthand confirmation. The sample's completion is the
dispatcher's explicit judgement, not a Stop event or transport-derived state.

## Artifact, validation and limits

The exact baseline was built with stable Rust `1.99.1` / Cargo `1.99.1` in
`/home/gem/.cache/crewmoor-issue-6-step2-dev-target`. Frozen artifacts under
`/home/gem/.local/state/crewmoor/crewmoor-issue-6-step2-001/`:

| Artifact | SHA256 |
|---|---|
| `artifacts/crewmoor` | `57864b5bf2301e57a33dd7f4e0f5a0039d8b46d5c13bb18ac50d6f28e4286205` |
| `artifacts/acceptance-driver` | `f26669dba1ff86b9ad62c258157fddd336c2449ea99d20e6283770205d856e94` |

The standalone Rust tool passed fmt, Clippy `-D warnings` and build; its
source/Cargo and exact calculation/preparation plan remain in the batch.
The final document-only candidate uses the existing repository's full
`verify` entry with that isolated target; the exact candidate, command, exit
and log hash are retained in `developer-validation.json`. No new tests or
quality exemptions are added. Independent exact-SHA audit and PR/main CI
evidence are separate from the real sample above.

No Hook auto-forwarding, state inference, recovery, retry, queue, deduplication
framework, #8, Agent/runtime matrix or third step is included. Local role and
session attribution retain the existing trusted-input boundary. The assignment
message supplied the decision rule in advance: a matching result is accepted;
a mismatch requires review. This sample verifies that the dispatcher received
real feedback, independently checked it against that given rule and sent one
reply through the existing channel. It does **not** establish that a dispatcher
can autonomously decide how to handle feedback without predefined rules.
Only the matching-result branch was exercised; mismatch/review handling was
not tested.

User checkout,
AGENTS.md, `.codex/.env`, proxies, old HOLD and existing sessions stayed intact.
No new runtime resource needs closing. All batch source/Cargo, frozen binaries,
calculation outputs, contexts/journal, receipts and confirmations are retained.
The dispatcher's own calculation tool/binary belongs to p1 and is not cleaned
by the developer; the developer owns only its dedicated build target and
temporary checkout until publication handoff. Issue #6 stays **OPEN**.
