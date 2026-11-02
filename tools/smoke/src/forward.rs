//! The approved real acceptance sequence for #6 step 3: a bound worker's Hook Stop is
//! forwarded to the dispatcher. Waits read only Hook journals and files the Agents write.
use crate::{
    Result,
    flow::{self, LONG, Team},
    run::Run,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

/// Quiet period that must pass with no new Hook prompt or notice for the chain to count as stopped.
const SETTLE: u64 = 90;

pub fn execute(run: &Run) -> Result<()> {
    flow::preflight(run)?;
    let team = flow::start(run)?;
    bind(run, &team)?;
    let paths = Paths::new(run, &team)?;
    convention(run, &team, &paths)?;
    task(run, &team, &paths)?;
    halt(run, &team, &paths)
}

struct Paths {
    crewmoor: String,
    binding: String,
    work: String,
    challenge: String,
}
impl Paths {
    fn new(run: &Run, team: &Team) -> Result<Self> {
        Ok(Self {
            crewmoor: run.get("crewmoor")?,
            binding: team
                .dir
                .join("roles/worker/binding.json")
                .display()
                .to_string(),
            work: run.work.display().to_string(),
            challenge: run.get("challenge")?,
        })
    }
}

/// Native-hook journal lines with this event name.
fn hooks(team: &Team, role: &str, event: &str) -> usize {
    team.journal(role)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| e["source"] == "native-hook" && e["event"] == event)
        .count()
}
fn lines(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}
fn counts(run: &Run, team: &Team) -> Value {
    let w = &run.work;
    json!({
        "worker_prompts": hooks(team, "worker", "UserPromptSubmit"),
        "worker_stops": hooks(team, "worker", "Stop"),
        "lead_prompts": hooks(team, "lead", "UserPromptSubmit"),
        "lead_stops": hooks(team, "lead", "Stop"),
        "lead_notices": lines(&w.join("lead-notices.jsonl")),
        "lead_halts": lines(&w.join("lead-halts.jsonl")),
        "lead_replies": usize::from(w.join("lead-reply.exit").exists()),
        "worker_rejections": team.journal("worker").unwrap_or_default()
            .matches("\"source\":\"session-rejected\"").count(),
    })
}
/// Both roles start with an initial prompt, so both sessions register before bind.
fn bind(run: &Run, team: &Team) -> Result<()> {
    for role in ["lead", "worker"] {
        let registered = || matches!(team.session(role), Ok(Some(_)));
        if !run.wait(&format!("session-start-{role}"), LONG, registered)? {
            return Err(format!("{role} SessionStart not registered").into());
        }
    }
    let (code, _) = run.exec(
        "team-bind",
        &flow::team_cmd(run, team, &["bind", "worker"])?,
    )?;
    let refs =
        |role: &str| -> Result<bool> { Ok(team.instance(role)?.join("binding-ref.json").exists()) };
    run.save(
        "bind.json",
        &json!({"exit":code,"worker_binding_ref":refs("worker")?,
        "lead_binding_ref":refs("lead")?,"counts":counts(run, team)}),
    )?;
    if code != 0 {
        return Err("team bind failed".into());
    }
    Ok(())
}
/// Waits `SETTLE` s for any change; `met` false is the expected, stopped chain.
fn settle(run: &Run, team: &Team, label: &str) -> Result<(Value, bool)> {
    let snapshot = counts(run, team);
    let moved = run.wait(label, SETTLE, || counts(run, team) != snapshot)?;
    Ok((snapshot, !moved))
}
/// The dispatcher convention from docs/hook-forward.md, given to the lead once.
fn convention(run: &Run, team: &Team, p: &Paths) -> Result<()> {
    flow::prompt(run, team, "lead", "prompt-lead-convention", &lead(p))?;
    let ready = run.work.join("lead-ready.txt");
    if !run.wait("lead-ready", LONG, || ready.exists())? {
        return Err("lead did not acknowledge the convention".into());
    }
    let (after, stopped) = settle(run, team, "settle-after-convention")?;
    run.save(
        "convention.json",
        &json!({"counts":after,"stopped":stopped,
        "lead_stop_not_forwarded":after["worker_prompts"] == 1}),
    )
}
/// Items 1, 2 and 4: one small worker task, its forwarded Stop, at most one reply.
fn task(run: &Run, team: &Team, p: &Paths) -> Result<()> {
    let before = counts(run, team);
    flow::prompt(run, team, "worker", "prompt-worker-task", &worker(p))?;
    let w = run.work.clone();
    let replied = run.wait("lead-reply", LONG, || w.join("lead-reply.exit").exists())?;
    let received = run.wait("worker-received", LONG, || {
        w.join("worker-received.json").exists()
    })?;
    let second = run.wait("second-notice", LONG, || {
        lines(&w.join("lead-notices.jsonl")) >= 2
    })?;
    let (after, stopped) = settle(run, team, "settle-after-task")?;
    run.save(
        "task.json",
        &json!({"before":before,"after":after,"lead_replied":replied,
        "worker_received_reply":received,"second_notice":second,"stopped":stopped,
        "journal_worker":team.journal("worker")?,"journal_lead":team.journal("lead")?}),
    )?;
    if !(replied && received && second) {
        return Err("forwarded Stop exchange incomplete; see work/".into());
    }
    Ok(())
}
/// Item 3: after /clear the worker's next turn is rejected by its Hook and the halt
/// notice goes to the lead, which stops dispatching and writes a report.
fn halt(run: &Run, team: &Team, p: &Paths) -> Result<()> {
    let before = counts(run, team);
    flow::prompt(run, team, "worker", "prompt-clear", "/clear")?;
    flow::prompt(run, team, "worker", "prompt-after-clear", AFTER_CLEAR)?;
    let rejected = run.wait("hook-rejection", LONG, || {
        team.journal("worker").is_ok_and(|j| {
            j.contains("\"source\":\"session-rejected\"") && j.contains("\"from\":\"native-hook\"")
        })
    })?;
    let report = run.work.join("lead-halt-report.md");
    let reported = run.wait("lead-halt-report", LONG, || report.exists())?;
    let (after, stopped) = settle(run, team, "settle-after-halt")?;
    let show = vec![
        p.crewmoor.clone(),
        "feedback".into(),
        "show".into(),
        team.registry.display().to_string(),
        flow::role_value(&team.dir, "worker", "context.json", "/instance")?,
    ];
    run.exec("feedback-show-worker", &show)?;
    run.save(
        "halt.json",
        &json!({"before":before,"after":after,"hook_rejection_recorded":rejected,
        "lead_halt_report":reported,"stopped":stopped,
        "journal_worker":team.journal("worker")?}),
    )
}
const AFTER_CLEAR: &str = "Issue #6 smoke test: reply with the single word OK. Run no commands.";
fn lead(p: &Paths) -> String {
    format!(
        "Issue #6 step 3 smoke test, operator instruction. You are role `lead` (dispatcher). \
From now on, JSON team messages from role `worker` may arrive as new prompts. Each has \
message.content, a JSON string with fields `record` and `system_halt_notification`. Rules: \
(A) If system_halt_notification is false: append the received JSON as one line to \
{w}/lead-notices.jsonl. If {w}/lead-reply.exit does NOT exist yet, write {w}/lead-reply.json \
containing {{\"id\":\"smoke6-lead-reply-001\",\"from\":\"lead\",\"to\":\"worker\",\
\"reply_to\":\"<message.id from the received JSON>\",\"content\":\"lead ack {c}. Save this \
exact message as JSON to {w}/worker-received.json and run no other command.\"}} and run exactly \
once: {x} message send {b} {w}/lead-reply.json > {w}/lead-reply.out 2>&1; echo $? > \
{w}/lead-reply.exit . If {w}/lead-reply.exit already exists, do not reply: a Stop is only a \
notice. (B) If system_halt_notification is true: append the received JSON as one line to \
{w}/lead-halts.jsonl. If {w}/lead-halt-report.md does not exist, write it with the worker role, \
record.native_session_id and record.native.associated_session, and state that no further work \
is sent to the worker. Never message the worker after a halt. Never resend or retry. \
Now write the word ready to {w}/lead-ready.txt and then wait for messages.",
        w = p.work,
        c = p.challenge,
        x = p.crewmoor,
        b = p.binding,
    )
}
fn worker(p: &Paths) -> String {
    format!(
        "Issue #6 step 3 smoke test, operator instruction. You are role `worker`. Write the \
text `worker task {c}` to {w}/worker-task.txt and then end your turn. Run no other command and \
do not report. If a JSON team message from `lead` arrives later as a new prompt, follow the \
instruction in its content and nothing else.",
        w = p.work,
        c = p.challenge,
    )
}
