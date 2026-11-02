# Issue #6 step 3: Hook Stop forwarding, real acceptance real-001

Batch `crewmoor-issue-6-step3-001`. The run used candidate `77d1fd6ec478ce8ab5e7d9e3b1f86264bf319dca` with the dispatcher-approved plan (`real-001/plan.json`, SHA-256 `03b57628…3820`). It ran once and nothing was retried. **Issue #6 stays OPEN; the dispatcher decides closure.**

## Setup

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

## Results

| # | Acceptance item | Result | Evidence |
|---|---|---|---|
| 1 | Worker Stop is forwarded automatically and the lead receives it | **Pass** | Every worker `Stop` after bind is followed by a lead `UserPromptSubmit` within 0.4–1.1 s, and each one appears in `work/lead-notices.jsonl` (3 notices) |
| 2 | The lead replies at most once and the chain stops | **Partly met.** The designed quiet check was **not met** | One reply only (`lead-reply.json`, `reply_to` = first notice `c37a7a1c…`, exit 0). After the reply the worker's next Stop was a notice with no reply. The planned 90 s quiet check reported a change after 2 s at every phase; see below |
| 3 | After `/clear` the halt reaches the lead, which stops dispatching and writes a report | **Pass** | 3 `session-rejected` records from `native-hook`, each forwarded with `system_halt_notification: true` (`lead-halts.jsonl`). `lead-halt-report.md` was written once with both session ids. `lead_replies` stays 1 and the worker's `native-hook` prompt count stays 3 |
| 4 | The lead's own Stop is not forwarded | **Pass** | The lead has no `binding-ref.json`. After each of the lead's Stops (`…461.6`, `…506.7`, `…541.8` s), the worker's only new prompts are the driver's task prompt and the lead's one reply |
| 5 | Callback swapped and restored | **Pass** | `run.log`: pre `bb277dde…`, swapped `8503facc…`, restored `bb277dde…` |

`evidence/SHA256SUMS` covers the evidence files and `sha256sum -c` passes. The SHA256SUMS file itself has SHA-256 `b5c57b4d3caa8b40dee02f2b83e7ea7d7b6ee98f777cdb4cc44ae1368c1367e9`.

### Why the quiet check was not met

The driver takes its snapshot as soon as the awaited file appears. At that moment the Agent's turn is still running, and its Stop (and the forwarded notice) lands seconds later. The counts therefore change within 2 s at every phase. The check failed because of the driver design. It is reported here as not met, not reinterpreted as a pass.

The journals show that the chain is finite. Times are the last digits of `received_ms`, in seconds.

- The worker's last `native-hook` event is `Stop` at `…513.9`. Its last event of any kind is the rejected `Stop` at `…519.7`.
- The lead's last event is `Stop` at `…541.8`. `team stop` ran at `…543`, with no new worker event in the 24 s before it.
- Final totals: worker prompts 3 (initial, task, the one reply), lead notices 3, lead replies 1, lead halts 3.

A later rerun could check for silence after the lead's last Stop, not right after the awaited file. That rerun is not part of this batch.

## Other observed facts

- **The plan's expected counts were wrong.** The worker's initial-prompt Stop (`…462.1`) came after `team bind`, so it was forwarded too. The plan expected 2 notices; the run observed 3. The lead's single reply answered that first notice.
- **Notices to a busy lead were not lost.** Two notices and all three halts arrived while the lead was in a turn. Each one still produced its own lead `UserPromptSubmit`: the lead has 8 `UserPromptSubmit` records, which are 1 initial + 1 convention + 3 notices + 3 halts.
- **Each rejected Hook sends a halt.** After `/clear`, the new session's `SessionStart`, `UserPromptSubmit` and `Stop` were each rejected and forwarded: 3 halt notifications, as documented in `docs/hook-forward.md`.
- **The convention came from the driver's prompt.** This does not show that a dispatcher applies the convention on its own.

## Not covered

A Claude role in a team, the core team entry (#16), and the formal callback deployment after merge.

## Retained resources

- Feedback contexts and journals: `/home/gem/.local/state/crewmoor/feedback/instances/{de1ed4ba…,d664c0e0…}`.
- Team directory, cwds, driver output, evidence and callback backup: `/home/gem/.local/state/crewmoor/crewmoor-issue-6-step3-001/real-001/`.
- `team stop` reported `target-stopped-host-retained` for both roles. `w12`/`w13` are absent from the after lists, and no Herdr resource from this run remains open.
