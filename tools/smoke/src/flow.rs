//! The approved real acceptance sequence for #15. Steps stop on the first hard failure.
use crate::{Result, prompts, run::Run};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const LONG: u64 = 600;

fn s(list: &[&str]) -> Vec<String> {
    list.iter().map(|v| (*v).into()).collect()
}
fn read(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}
fn role_value(dir: &Path, role: &str, file: &str, pointer: &str) -> Result<String> {
    let value = read(&dir.join("roles").join(role).join(file));
    Ok(value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or("missing team record field")?
        .into())
}
struct Team {
    dir: PathBuf,
    registry: PathBuf,
}
impl Team {
    fn instance(&self, role: &str) -> Result<PathBuf> {
        let id = role_value(&self.dir, role, "context.json", "/instance")?;
        Ok(self.registry.join("instances").join(id))
    }
    fn pane(&self, role: &str) -> Result<String> {
        role_value(&self.dir, role, "launch/record.json", "/handle")
    }
    fn session(&self, role: &str) -> Result<Option<String>> {
        let path = self.instance(role)?.join("native-session.json");
        Ok(serde_json::from_value(read(&path)).ok())
    }
    fn journal(&self, role: &str) -> Result<String> {
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
fn preflight(run: &Run) -> Result<()> {
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
/// The team directory this run will create: `<state>/teams/<team>` from the approved
/// team config. Owned only if it did not exist before this run's `team start`.
fn expected_dir(run: &Run) -> Result<PathBuf> {
    let config = fs::read_to_string(run.get("team_toml")?)?;
    let name = config
        .lines()
        .find_map(|l| l.strip_prefix("team = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or("team config has no top-level team name")?;
    Ok(PathBuf::from(run.get("state")?).join("teams").join(name))
}
fn start(run: &Run) -> Result<Team> {
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
fn team_cmd(run: &Run, team: &Team, args: &[&str]) -> Result<Vec<String>> {
    let mut argv = vec![run.get("crewmoor")?, "team".into()];
    argv.extend(s(args));
    argv.insert(3, team.dir.display().to_string());
    Ok(argv)
}
fn bind(run: &Run, team: &Team) -> Result<()> {
    let before = json!({"lead":team.session("lead")?,"worker":team.session("worker")?});
    run.save("sessions-before-early-bind.json", &before)?;
    run.exec(
        "team-bind-early",
        &team_cmd(run, team, &["bind", "worker"])?,
    )?;
    let registered = || {
        matches!(
            (team.session("lead"), team.session("worker")),
            (Ok(Some(_)), Ok(Some(_)))
        )
    };
    if !run.wait("session-start-both", LONG, registered)? {
        return Err("SessionStart not registered for both roles".into());
    }
    let after = json!({"lead":team.session("lead")?,"worker":team.session("worker")?,
        "lead_journal":team.journal("lead")?,"worker_journal":team.journal("worker")?});
    run.save("sessions-registered.json", &after)?;
    let binding = team.dir.join("roles/worker/binding.json");
    if !binding.exists() {
        let (code, _) = run.exec("team-bind", &team_cmd(run, team, &["bind", "worker"])?)?;
        if code != 0 {
            return Err("team bind failed after registration".into());
        }
    }
    Ok(())
}
fn prompt(run: &Run, team: &Team, role: &str, label: &str, text: &str) -> Result<()> {
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
fn probe(run: &Run, binding: &str, id: &str, label: &str) -> Result<i32> {
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
    Ok(run.exec(label, &argv)?.0)
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
    let probe = probe(
        run,
        binding,
        "smoke15-probe-after-compact",
        "send-after-compact",
    )?;
    run.save(
        "compact.json",
        &json!({"compacted":compacted,"before":before,"after":compactions(&session),
        "hook_recorded":hooked,"session_after":team.session("worker")?,"probe_exit":probe,
        "journal":team.journal("worker")?}),
    )
}
fn clear(run: &Run, team: &Team, p: &prompts::Paths, binding: &str) -> Result<()> {
    prompt(run, team, "worker", "prompt-clear", "/clear")?;
    let rejected = run.wait("hook-rejection", 120, || {
        team.journal("worker")
            .is_ok_and(|j| j.contains("session-rejected"))
    })?;
    prompt(
        run,
        team,
        "worker",
        "prompt-after-clear",
        &prompts::after_clear(p),
    )?;
    let work = run.work.clone();
    run.wait("report-after-clear", LONG, || {
        work.join("worker-report-after-clear.exit").exists()
    })?;
    let probe = probe(
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
        &json!({"hook_rejection_recorded":rejected,"probe_exit":probe,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("crewmoor-smoke-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    /// A stand-in `crewmoor` that creates the team directory, then fails like a
    /// partial `team start` (error on stderr, no JSON on stdout).
    fn partial_start(dir: &Path) -> PathBuf {
        let script = dir.join("partial-crewmoor");
        fs::write(
            &script,
            "#!/bin/sh\nmkdir -p \"$4/teams/smoke\"\necho 'role worker start failed' >&2\nexit 1\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        script
    }
    fn plan(dir: &Path, crewmoor: &Path) -> Value {
        fs::write(
            dir.join("team.toml"),
            "team = \"smoke\"\nruntime = \"tui\"\n",
        )
        .unwrap();
        json!({"evidence":dir.join("evidence"),"crewmoor":crewmoor,"team_toml":dir.join("team.toml"),
            "state":dir,"host_socket":dir.join("synthetic.sock")})
    }
    #[test]
    fn partial_start_keeps_the_owned_team_path_for_one_stop() {
        let dir = scratch("partial");
        let run = Run::create(plan(&dir, &partial_start(&dir)), "synthetic").unwrap();
        assert!(start(&run).is_err());
        assert_eq!(run.team_dir.borrow().clone(), Some(dir.join("teams/smoke")));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn an_existing_team_directory_is_never_claimed() {
        let dir = scratch("existing");
        fs::create_dir_all(dir.join("teams/smoke")).unwrap();
        let run = Run::create(plan(&dir, &partial_start(&dir)), "synthetic").unwrap();
        assert!(start(&run).is_err());
        assert_eq!(run.team_dir.borrow().clone(), None);
        assert!(!run.evidence.join("01-team-start.json").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
