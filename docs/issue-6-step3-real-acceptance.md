# Issue #6 step 3: Hook Stop forwarding, real acceptance

Batch `crewmoor-issue-6-step3-001`. The current run is **real-002 at candidate
`f8941ff40c48e408539f88854a89321fe4aabd8e`**. It includes the fixes for audit `48e6d59` F1–F5.
The dispatcher approved the plan (`real-002/plan.json`, SHA-256 `0eb60148…75c8`). It ran once
and nothing was retried. The real-001 record is kept below as history. **Issue #6 stays OPEN;
the dispatcher decides closure.**

## real-002 setup

| Item | Value |
|---|---|
| Container | User's default Herdr server `/home/gem/.config/herdr/herdr.sock`, CLI 0.9.5 / server 0.9.4, kept running |
| Roles | `lead` (dispatcher) and `worker`, both Codex CLI 0.166.0 with an `initial_prompt`, in new cwds |
| Team directory | `real-002/state/teams/i6s3-smoke2/` (new) |
| Panes / workspaces | lead `w14:p1` / `w14`, worker `w15:p1` / `w15` |
| Sessions | worker `01a19a2e-4bfc-7733-a3ff-0420a5a3c0d5`, worker after `/clear` `01a19a31-f1e9-74e0-b8b8-4b07047a02fc` |
| crewmoor | `dev/artifacts/crewmoor-f8941ff`, SHA-256 `597628e9…8ae5` |
| Driver | `dev/artifacts/crewmoor-smoke-f8941ff`, SHA-256 `bdae8349…1b20`, **exit 1** |

The callback was swapped to the candidate for the run and restored from
`real-002/callback-backup-bb277dde`. `real-002/run.log` ends with `RESTORE_OK`, and the restored
SHA-256 is `bb277dde4366145175158b3439bfacb89de379289706df4464c3c31370b7e497`. Hook definitions,
Agent settings, Codex trust and `~/.codex/.env` were not edited.

## real-002 results

| Phase | Result | Evidence |
|---|---|---|
| bind | **Pass** | Bound after both initial turns ended; the worker has `binding-ref.json`, the lead does not |
| convention (item 4) | **Pass** | Quiet for 90 s from the last change (94 s total). Worker prompts stay 1, so the lead's own Stop did not reach the worker |
| task (items 1, 2) | **Pass** | 2 notices: the worker's task Stop (`…199.3`) and its Stop after the reply (`…224.6`). 1 reply (`reply_to` = first notice `9cbe8caa…`, exit 0). Worker prompts grew by exactly 2. Quiet for 90 s from the last change |
| halt (item 3) | **Fail** (driver gate) | The rejection record, `lead-halt-report.md`, `lead_halts` = 3 = rejections, `lead_replies` 1, and unchanged worker prompts all hold, and the period is quiet. The gate `worker_unconfirmed == 0` failed: 1 record, explained below |
| callback (item 5) | **Pass** | `RESTORE_OK` |

`evidence/SHA256SUMS` verifies with `sha256sum -c`. The SHA256SUMS file itself has SHA-256
`a11e038d5f4b0eedd3f1c38618a4f670c6e91c735a3eca8791fc0ad8887baaf8`. `team stop` returned exit 0,
with `target-stopped-host-retained` for both roles. `w14`/`w15` are absent from the after lists.

### The unconfirmed record in the halt phase

The worker journal shows this sequence:

1. `/clear` at `…326`.
2. Three native-hook `session-rejected` records at `…328.9`, `…329.2` and `…331.7`, for the new session's `SessionStart`, `UserPromptSubmit` and `Stop`.
3. Each was forwarded as a halt. The lead logged all three.
4. At `…386.956`, about 60 s after `/clear` and before `team stop` (`…477`), Codex delivered `SessionEnd` for the **old** session `01a19a2e…`.

The old session is still the associated one, so this `SessionEnd` was recorded as a normal
`native-hook` event and took the normal forward path. That path checks the sender with
`require_unchanged`, which refuses, because the team has been halted by this worker's rejection.
The send stopped before the prompt call. The callback appended one `native-hook-unconfirmed`
record with `error: "not-submitted"`. Nothing was sent, and the lead's prompt count did not
change.

This matches the scope: after a halt, only the halt path skips the sender check, and other
events keep the full check. It is also the F2 behavior: a forward that did not happen is
recorded, not dropped. The driver gate assumed that no forward could fail in a successful run.
That assumption does not hold when a normal event arrives after the halt. The gate is recorded
here as failed, not reinterpreted.

### real-002 other observed facts

- **Bind timing.** Binding after each initial turn made the notice count deterministic, unlike real-001.
- **Halt repeats.** Each rejected Hook again sent one halt (3 in total), and all three appear in `lead-halts.jsonl`.
- **A late `SessionEnd`.** Codex delivered the old session's `SessionEnd` after `/clear` (in real-001, none was seen within the run). No cause is inferred.
- **The convention came from the driver's prompt.** This does not show that a dispatcher applies it on its own.

## real-002 not covered

A Claude role, the core team entry (#16) and its per-read socket timeouts, a deliberately
failed forward during a real run (F2 is covered by synthetic and subprocess tests), and the
formal callback deployment after merge.

## real-002 retained resources

Feedback contexts and journals: `/home/gem/.local/state/crewmoor/feedback/instances/{9fed2014…,5974db0d…}`.
Team directory, cwds, driver output, `run.log`, evidence and callback backup:
`/home/gem/.local/state/crewmoor/crewmoor-issue-6-step3-001/real-002/`. No Herdr resource from
this run remains open.

## History: real-001 at 77d1fd6

The first run's record is kept below as written. Only its headings were moved down one level.
Its quiet check was not met, and the audit of `48e6d59` found F1–F5 in that candidate.

### Setup

| Item | Value |
|---|---|
| Container | User's default Herdr server `/home/gem/.config/herdr/herdr.sock`, CLI 0.9.5 / server 0.9.4, kept running |
| Roles | `lead` (dispatcher) and `worker`, both Codex CLI 0.166.0 with an `initial_prompt`, in separate new cwds |
| Team directory | `real-001/state/teams/i6s3-smoke1/` (new) |
| Panes / workspaces | lead `w12:p1` / `w12`, worker `w13:p1` / `w13` |
| Sessions | lead `01a19a05…`, worker `01a19a06-12c8-71b3-8dc0-e86a37835af7`, worker after `/clear` `01a19a07-089c-74e1-a8a8-e9f9ef28ba9f` |
| crewmoor | `dev/artifacts/crewmoor-77d1fd6`, SHA-256 `8503facc…da49` |
| Driver | `dev/artifacts/crewmoor-smoke-77d1fd6`, SHA-256 `b82bcc4e…4910`, exit 0 |

The global Hook callback was swapped to the candidate build for the run. It was then restored from `real-001/callback-backup-bb277dde`. The restored SHA-256 matches `bb277dde4366145175158b3439bfacb89de379289706df4464c3c31370b7e497`. Hook definitions, Agent settings, Codex trust and `~/.codex/.env` were not edited.

### Results

| # | Acceptance item | Result | Evidence |
|---|---|---|---|
| 1 | Worker Stop is forwarded automatically and the lead receives it | **Pass** | Every worker `Stop` after bind is followed by a lead `UserPromptSubmit` within 0.4–1.1 s, and each one appears in `work/lead-notices.jsonl` (3 notices) |
| 2 | The lead replies at most once and the chain stops | **Partly met.** The designed quiet check was **not met** | One reply only (`lead-reply.json`, `reply_to` = first notice `c37a7a1c…`, exit 0). After the reply the worker's next Stop was a notice with no reply. The planned 90 s quiet check reported a change after 2 s at every phase; see below |
| 3 | After `/clear` the halt reaches the lead, which stops dispatching and writes a report | **Pass** | 3 `session-rejected` records from `native-hook`, each forwarded with `system_halt_notification: true` (`lead-halts.jsonl`). `lead-halt-report.md` was written once with both session ids. `lead_replies` stays 1 and the worker's `native-hook` prompt count stays 3 |
| 4 | The lead's own Stop is not forwarded | **Pass** | The lead has no `binding-ref.json`. After each of the lead's Stops (`…461.6`, `…506.7`, `…541.8` s), the worker's only new prompts are the driver's task prompt and the lead's one reply |
| 5 | Callback swapped and restored | **Pass** | `run.log`: pre `bb277dde…`, swapped `8503facc…`, restored `bb277dde…` |

`evidence/SHA256SUMS` covers the evidence files and `sha256sum -c` passes. The SHA256SUMS file itself has SHA-256 `b5c57b4d3caa8b40dee02f2b83e7ea7d7b6ee98f777cdb4cc44ae1368c1367e9`.

#### Why the quiet check was not met

The driver takes its snapshot as soon as the awaited file appears. At that moment the Agent's turn is still running, and its Stop (and the forwarded notice) lands seconds later. The counts therefore change within 2 s at every phase. The check failed because of the driver design. It is reported here as not met, not reinterpreted as a pass.

The journals show that the chain is finite. Times are the last digits of `received_ms`, in seconds.

- The worker's last `native-hook` event is `Stop` at `…513.9`. Its last event of any kind is the rejected `Stop` at `…519.7`.
- The lead's last event is `Stop` at `…541.8`. `team stop` ran at `…543`, with no new worker event in the 24 s before it.
- Final totals: worker prompts 3 (initial, task, the one reply), lead notices 3, lead replies 1, lead halts 3.

A later rerun could check for silence after the lead's last Stop, not right after the awaited file. That rerun is not part of this batch.

### Other observed facts

- **The plan's expected counts were wrong.** The worker's initial-prompt Stop (`…462.1`) came after `team bind`, so it was forwarded too. The plan expected 2 notices; the run observed 3. The lead's single reply answered that first notice.
- **Notices to a busy lead were not lost.** Two notices and all three halts arrived while the lead was in a turn. Each one still produced its own lead `UserPromptSubmit`: the lead has 8 `UserPromptSubmit` records, which are 1 initial + 1 convention + 3 notices + 3 halts.
- **Each rejected Hook sends a halt.** After `/clear`, the new session's `SessionStart`, `UserPromptSubmit` and `Stop` were each rejected and forwarded: 3 halt notifications, as documented in `docs/hook-forward.md`.
- **The convention came from the driver's prompt.** This does not show that a dispatcher applies the convention on its own.

### Not covered

A Claude role in a team, the core team entry (#16), and the formal callback deployment after merge.

### Retained resources

- Feedback contexts and journals: `/home/gem/.local/state/crewmoor/feedback/instances/{de1ed4ba…,d664c0e0…}`.
- Team directory, cwds, driver output, evidence and callback backup: `/home/gem/.local/state/crewmoor/crewmoor-issue-6-step3-001/real-001/`.
- `team stop` reported `target-stopped-host-retained` for both roles. `w12`/`w13` are absent from the after lists, and no Herdr resource from this run remains open.
