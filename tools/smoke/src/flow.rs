//! The approved real acceptance sequence for #15. Steps stop on the first hard failure.
use crate::{Result, prompts, run::Run};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) const LONG: u64 = 600;

fn s(list: &[&str]) -> Vec<String> {
    list.iter().map(|v| (*v).into()).collect()
}
fn read(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}
pub(crate) fn role_value(dir: &Path, role: &str, file: &str, pointer: &str) -> Result<String> {
    let value = read(&dir.join("roles").join(role).join(file));
    Ok(value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or("missing team record field")?
        .into())
}
pub struct Team {
    pub(crate) dir: PathBuf,
    pub(crate) registry: PathBuf,
}
impl Team {
    pub(crate) fn instance(&self, role: &str) -> Result<PathBuf> {
        let id = role_value(&self.dir, role, "context.json", "/instance")?;
        Ok(self.registry.join("instances").join(id))
    }
    pub(crate) fn pane(&self, role: &str) -> Result<String> {
        role_value(&self.dir, role, "launch/record.json", "/handle")
    }
    pub(crate) fn session(&self, role: &str) -> Result<Option<String>> {
        let path = self.instance(role)?.join("native-session.json");
        Ok(serde_json::from_value(read(&path)).ok())
    }
    pub(crate) fn journal(&self, role: &str) -> Result<String> {
        Ok(fs::read_to_string(self.instance(role)?.join("events.jsonl")).unwrap_or_default())
    }
}
pub fn execute(run: &Run) -> Result<()> {
    preflight(run)?;
    let team = start(run)?;
    bind(run, &team)?;
    let binding = team
        .dir
        .join("roles/worker/binding.json")
        .display()
        .to_string();
    let paths = prompts::Paths::new(run, &binding)?;
    exchange(run, &team, &paths)?;
    compact(run, &team, &binding)?;
    clear(run, &team, &paths, &binding)
}
pub(crate) fn preflight(run: &Run) -> Result<()> {
    let crewmoor = PathBuf::from(run.get("crewmoor")?);
    if crate::run::sha256(&crewmoor)? != run.get("crewmoor_sha256")? {
        return Err("frozen crewmoor binary differs from approved plan".into());
    }
    run.exec("agent-list-before", &s(&["herdr", "agent", "list"]))?;
    run.exec("workspace-list-before", &s(&["herdr", "workspace", "list"]))?;
    run.exec(
        "server-status",
        &s(&["herdr", "status", "server", "--json"]),
    )?;
    let callback = PathBuf::from(run.get("callback")?);
    run.save(
        "callback.json",
        &json!({"path":callback,"sha256":crate::run::sha256(&callback)?}),
    )
}
#[derive(serde::Deserialize)]
struct TeamName {
    team: String,
}
/// The team directory this run will create: `<state>/teams/<team>`, with the team name
/// decoded by the same TOML parser as the real launch. Owned only if absent before start.
pub fn expected_dir(run: &Run) -> Result<PathBuf> {
    let config: TeamName = toml::from_str(&fs::read_to_string(run.get("team_toml")?)?)?;
    let name = Path::new(&config.team);
    if name.components().count() != 1 || name.file_name().is_none() {
        return Err("team name is not a single directory name".into());
    }
    Ok(PathBuf::from(run.get("state")?).join("teams").join(name))
}
pub fn start(run: &Run) -> Result<Team> {
    let expected = expected_dir(run)?;
    if expected.exists() {
        return Err("team directory already exists before start; not owned by this run".into());
    }
    let argv = vec![
        run.get("crewmoor")?,
        "team".into(),
        "start".into(),
        run.get("team_toml")?,
        run.get("state")?,
    ];
    let result = run.exec("team-start", &argv);
    // Keep the owned path even when start fails part-way, so finish stops it once.
    if expected.exists() {
        *run.team_dir.borrow_mut() = Some(expected.clone());
    }
    let (code, out) = result?;
    let value: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    if code != 0 || value["team_dir"].as_str() != expected.to_str() {
        return Err("team start failed or returned another team directory".into());
    }
    Ok(Team {
        dir: expected,
        registry: PathBuf::from(run.get("registry")?),
    })
}
pub(crate) fn team_cmd(run: &Run, team: &Team, args: &[&str]) -> Result<Vec<String>> {
    let mut argv = vec![run.get("crewmoor")?, "team".into()];
    argv.extend(s(args));
    argv.insert(3, team.dir.display().to_string());
    Ok(argv)
}
/// Item 2: the worker starts without an initial prompt, so it has no session yet and
/// the first bind must be refused. The driver then sends its first prompt.
fn bind(run: &Run, team: &Team) -> Result<()> {
    let lead = run.wait("session-start-lead", LONG, || {
        team.session("lead").is_ok_and(|s| s.is_some())
    })?;
    let before = json!({"lead":team.session("lead")?,"worker":team.session("worker")?});
    run.save("sessions-before-early-bind.json", &before)?;
    let (early, _, error) = run.exec_full(
        "team-bind-early",
        &team_cmd(run, team, &["bind", "worker"])?,
    )?;
    let binding = team.dir.join("roles/worker/binding.json");
    run.save(
        "early-bind.json",
        &json!({"lead_registered":lead,
        "worker_registered_before":before["worker"],"exit":early,"stderr":error,
        "refused_unregistered":early != 0 && error.contains("not registered"),
        "binding_written":binding.exists()}),
    )?;
    if early == 0 {
        return Err("early bind was not refused; item 2 is not covered as planned".into());
    }
    prompt(
        run,
        team,
        "worker",
        "prompt-worker-first",
        &prompts::worker_first(),
    )?;
    let registered = || matches!(team.session("worker"), Ok(Some(_)));
    if !run.wait("session-start-worker", LONG, registered)? {
        return Err("worker SessionStart not registered".into());
    }
    let after = json!({"lead":team.session("lead")?,"worker":team.session("worker")?,
        "lead_journal":team.journal("lead")?,"worker_journal":team.journal("worker")?});
    run.save("sessions-registered.json", &after)?;
    let (code, _) = run.exec("team-bind", &team_cmd(run, team, &["bind", "worker"])?)?;
    if code != 0 {
        return Err("team bind failed after registration".into());
    }
    Ok(())
}
pub(crate) fn prompt(run: &Run, team: &Team, role: &str, label: &str, text: &str) -> Result<()> {
    let argv = vec![
        "herdr".into(),
        "agent".into(),
        "prompt".into(),
        team.pane(role)?,
        text.into(),
    ];
    run.exec(label, &argv).map(|_| ())
}
fn exchange(run: &Run, team: &Team, p: &prompts::Paths) -> Result<()> {
    prompt(run, team, "lead", "prompt-lead", &prompts::lead(p))?;
    prompt(run, team, "worker", "prompt-worker", &prompts::worker(p))?;
    let work = run.work.clone();
    let done = |name: &str| work.join(name).exists();
    let report = run.wait("worker-report", LONG, || done("worker-report.exit"))?;
    let reply = run.wait("lead-reply", LONG, || done("lead-reply.exit"))?;
    let received = run.wait("worker-received", LONG, || done("worker-received.json"))?;
    run.save(
        "journal-after-exchange.json",
        &json!({"worker":team.journal("worker")?}),
    )?;
    if !(report && reply && received) {
        return Err("report/reply exchange incomplete; see work/".into());
    }
    Ok(())
}
fn compactions(session: &str) -> usize {
    let home = std::env::var("HOME").unwrap_or_default();
    let root = PathBuf::from(home).join(".codex/sessions");
    let out = std::process::Command::new("grep")
        .args(["-rlF", session])
        .arg(&root)
        .arg("--include=*.jsonl")
        .output();
    let file = out
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let file = file
        .lines()
        .find(|l| l.contains(session))
        .unwrap_or_default()
        .to_owned();
    fs::read_to_string(file)
        .unwrap_or_default()
        .matches("\"type\":\"compacted\"")
        .count()
}
fn probe(run: &Run, binding: &str, id: &str, label: &str) -> Result<(i32, String)> {
    let message = json!({"id":id,"from":"lead","to":"worker","reply_to":null,
        "content":"Operator probe through the lead binding (driver-authored). Reply only OK; run no commands."});
    let path = run.work.join(format!("{id}.json"));
    fs::write(&path, message.to_string())?;
    let argv = vec![
        run.get("crewmoor")?,
        "message".into(),
        "send".into(),
        binding.into(),
        path.display().to_string(),
    ];
    let (code, _, error) = run.exec_full(label, &argv)?;
    Ok((code, error))
}
fn compact(run: &Run, team: &Team, binding: &str) -> Result<()> {
    let session = team.session("worker")?.ok_or("worker session unknown")?;
    let before = compactions(&session);
    let lines = team.journal("worker")?.lines().count();
    prompt(run, team, "worker", "prompt-compact", "/compact")?;
    let compacted = run.wait("compacted", LONG, || compactions(&session) > before)?;
    prompt(
        run,
        team,
        "worker",
        "prompt-after-compact",
        &prompts::after_compact(),
    )?;
    let hooked = run.wait("hook-after-compact", LONG, || {
        team.journal("worker")
            .is_ok_and(|j| j.lines().count() > lines + 1)
    })?;
    let (probe, probe_stderr) = probe(
        run,
        binding,
        "smoke15-probe-after-compact",
        "send-after-compact",
    )?;
    run.save(
        "compact.json",
        &json!({"compacted":compacted,"before":before,"after":compactions(&session),
        "hook_recorded":hooked,"session_after":team.session("worker")?,"probe_exit":probe,
        "probe_stderr":probe_stderr,
        "journal":team.journal("worker")?}),
    )
}
/// Item 4: Codex records the new session only with the next prompt after /clear,
/// so the rejection is awaited after that prompt; then one lead→worker send.
fn clear(run: &Run, team: &Team, p: &prompts::Paths, binding: &str) -> Result<()> {
    prompt(run, team, "worker", "prompt-clear", "/clear")?;
    prompt(
        run,
        team,
        "worker",
        "prompt-after-clear",
        &prompts::after_clear(p),
    )?;
    let hook = |j: &str| {
        j.contains("\"source\":\"session-rejected\"") && j.contains("\"from\":\"native-hook\"")
    };
    let rejected = run.wait("hook-rejection", LONG, || {
        team.journal("worker").is_ok_and(|j| hook(&j))
    })?;
    let work = run.work.clone();
    run.wait("report-after-clear", LONG, || {
        work.join("worker-report-after-clear.exit").exists()
    })?;
    let (code, error) = probe(
        run,
        binding,
        "smoke15-probe-after-clear",
        "send-after-clear",
    )?;
    let show = vec![
        run.get("crewmoor")?,
        "feedback".into(),
        "show".into(),
        team.registry.display().to_string(),
        role_value(&team.dir, "worker", "context.json", "/instance")?,
    ];
    run.exec("feedback-show-worker", &show)?;
    run.save(
        "clear.json",
        &json!({"hook_rejection_recorded":rejected,"send_exit":code,
        "send_stderr":error,"halted_by_hook":code != 0 && error.contains("team halted"),
        "journal":team.journal("worker")?}),
    )
}
/// Always attempt one team stop if a team directory exists, then list agents again.
pub fn finish(run: &Run) -> Result<()> {
    let dir = run.team_dir.borrow().clone();
    if let Some(dir) = dir {
        let argv = vec![
            run.get("crewmoor")?,
            "team".into(),
            "stop".into(),
            dir.display().to_string(),
        ];
        run.exec("team-stop", &argv)?;
    }
    run.exec("agent-list-after", &s(&["herdr", "agent", "list"]))?;
    run.exec("workspace-list-after", &s(&["herdr", "workspace", "list"]))?;
    Ok(())
}
