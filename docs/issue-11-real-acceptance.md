# Issue #11 core message acceptance

Batch `crewmoor-issue-11-001`; baseline
`29784e4af3bdcda4cf7fde47bbf6b0b092c91f61`.

One real request and one actively authored reply completed on the same existing
herdr-core instance. Dedicated core dispatcher `t26` sent to worker `t25`;
the worker replied through the same CrewMoor message entry and the core
dispatcher confirmed actual receipt. The existing TUI coordinator `wV:p1`
coordinated and reviewed this exchange; it was not presented as a core endpoint.
No current TUI session was moved into core.

## Source, versions and automatic checks

Tested candidate `fc4bb348a2f60364b4203f8723a441dd9a705baf`, tree
`979fcee0c379b826b88be10a2c84f62fadeb6f16`, CrewMoor `0.1.0`.
The final evidence-document commit changes only this file. The fixed binary
was built at `5b4f691ad5e87333c79d81d0a97174ca89f9edc3`; its only difference
from `fc4bb34` is a TUI configuration test, with production source unchanged.

Artifacts under `/home/gem/.local/state/crewmoor/crewmoor-issue-11-001/`:

| Artifact | SHA-256 |
|---|---|
| `artifacts/crewmoor-message` | `4ee7ec15a17bb8936c2a5e2a112c327b20b58b3797301e408f57b3f19e4265ab` |
| `artifacts/live-driver` | `cae840fbf184bc6998a9b9de022306aca3c1d02fb8f71689b2fe572d62764b67` |
| `developer-verify-004.log` | `ba1840731dc6775f6de438d45b97572647ad5fb28862919b034699355560050d` |

Stable Rust `1.99.1 (58cf8f9c3 2026-10-13)`, Cargo `1.99.1`, isolated target
`/home/gem/.cache/crewmoor-issue-11-dev-target`. The unchanged full quality
`verify` entry passed with exit 0: fmt, policy, Clippy `-D warnings`, real test
coverage and CRAP <= 10, covering 533 functions, 25 quality tests and 72 business
tests. The six new core message tests use a synthetic Rust socket server and
owned fixture processes; existing TUI tests also pass, including a configuration
with the new runtime fields omitted. Those tests are separate from this live
Agent exchange. The standalone Rust acceptance driver passed Clippy and build;
its source and exact plan are retained in `live-driver/` and `real-plan.json`.

The running core and installed `hc` are `0.1.15`. Both agents use the
**Claude Code 2.1.316 client**. The coordinator observed
**`deepseek/deepseek-flash` as the actual backend model** in their native
interfaces, using the existing configuration; no model selection was changed.
This is not evidence of a Claude backend model. Current App version is
**unknown** and App GUI observation was not performed.

## Instance and native identities

Socket `/run/user/1000/herdr-core/default.sock`, device `54`, inode `92169`;
core id `4a8f843e64e0a7778943d8a31c25cc26`. Protocol source
`4a3591aa835c197f57a5d1b8e8951a75e2a79faa` and installed schema were checked.
Developer and auditor independently confirmed native plain JSON-RPC using
read-only `server.status` and `terminal.get` calls on that socket. See the
[implementation and protocol contract](messages.md).

| Role | Local core address | Explicit feedback instance | Native session |
|---|---|---|---|
| dispatcher | t26 | a666bf84b2c34c4a84c52b6ebc252d08 | 5a0dc565-e929-4e82-9b8d-c2e3a91c9b4f |
| worker | t25 | 10f0819ec56d471fb6c518561beab1e8 | f77740eb-cd6c-4e98-9a52-cb8ff24799be |

Each agent was launched once as a dedicated interactive terminal using native
`--session-id`, exact Claude executable/cwd and its own
`CREWMOOR_FEEDBACK_INSTANCE`. Actual global user-level SessionStart records in
the existing feedback registry match both UUIDs; no SessionStart was
manufactured. Core `get` supplied only address/generation/argv/cwd/kernel
process facts. It has no native Agent session field and cannot independently
prove conversation switching within a process. `Terminal.identity` and
terminal/OSC status were never interpreted as Agent session or business state.

## Separate observed facts

The coordinator reviewed the exact Rust driver and hashes, then executed
`prepare`, `start worker`, `start dispatcher`, `bind` and `kickoff` once each,
all with exit 0. The final step sent one native operator instruction to the
dedicated dispatcher. The two test messages were then sent by the actual agents:

1. **Request accepted:** core dispatcher `t26` invoked
   `artifacts/crewmoor-message message send live/binding.json live/request.json`.
   `live/request-receipt.json` records `transport: accepted`, `native_turn: 0`
   for `crewmoor-issue-11-001-request-001`, dispatcher → worker.
2. **Worker received and actively replied:** worker `t25` personally read the
   delivered envelope, authored `live/reply.json`, then invoked the same binary
   and entry with that file. The response is
   `crewmoor-issue-11-001-reply-001`, worker → dispatcher,
   `reply_to: crewmoor-issue-11-001-request-001`, and echoes
   `challenge=core-message-11-001`. `live/reply-receipt.json` records
   `transport: accepted`, `native_turn: 1`. Rust did not generate that answer.
3. **Dispatcher received the reply:** core dispatcher authored
   `live/dispatcher-confirmation.json` only after the reply arrived natively
   in its own session, recording the received envelope and matching
   team/from/to/reply correlation/challenge. The TUI coordinator inspected it,
   both SessionStart records and the matching receipts, then saved
   `coordinator-confirmation.json` and proactively reported `REAL_CONFIRMED`.

Both receipts are **derived CrewMoor receipts**, not saved raw underlying
`terminal.send` JSON-RPC responses. No missing raw response was reconstructed.
The coordinator retained `live/dispatcher-observed.txt` and
`live/worker-observed.txt` only as message receipt corroboration. Native core
wait was used to time evidence collection, not to establish business completion.
No automatic resend or third test message occurred.

## Resource cleanup and limits

After confirming the exchange, the coordinator executed the reviewed driver's
`cleanup` once, exit 0. `live/worker-cleanup.json` and
`live/dispatcher-cleanup.json` record removal of owned `t25` and `t26` plus
process-exit confirmation. Cleanup checked core/socket and the saved native
terminal identity, launch tuple and owner label before kill/rm. The external
core/holder and user terminal `t1n` remained. Old HOLD resources were not test
targets and were not changed. No service was started, upgraded or stopped;
no proxy, `.codex/.env`, Hook configuration/trust or authentication was edited.

The batch directory, fixed binaries, driver, launch/identity/cleanup receipts,
explicit feedback contexts and short evidence logs remain for review. The
development checkout/target are retained pending audit/publication handoff;
their later cleanup is separate from the completed live terminal cleanup.

Coverage is one actual Claude client pair on this core, with the observed
DeepSeek backend. There is no real core Codex or Agent combination matrix,
App GUI test, state forwarding (#6), requirement intake (#8), queue, recovery
service or automatic retry. Independent exact-candidate audit and CI remain
separate requirements; this document records only the evidence above.
