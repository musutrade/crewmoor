//! The approved real acceptance sequence for #6 step 3: a bound worker's Hook Stop is
//! forwarded to the dispatcher. Waits read only Hook journals and files the Agents write.
use crate::{
    Result,
    flow::{self, LONG, Team},
    run::Run,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

/// Quiet period, from the last observed change, for the chain to count as stopped.
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

/// Journal records of `role` that match `keep`.
fn count(team: &Team, role: &str, keep: impl Fn(&Value) -> bool) -> usize {
    team.journal(role)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| keep(e))
        .count()
}
/// Native-hook journal records with this event name.
fn hooks(team: &Team, role: &str, event: &str) -> usize {
    count(team, role, |e| {
        e["source"] == "native-hook" && e["event"] == event
    })
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
        "worker_rejections": count(team, "worker", |e| {
            e["source"] == "session-rejected" && e["native"]["from"] == "native-hook"
        }),
        "worker_unconfirmed": count(team, "worker", |e| e["source"] == "native-hook-unconfirmed"),
    })
}
fn n(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or(u64::MAX)
}
/// Save a phase result; a failed check fails the phase and so the driver.
fn verdict(run: &Run, name: &str, pass: bool, record: Value) -> Result<()> {
    let mut record = record;
    record["pass"] = json!(pass);
    run.save(&format!("{name}.json"), &record)?;
    println!("check {name}: pass={pass}");
    if pass {
        Ok(())
    } else {
        Err(format!("{name} check failed; see {name}.json").into())
    }
}
/// Both roles start with an initial prompt. Bind only after each initial turn has ended,
/// so every later worker Stop is forwarded and the expected notice count is fixed.
fn bind(run: &Run, team: &Team) -> Result<()> {
    for role in ["lead", "worker"] {
        let ended = || matches!(team.session(role), Ok(Some(_))) && hooks(team, role, "Stop") >= 1;
        if !run.wait(&format!("initial-turn-{role}"), LONG, ended)? {
            return Err(format!("{role} initial turn not recorded").into());
        }
    }
    let (code, _) = run.exec(
        "team-bind",
        &flow::team_cmd(run, team, &["bind", "worker"])?,
    )?;
    let refs =
        |role: &str| -> Result<bool> { Ok(team.instance(role)?.join("binding-ref.json").exists()) };
    let (worker, lead) = (refs("worker")?, refs("lead")?);
    verdict(
        run,
        "bind",
        code == 0 && worker && !lead,
        json!({"exit":code,"worker_binding_ref":worker,"lead_binding_ref":lead,
        "counts":counts(run, team)}),
    )
}
/// Quiet means no count changed for `SETTLE` s, measured from the last change seen.
fn settle(run: &Run, team: &Team, label: &str) -> Result<(Value, bool)> {
    let start = Instant::now();
    let mut last = counts(run, team);
    let mut changed_at = Instant::now();
    let mut changes = 0u32;
    let quiet_for = Duration::from_secs(SETTLE);
    while changed_at.elapsed() < quiet_for && start.elapsed() < Duration::from_secs(LONG) {
        std::thread::sleep(Duration::from_secs(2));
        let now = counts(run, team);
        if now != last {
            last = now;
            changed_at = Instant::now();
            changes += 1;
        }
    }
    let quiet = changed_at.elapsed() >= quiet_for;
    let seconds = start.elapsed().as_secs();
    run.save(
        &format!("settle-{label}.json"),
        &json!({"quiet":quiet,"changes":changes,"seconds":seconds,"counts":last}),
    )?;
    println!("settle {label}: quiet={quiet} after {seconds}s, {changes} change(s)");
    Ok((last, quiet))
}
/// The dispatcher convention from docs/hook-forward.md, given to the lead once.
fn convention(run: &Run, team: &Team, p: &Paths) -> Result<()> {
    let before = counts(run, team);
    flow::prompt(run, team, "lead", "prompt-lead-convention", &lead(p))?;
    let ready = run.work.join("lead-ready.txt");
    let acked = run.wait("lead-ready", LONG, || ready.exists())?;
    let (after, quiet) = settle(run, team, "after-convention")?;
    // The lead's own Stop must not reach the worker.
    let not_forwarded = n(&after, "worker_prompts") == n(&before, "worker_prompts");
    verdict(
        run,
        "convention",
        acked && quiet && not_forwarded && n(&after, "lead_notices") == 0,
        json!({"before":before,"after":after,"lead_ready":acked,"quiet":quiet,
        "lead_stop_not_forwarded":not_forwarded}),
    )
}
/// Items 1, 2 and 4: one worker task; its Stop and the Stop after the one reply are
/// forwarded; the lead replies once and the chain goes quiet.
fn task(run: &Run, team: &Team, p: &Paths) -> Result<()> {
    let before = counts(run, team);
    flow::prompt(run, team, "worker", "prompt-worker-task", &worker(p))?;
    let w = run.work.clone();
    let replied = run.wait("lead-reply", LONG, || w.join("lead-reply.exit").exists())?;
    let received = run.wait("worker-received", LONG, || {
        w.join("worker-received.json").exists()
    })?;
    let stops = n(&before, "worker_stops") + 2;
    let second = run.wait("second-notice", LONG, || {
        hooks(team, "worker", "Stop") as u64 >= stops && lines(&w.join("lead-notices.jsonl")) >= 2
    })?;
    let (after, quiet) = settle(run, team, "after-task")?;
    let reply_exit = fs::read_to_string(w.join("lead-reply.exit")).unwrap_or_default();
    let one_reply = n(&after, "lead_replies") == 1
        && reply_exit.trim() == "0"
        && n(&after, "worker_prompts") == n(&before, "worker_prompts") + 2;
    let pass = replied
        && received
        && second
        && quiet
        && one_reply
        && n(&after, "lead_notices") == 2
        && n(&after, "lead_halts") == 0
        && n(&after, "worker_unconfirmed") == 0;
    verdict(
        run,
        "task",
        pass,
        json!({"before":before,"after":after,"lead_replied":replied,
        "worker_received_reply":received,"second_notice":second,"quiet":quiet,
        "one_reply":one_reply,"reply_exit":reply_exit.trim(),
        "journal_worker":team.journal("worker")?,"journal_lead":team.journal("lead")?}),
    )
}
/// Item 3: after /clear the worker's next turn is rejected by its Hook and the halt
/// notice goes to the lead, which stops dispatching and writes a report.
fn halt(run: &Run, team: &Team, p: &Paths) -> Result<()> {
    let before = counts(run, team);
    flow::prompt(run, team, "worker", "prompt-clear", "/clear")?;
    flow::prompt(run, team, "worker", "prompt-after-clear", AFTER_CLEAR)?;
    let rejected = run.wait("hook-rejection", LONG, || {
        counts(run, team)["worker_rejections"].as_u64() > Some(0)
    })?;
    let report = run.work.join("lead-halt-report.md");
    let reported = run.wait("lead-halt-report", LONG, || report.exists())?;
    let (after, quiet) = settle(run, team, "after-halt")?;
    let show = vec![
        p.crewmoor.clone(),
        "feedback".into(),
        "show".into(),
        team.registry.display().to_string(),
        flow::role_value(&team.dir, "worker", "context.json", "/instance")?,
    ];
    run.exec("feedback-show-worker", &show)?;
    let no_dispatch = n(&after, "lead_replies") == 1
        && n(&after, "worker_prompts") == n(&before, "worker_prompts");
    let pass = rejected
        && reported
        && quiet
        && no_dispatch
        && n(&after, "lead_halts") >= 1
        && n(&after, "worker_unconfirmed") == 0;
    verdict(
        run,
        "halt",
        pass,
        json!({"before":before,"after":after,"hook_rejection_recorded":rejected,
        "lead_halt_report":reported,"quiet":quiet,"no_further_dispatch":no_dispatch,
        "all_halts_logged":after["lead_halts"] == after["worker_rejections"],
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
