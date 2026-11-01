# Issue #15 real acceptance (TUI)

Batch `crewmoor-issue-15-001`. The current real run is **real-002 at candidate
`f2d339ab6f34fc9e260280ceb30fc908be020b51`**. Its `src/`, `Cargo.toml` and
`Cargo.lock` are identical to `5d5c635`, so it includes every fix from audits
001–003 and the Hook-first check order. The dispatcher approved the plan
(`real-002/plan.json` SHA-256 `30992dce…e025`), and the user chose callback
option A again. The run was executed once and nothing was retried.
**Issue #15 stays OPEN; the dispatcher decides closure.**

## real-002 setup

| Item | Value |
|---|---|
| Container | User's default Herdr server `/home/gem/.config/herdr/herdr.sock`, CLI 0.9.5 / server 0.9.4, kept running |
| Roles | `lead` (dispatcher) with one `initial_prompt`; `worker` **without** one. Both Codex CLI 0.166.0, separate new cwds |
| Team directory | `real-002/state/teams/issue15-smoke2/` (new) |
| Panes / workspaces | lead `w0:p1` / `w0`, worker `w11:p1` / `w11` |
| Sessions | lead `01a1979f-e915-7763-9aa2-075119c8ccd5`, worker `01a1979f-fcc2-74d0-a1c8-296610fa8635` |
| crewmoor | `dev/artifacts/crewmoor-f2d339a`, SHA-256 `83f5a4c8…beaa8` |
| Driver | `dev/artifacts/crewmoor-smoke-f2d339a`, SHA-256 `21825933…e958`, exit 0 |

The global Hook callback was swapped to the candidate build for the run. It was
then restored from `real-002/callback-backup-bb277dde`, and the restored
SHA-256 matches `bb277dde4366145175158b3439bfacb89de379289706df4464c3c31370b7e497`.
Hook definitions, Agent settings and Codex trust entries were not edited.

## real-002 results

| # | Acceptance item | Result | Evidence |
|---|---|---|---|
| 1 | Both SessionStarts register a session, from the Hook | **Pass** | Both journals start with `native-hook` `SessionStart` `source: startup`. The worker's arrived only after the driver's first prompt. |
| 2 | `team bind` succeeds; errors before registration | **Pass** | With the lead registered and the worker not, the early bind exited 1 with `native session not registered yet; bind after the Agent's SessionStart` and wrote no binding (`early-bind.json`). After the worker's first prompt its SessionStart arrived within 2 s, and `team bind` exited 0. |
| 3 | One `report-to`, dispatcher receives and replies once | **Pass** | Worker `report-to` exit 0, `recorded: true`, `transport: accepted`, message `a8d686cc827144f3a9f59920eaae1a7f`. The lead saved the envelope it received and replied once with `smoke15-lead-reply-001`, `reply_to` that id, exit 0. The worker saved the reply it received. |
| 4 | After `/clear`, Hooks and `report-to` are refused with a record visible in `feedback show` | **Pass** | New session `01a197a1-72af-7601-a6cc-b628993a616f`: `SessionStart` (`source: clear`) and `UserPromptSubmit` gave `session-rejected` records with `from: native-hook`. `report-to` exited 1 with "rejection recorded". The lead→worker send exited 1 with stderr `Hook reported a changed native session; team halted, binding kept`, so `halted_by_hook: true`. `feedback show`: `latest_session_rejection` is present, `agent_state: unknown`, and the business report is unchanged. |
| 5 | After compaction, Hooks still recorded and binding usable | **Pass** | The rollout `compacted` count went 0→1. The Hook then recorded `SessionStart` `source: compact` with the same session id, followed by `UserPromptSubmit`/`Stop`. A send after compaction was `accepted`. |
| 6 | `team stop` ends only team resources; user sessions unchanged | **Pass** | Exit 0, both roles `target-stopped-host-retained`, `w0`/`w11` closed, no `session stop`. `agent list` before and after shows the same 9 panes with the same sessions; `workspace list` shows `wD`, `wS`, `wV` both times. |
| 7 | The smoke program runs the flow with evidence and hashes | **Pass** | `real-002/evidence/SHA256SUMS` covers 49 files and `sha256sum -c` passes. The SHA256SUMS file itself has SHA-256 `4ea4dc1bbcd00e8c33058c9ee0e912e2972d69f3317b3362261ec7b43909635e`. |

Transport acceptance, actual receipt and Hook records are reported separately.
The probe sends were written by the driver; the worker report and lead reply
are the Agent-written business messages. `Stop` and `transport: accepted` are
not treated as business completion.

## real-002 other observed facts

- Lead `agent start` returned `timeout` (`accepted: null`); worker `accepted: true`.
  Both Codex processes were located by `process-info`. No start was resent.
- Unlike real-001, the worker journal has no `SessionEnd` for the old session
  before the `/clear` rejection. This is recorded as observed; no cause is
  inferred.
- After `/clear`, the new session's first Hook arrived 2 s after the next prompt,
  consistent with real-001.

## Not covered

- The core team entry is not implemented; see #16. Core marker injection has
  synthetic tests only.
- A Claude role in a team was not run.

## Retained resources

Feedback contexts and journals: `/home/gem/.local/state/crewmoor/feedback/instances/`.
Team directory, cwds, driver output, evidence and callback backup:
`/home/gem/.local/state/crewmoor/crewmoor-issue-15-001/real-002/`. No Herdr
resource from this run remains open.

## History: real-001 at f645a32

The first run's record is kept below as written; only its headings were moved down
one level. That run was made before the audit fixes, and item 2's refusal before
registration was not observed in it.

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

When this record was written, these fixes had synthetic regression tests only;
real-002 above now runs them. The results below describe `f645a32`.

### Setup

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

### Results

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

### Other observed facts

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

### Not covered

- The real refusal of `team bind` before registration (item 2), as above.
- The core team entry is not implemented; see #16. Core marker injection
  (`terminal.run` `env`) has synthetic tests only and no real acceptance.
- A Claude role in a team was not run.

### Retained resources

The feedback contexts and journals stay under
`/home/gem/.local/state/crewmoor/feedback/instances/`. The team directory,
cwds, driver output, evidence and callback backup stay under
`/home/gem/.local/state/crewmoor/crewmoor-issue-15-001/real-001/`. No Herdr
resource from this run remains open.
