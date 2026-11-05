//! Direct TUI team regressions: real CrewMoor wrapper, synthetic native Agent/API.
use super::{Setup, common, no_layout, opencode_watch};
use crewmoor::{
    Result,
    config::AgentKind,
    engine, feedback,
    message::{Binding, Native},
    native::Runner,
    opencode::{endpoint, prepare},
    process,
    record::Record,
    team,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

const PROMPT: &str = "一次初始化：\"中文\" \\路径";

struct Tui<'a> {
    fake: &'a common::Fake,
    shell: Child,
    unknown: bool,
    reject_export: bool,
}
impl<'a> Tui<'a> {
    fn new(fake: &'a common::Fake, unknown: bool, reject_export: bool) -> Self {
        let shell = Command::new("/bin/bash")
            .args(["--noprofile", "--norc", "-c", "read -r value"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            fake,
            shell,
            unknown,
            reject_export,
        }
    }
    fn start_wrapper(&self, record: &Record, args: &[String]) -> Result<Value> {
        self.fake.calls.borrow_mut().push(args.to_vec());
        let issued = Record::load(&record.root)?.request_issued;
        assert!(issued);
        let alias = prepare::alias(record);
        fs::remove_file(&alias)?;
        symlink(env!("CARGO_BIN_EXE_crewmoor"), &alias)?;
        let registry = record.config.feedback_registry.as_ref().unwrap();
        let instance = record.config.feedback_instance.as_ref().unwrap();
        let ready = opencode_watch::watch(&registry.join("instances").join(instance));
        let boundary = args.iter().position(|arg| arg == "--").unwrap();
        let child = Command::new(alias)
            .args(&args[boundary + 1..])
            .env(prepare::LAUNCH_ENV, prepare::launch_file(record))
            .env("CM_OPENCODE_TEST_TRACE", record.root.join("trace.jsonl"))
            .current_dir(&record.config.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        *self.fake.agent.borrow_mut() = Some(child);
        // Reuse the existing OS close-write observation; no Agent-state polling.
        ready.recv_timeout(Duration::from_secs(10))??;
        if self.unknown {
            return Err(Box::new(crewmoor::failure::NativeFailure {
                code: "timeout".into(),
                message: "synthetic admission unknown after wrapper startup".into(),
            }));
        }
        Ok(self.fake.agent_info(record))
    }
    fn export(&self, args: &[String]) -> Result<Value> {
        self.fake.calls.borrow_mut().push(args.to_vec());
        if self.reject_export {
            return Err("synthetic owned-shell export rejected before agent start".into());
        }
        Ok(json!({"type":"pane_input_sent"}))
    }
    fn finish(&self) {
        if let Some(mut child) = self.fake.agent.borrow_mut().take() {
            drop(child.stdin.take());
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
impl Runner for Tui<'_> {
    fn boot(&self, _: &Record) -> Result<process::Identity> {
        Err("existing synthetic host must not boot".into())
    }
    fn call(&self, record: &Record, args: &[String]) -> Result<Value> {
        match (args[0].as_str(), args[1].as_str()) {
            ("agent", "start") => self.start_wrapper(record, args),
            ("pane", "run") => self.export(args),
            ("pane", "process-info") => {
                let mut value = self.fake.call(record, args)?;
                value["process_info"]["shell_pid"] = json!(self.shell.id());
                Ok(value)
            }
            _ => self.fake.call(record, args),
        }
    }
}
impl Drop for Tui<'_> {
    fn drop(&mut self) {
        self.finish();
        drop(self.shell.stdin.take());
        self.shell.wait().unwrap();
    }
}
fn start(setup: &Setup, tui: &Tui<'_>) -> Result<Value> {
    let mut team = setup.team();
    let worker = team
        .roles
        .iter_mut()
        .find(|role| role.role == "worker")
        .unwrap();
    worker.agent = AgentKind::OpenCode;
    worker.agent_version = "opencode v2.2.5".into();
    worker.agent_executable = env!("CARGO_BIN_EXE_opencode-fixture").into();
    worker.agent_args.clear();
    worker.initial_prompt = Some(PROMPT.into());
    team::start(
        team,
        setup.dir.path(),
        &mut no_layout,
        &mut |config, root| {
            if config.agent == AgentKind::OpenCode {
                engine::launch(config, root, tui)
            } else {
                engine::launch(config, root, &setup.fakes[0])
            }
        },
    )
}
fn native_state(setup: &Setup) {
    setup.native_state();
    let path = setup.socket.with_extension("json");
    let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let pane = setup.fakes[1].pane_id();
    state[&pane]["agent"] = json!("opencode");
    state[&pane]["agent_session"] =
        json!({"kind":"id","value":"reported-subtree-B","source":"herdr:opencode"});
    state["processes"][&pane][0]["name"] = json!("opencode");
    fs::write(path, state.to_string()).unwrap();
}
fn start_count(fake: &common::Fake) -> usize {
    fake.calls
        .borrow()
        .iter()
        .filter(|args| args[..2] == ["agent", "start"])
        .count()
}
fn rows(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("trace.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn assert_one_startup(record: &Record) {
    let context: feedback::Context = serde_json::from_slice(
        &fs::read(record.root.parent().unwrap().join("context.json")).unwrap(),
    )
    .unwrap();
    let endpoint =
        endpoint::load(record.config.feedback_registry.as_ref().unwrap(), &context).unwrap();
    let rows = rows(&record.root);
    let posts: Vec<_> = rows.iter().filter(|row| row["method"] == "POST").collect();
    assert_eq!(
        posts.len(),
        1,
        "bind/repeated start must not create a session or submit a prompt"
    );
    assert_eq!(posts[0]["path"], "/api/session");
    let children: Vec<_> = rows.iter().filter(|row| row["method"] == "TUI").collect();
    assert_eq!(children.len(), 1);
    assert_eq!(
        children[0]["body"],
        json!([
            "--server",
            endpoint.url,
            "--session",
            "ses_A",
            "--prompt",
            PROMPT
        ])
    );
}

#[test]
fn opencode_tui_team_start_bind_and_repeated_calls_do_not_replay_initialization() {
    bound_start(false);
}
#[test]
fn opencode_tui_unknown_start_preserves_native_association_without_replay() {
    bound_start(true);
}
fn bound_start(unknown: bool) {
    let setup = Setup::new();
    let tui = Tui::new(&setup.fakes[1], unknown, false);
    start(&setup, &tui).unwrap();
    let record = setup.record("worker");
    assert_eq!(record.accepted, (!unknown).then_some(true));
    assert!(record.process_verified);
    assert_eq!(
        record.request_error.as_deref(),
        unknown.then_some("timeout")
    );
    assert!(!setup.team_dir().join("roles/worker/binding.json").exists());
    native_state(&setup);
    assert!(team::bind(&setup.team_dir(), "worker", &Native).is_err());
    assert!(setup.session_start("lead", AgentKind::Claude, "lead-session", "startup"));
    let bound = team::bind(&setup.team_dir(), "worker", &Native).unwrap();
    assert_eq!(bound["worker_session"], "ses_A");
    assert_repeated_calls(&setup, &tui);
    tui.finish();
    crewmoor::opencode::confirm_exit(&record).unwrap();
    assert_one_startup(&record);
}
fn assert_repeated_calls(setup: &Setup, tui: &Tui<'_>) {
    let path = setup.team_dir().join("roles/worker/binding.json");
    let original = fs::read(&path).unwrap();
    let binding: Binding = serde_json::from_slice(&original).unwrap();
    assert_eq!(
        binding.worker.opencode.as_ref().unwrap().session_id,
        "ses_A"
    );
    assert!(team::bind(&setup.team_dir(), "worker", &Native).is_err());
    assert_eq!(fs::read(path).unwrap(), original);
    assert!(
        start(setup, tui)
            .unwrap_err()
            .to_string()
            .contains("team directory exists")
    );
    assert_eq!(start_count(&setup.fakes[0]), 1);
    assert_eq!(start_count(&setup.fakes[1]), 1);
}
#[test]
fn opencode_tui_partial_team_failure_before_submit_never_launches_or_replays() {
    let setup = Setup::new();
    let tui = Tui::new(&setup.fakes[1], false, true);
    let error = start(&setup, &tui).unwrap_err().to_string();
    assert!(error.contains("role worker start failed"));
    assert!(error.contains("export rejected before agent start"));
    let record = setup.record("worker");
    assert!(!record.request_issued);
    assert_eq!(record.phase, "unknown-outcome");
    assert_eq!(record.config.initial_prompt.as_deref(), Some(PROMPT));
    native_state(&setup);
    assert!(setup.session_start("lead", AgentKind::Claude, "lead-session", "startup"));
    assert!(team::bind(&setup.team_dir(), "worker", &Native).is_err());
    assert!(team::bind(&setup.team_dir(), "worker", &Native).is_err());
    assert!(
        start(&setup, &tui)
            .unwrap_err()
            .to_string()
            .contains("team directory exists")
    );
    assert!(!record.root.join("trace.jsonl").exists());
    assert!(!setup.team_dir().join("roles/worker/binding.json").exists());
    assert_eq!(start_count(&setup.fakes[0]), 1);
    assert_eq!(start_count(&setup.fakes[1]), 0);
}
