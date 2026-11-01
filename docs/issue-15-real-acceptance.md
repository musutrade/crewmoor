# Issue #15 real acceptance (TUI)

Batch `crewmoor-issue-15-001`, plan `real-001`, candidate
`f645a32a360afa518055d547d6ce1dfdda134da5` (baseline `fc09ebe`). The dispatcher
approved the plan (`plan.json` SHA-256 `c2f54d7c…cb94c`) and the user chose
callback option A. The run was executed once and nothing was retried.
**Issue #15 stays OPEN; the dispatcher decides closure.**

**Code revision.** The real run used `f645a32`. Audit AUDIT_RESULT-001 then
returned needs-fix, and four fixes were made after this run:

- role names are validated before any path is used;
- a Hook-recorded session change halts bind and send;
- a new TUI binding must locate the Agent process;
- the smoke driver keeps the path of a team it created, even after a partial
  start.

Audit AUDIT_RESULT-002 led to three more fixes:

- the session check now reads the journal without its lock, so it cannot make a
  Hook append fail;
- the smoke driver now decodes the team name with the `toml` parser;
- the smoke test double is now Rust.

These fixes have synthetic regression tests only. The real results below
describe `f645a32`, not the fixed candidate.

## Setup

| Item | Value |
|---|---|
| Container | User's default Herdr server `/home/gem/.config/herdr/herdr.sock`, CLI 0.9.5 / server 0.9.4, kept running |
| Roles | `lead` (dispatcher) and `worker`, both Codex CLI 0.166.0, separate new cwds; each with one `initial_prompt` |
| Team directory | `real-001/state/teams/issue15-smoke/` (new) |
| Feedback instances | lead `ac7b2710c86641849b81d9a951510c5e`, worker `12294406d8844826a9033f934ff03dcf` |
| Panes / workspaces | lead `wY:p1` / `wY`, worker `wZ:p1` / `wZ` |
| Sessions | lead `01a1975d-08c4-7101-abc5-8b95e484026e`, worker `01a1975d-1d3e-7f33-9156-e7db2d9e02f5` |
| crewmoor | `dev/artifacts/crewmoor-f645a32`, SHA-256 `9bcb585f…bec51b` |
| Driver | `dev/artifacts/crewmoor-smoke-f645a32` (`tools/smoke`), SHA-256 `4e90f67b…63ba46`, exit 0 |

Codex was proposed for both roles because a Claude dispatcher would need a Bash
permission flag that #15 does not allow. The global Hook callback was swapped
to the frozen build for the run. Afterwards it was restored from
`real-001/callback-backup-bb277dde`, and the restored SHA-256 matches
`bb277dde4366145175158b3439bfacb89de379289706df4464c3c31370b7e497`. The Hook
definitions, Agent settings and Codex trust entries were not edited.

## Results

| # | Acceptance item | Result | Evidence |
|---|---|---|---|
| 1 | Both SessionStarts register a session, from the Hook | **Pass** | Both journals start with `native-hook` `SessionStart` `source: startup`. Codex's Hook child inherited the `--env` marker, which settles open risk 2. |
| 2 | `team bind` succeeds; errors before registration | **Pass / not covered** | Bind succeeded (`05-team-bind-early.json`). Both sessions were already registered when the early bind ran (`sessions-before-early-bind.json`), so the refusal before registration was not observed for real. That part rests only on the synthetic test. |
| 3 | One `report-to`, dispatcher receives and replies once | **Pass** | Worker `report-to` exit 0, `recorded: true`, `transport: accepted`, message `b9322654245843e58d2097059777ed77`. The lead saved the envelope it received (`work/lead-received.json`) and sent one reply, `smoke15-lead-reply-001`, with that `reply_to` (exit 0, accepted). The worker saved the reply it received (`work/worker-received.json`). |
| 4 | After `/clear`, Hooks and `report-to` are refused with a journal record visible in `feedback show` | **Pass** | Old session `SessionEnd` (`reason: other`). New session `01a1975e-eb0c-7f83-94b1-043e86383130`: `SessionStart` (`source: clear`), `UserPromptSubmit` and `report-to` each produced a `session-rejected` record. `report-to` exit 1, "rejection recorded". `feedback show` returns `latest_session_rejection`, `agent_state: unknown`, and the business report is unchanged. A later lead→worker send was refused with "bound Agent native session changed". |
| 5 | After compaction, Hooks still recorded and binding usable | **Pass** | The rollout `compacted` count went 0→1. The Hook then recorded `SessionStart` with `source: compact` and the **same** session id, followed by `UserPromptSubmit`/`Stop`. A probe send after compaction was `accepted`. |
| 6 | `team stop` ends only team resources; user sessions unchanged | **Pass** | Exit 0, both roles `target-stopped-host-retained`. `wY`/`wZ` were closed and no `session stop` was issued. `agent list` before and after shows the same 9 panes with the same sessions (`wD:p1/pQ/pX/p1H`, `wS:p1`, `wV:p5/p7/p8/p9`). `workspace list` shows `wD`, `wS`, `wV` both times. |
| 7 | The smoke program runs the flow with evidence and hashes | **Pass** | `real-001/evidence/SHA256SUMS` covers 45 files and `sha256sum -c` passes. The SHA256SUMS file itself has SHA-256 `2813e752…ce3c`. |

Transport acceptance, actual receipt and Hook records are reported separately
above. The probe messages in steps 5–6 were written by the driver and labelled as
such; the only Agent-authored business messages are the worker report and the
lead reply. `Stop` and `transport: accepted` are not treated as business
completion.

## Other observed facts

- **Start admission unknown, processes verified.** Both official `agent start`
  calls returned `timeout` within their 5000 ms wait (`accepted: null`,
  `request_error: timeout`). `process-info` then located each Codex process
  (`process_verified: true`, executable
  `/home/gem/.codex/packages/standalone/releases/0.166.0-x86_64-unknown-linux-musl/bin/codex`).
  Admission and actual process start are reported separately, as required. No
  start was resent.
- **Codex `/clear` is lazy.** `SessionEnd` fired at `/clear`. The new thread's
  `SessionStart` (`source: clear`) arrived only with the next prompt, after the
  driver's 120 s rejection wait had already ended (`wait-hook-rejection.json`:
  `met: false`). The final journal contains the record. This resolves the
  question in open risk 1 about SessionStart timing: with an initial prompt, a
  Codex session exists at start.
- **Compaction source.** Codex 0.166.0 reported `SessionStart` with
  `source: compact`, a value not in the locally generated `ThreadStartSource`
  enum (`startup`, `clear`). The id did not change, so it was recorded as normal
  operation.

## Not covered

- The real refusal of `team bind` before registration (item 2), as above.
- The core team entry is not implemented; see #16. Core marker injection
  (`terminal.run` `env`) has synthetic tests only and no real acceptance.
- A Claude role in a team was not run.

## Retained resources

The feedback contexts and journals stay under
`/home/gem/.local/state/crewmoor/feedback/instances/`. The team directory,
cwds, driver output, evidence and callback backup stay under
`/home/gem/.local/state/crewmoor/crewmoor-issue-15-001/real-001/`. No Herdr
resource from this run remains open.
