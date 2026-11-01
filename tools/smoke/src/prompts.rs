//! Operator task texts. They state the exact commands; the Agents run them themselves.
use crate::{Result, run::Run};

pub struct Paths {
    pub crewmoor: String,
    pub binding: String,
    pub work: String,
    pub challenge: String,
}
impl Paths {
    pub fn new(run: &Run, binding: &str) -> Result<Self> {
        Ok(Self {
            crewmoor: run.get("crewmoor")?,
            binding: binding.into(),
            work: run.work.display().to_string(),
            challenge: run.get("challenge")?,
        })
    }
}
/// Sent once to the dispatcher before the worker reports.
pub fn lead(p: &Paths) -> String {
    format!(
        "Issue #15 smoke test, operator instruction. You are role `lead` (dispatcher). \
Soon one JSON team message from role `worker` will arrive as a new prompt. When it arrives: \
(1) save the exact JSON you received to {w}/lead-received.json; \
(2) write {w}/lead-reply.json containing {{\"id\":\"smoke15-lead-reply-001\",\"from\":\"lead\",\
\"to\":\"worker\",\"reply_to\":\"<message.id from the received JSON>\",\
\"content\":\"lead received challenge {c}\"}}; \
(3) run exactly once: {x} message send {b} {w}/lead-reply.json > {w}/lead-reply.out 2>&1; \
echo $? > {w}/lead-reply.exit . Do not resend or retry. Until that message arrives, do nothing.",
        w = p.work,
        c = p.challenge,
        x = p.crewmoor,
        b = p.binding,
    )
}
/// The worker's first prompt: it starts the Codex session, so SessionStart fires.
pub fn worker_first() -> String {
    "You are role worker in the Issue 15 smoke team. Do nothing until an operator instruction arrives.".into()
}
/// Sent once to the worker after bind.
pub fn worker(p: &Paths) -> String {
    format!(
        "Issue #15 smoke test, operator instruction. You are role `worker`. Run exactly once: \
{x} feedback report-to {b} \"$CODEX_THREAD_ID\" agent progress \"worker challenge {c}\" \
> {w}/worker-report.out 2>&1; echo $? > {w}/worker-report.exit . \
Then wait. When one JSON team message from `lead` arrives as a new prompt, save the exact JSON \
you received to {w}/worker-received.json and do nothing else. Do not resend or retry.",
        w = p.work,
        c = p.challenge,
        x = p.crewmoor,
        b = p.binding,
    )
}
/// Sent after compaction: one plain turn so Hooks fire under the kept session.
pub fn after_compact() -> String {
    "Issue #15 smoke test: reply with the single word OK. Run no commands.".into()
}
/// Sent after /clear: the worker reports once more with its current session.
pub fn after_clear(p: &Paths) -> String {
    format!(
        "Issue #15 smoke test, operator instruction. Run exactly once: \
{x} feedback report-to {b} \"$CODEX_THREAD_ID\" agent progress \"after clear\" \
> {w}/worker-report-after-clear.out 2>&1; echo $? > {w}/worker-report-after-clear.exit ; \
echo \"$CODEX_THREAD_ID\" > {w}/worker-thread-after-clear.txt . Do not retry.",
        w = p.work,
        x = p.crewmoor,
        b = p.binding,
    )
}
