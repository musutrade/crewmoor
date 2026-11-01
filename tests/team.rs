#[path = "support/common.rs"]
mod common;
use crewmoor::{
    config::{self, AgentKind, Host, Runtime},
    engine,
    feedback::{self, INSTANCE_ENV},
    message::{self, Binding, Message, Native},
    native::strings,
    record::Record,
    team,
};
use serde_json::{Value, json};
use std::{fs, os::unix::net::UnixListener};

const FIXTURE: &str = env!("CARGO_BIN_EXE_transport-fixture");

struct Setup {
    dir: tempfile::TempDir,
    _listener: UnixListener,
    socket: std::path::PathBuf,
    fakes: Vec<common::Fake>,
}
impl Setup {
    fn new() -> Self {
        let dir = common::directory();
        let socket = dir.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let fakes = (0..2)
            .map(|index| {
                let mut fake = common::Fake::default();
                fake.index = index;
                fake
            })
            .collect();
        Self {
            _listener: listener,
            socket,
            fakes,
            dir,
        }
    }
    fn base(&self) -> std::path::PathBuf {
        self.dir.path().canonicalize().unwrap()
    }
    fn team(&self) -> team::Team {
        let text = format!(
            r#"team = "smoke"
task = "issue-15"
runtime = "tui"
runtime_binary = "{FIXTURE}"
runtime_version = "herdr 0.9.5"
feedback_registry = "{base}/registry"
evidence = "synthetic"
dispatcher = "lead"
[host]
socket = "{socket}"
server_version = "0.9.4"
[[roles]]
role = "worker"
cwd = "{base}"
agent = "codex"
agent_executable = "{FIXTURE}"
agent_version = "fixture 1"
agent_args = ["--no-alt-screen"]
initial_prompt = "Read the task file and report once."
[[roles]]
role = "lead"
cwd = "{base}"
agent = "claude"
agent_executable = "{FIXTURE}"
agent_version = "fixture 1"
"#,
            base = self.base().display(),
            socket = self.socket.display(),
        );
        let path = self.dir.path().join("team.toml");
        fs::write(&path, text).unwrap();
        team::read(&path).unwrap()
    }
    fn start(&self) -> crewmoor::Result<Value> {
        let mut next = 0;
        team::start(self.team(), self.dir.path(), &mut |cfg, root| {
            let fake = &self.fakes[next];
            next += 1;
            engine::launch(cfg, root, fake)
        })
    }
    fn team_dir(&self) -> std::path::PathBuf {
        self.dir.path().join("teams/smoke")
    }
    /// Roles start dispatcher first: lead uses fake 0, worker fake 1.
    fn fake(&self, record: &Record) -> &common::Fake {
        &self.fakes[usize::from(record.config.role == "worker")]
    }
    fn stop(&self) -> crewmoor::Result<Value> {
        team::stop(&self.team_dir(), &mut |root| {
            let mut record = Record::load(root)?;
            let _lock = record.lock()?;
            let fake = self.fake(&record);
            engine::stop(&mut record, fake)?;
            Ok(record)
        })
    }
    fn instance(&self, role: &str) -> String {
        let path = self.team_dir().join(format!("roles/{role}/context.json"));
        let context: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        context["instance"].as_str().unwrap().into()
    }
    fn session_start(&self, role: &str, kind: AgentKind, session: &str, source: &str) -> bool {
        let input = json!({"session_id":session,"cwd":self.base(),
            "hook_event_name":"SessionStart","source":source});
        feedback::ingest(
            &self.base().join("registry"),
            &self.instance(role),
            &kind,
            &serde_json::to_vec(&input).unwrap(),
        )
        .is_ok()
    }
    fn record(&self, role: &str) -> Record {
        Record::load(&team::launch_root(&self.team_dir(), role)).unwrap()
    }
    /// Herdr CLI fixture state: Claude has a container session, idle Codex has none.
    fn native_state(&self) {
        let mut state = json!({"processes":{}});
        for (role, kind) in [("lead", "claude"), ("worker", "codex")] {
            let record = self.record(role);
            let fake = self.fake(&record);
            let pane = fake.pane_id();
            let mut agent = json!({"agent":kind,"pane_id":pane,
                "terminal_id":fake.terminal_id(),"workspace_id":fake.workspace_id()});
            if kind == "claude" {
                agent["agent_session"] = json!({"kind":"id","value":"lead-session"});
            }
            state[&pane] = agent;
            state["processes"][&pane] = json!([{"name":kind,"pid":common::child_pid(&fake.agent)}]);
        }
        fs::write(self.socket.with_extension("json"), state.to_string()).unwrap();
    }
}

#[test]
fn team_start_injects_markers_creates_directory_once_and_reports_absolute_path() {
    let setup = Setup::new();
    let started = setup.start().unwrap();
    let dir = setup.team_dir();
    assert_eq!(started["team_dir"], json!(dir));
    assert_eq!(started["roles"][0]["role"], "lead");
    assert_eq!(started["roles"][1]["role"], "worker");
    let snapshot: Value =
        serde_json::from_slice(&fs::read(dir.join("team.json")).unwrap()).unwrap();
    assert_eq!(snapshot["runtime"], "tui");
    for (index, role) in ["lead", "worker"].into_iter().enumerate() {
        let fake = &setup.fakes[index];
        let marker = format!("{INSTANCE_ENV}={}", setup.instance(role));
        let calls = fake.calls.borrow();
        let create = calls
            .iter()
            .find(|c| c[..2] == strings(&["workspace", "create"]));
        let create = create.unwrap();
        assert_eq!(create[create.len() - 2..], [String::from("--env"), marker]);
        assert!(!calls.iter().any(|c| c[0] == "session"));
        let record = setup.record(role);
        assert!(!record.server_owned);
        assert_eq!(record.binding_scope, "workspace");
        assert_eq!(record.server, None);
        assert_eq!(
            record.container_id,
            Some(setup.socket.display().to_string())
        );
    }
    let start = setup.fakes[1].calls.borrow();
    let start = start.iter().find(|c| c[0] == "agent").unwrap();
    assert_eq!(start.last().unwrap(), "Read the task file and report once.");
    let lead = setup.fakes[0].calls.borrow();
    assert_eq!(
        lead.iter()
            .find(|c| c[0] == "agent")
            .unwrap()
            .last()
            .unwrap(),
        "--"
    );
    let again = setup.start().unwrap_err().to_string();
    assert!(again.contains("team directory exists"));
}

#[test]
fn team_bind_needs_registered_sessions_then_binds_launched_resources_and_messages() {
    let setup = Setup::new();
    setup.start().unwrap();
    setup.native_state();
    let dir = setup.team_dir();
    let early = team::bind(&dir, "worker", &Native).unwrap_err().to_string();
    assert!(early.contains("not registered"));
    assert!(!dir.join("roles/worker/binding.json").exists());
    assert!(team::bind(&dir, "lead", &Native).is_err());
    assert!(team::bind(&dir, "nobody", &Native).is_err());
    assert!(setup.session_start("lead", AgentKind::Claude, "lead-session", "startup"));
    assert!(setup.session_start("worker", AgentKind::Codex, "worker-session", "startup"));
    let bound = team::bind(&dir, "worker", &Native).unwrap();
    assert_eq!(bound["worker_session"], "worker-session");
    assert!(team::bind(&dir, "worker", &Native).is_err());
    let binding: Binding =
        serde_json::from_slice(&fs::read(dir.join("roles/worker/binding.json")).unwrap()).unwrap();
    assert!(binding.worker.process.is_some());
    let report = Message {
        id: "report-1".into(),
        from: "worker".into(),
        to: "lead".into(),
        reply_to: None,
        content: "synthetic".into(),
    };
    assert_eq!(
        message::send(&binding, &report, &Native).unwrap().transport,
        "accepted"
    );
    // A changed pane process is refused even though Codex has no container session.
    let mut moved = binding;
    let mut process = moved.worker.process.take().unwrap();
    process.pid = std::process::id();
    moved.worker.process = Some(process);
    let error = message::send(&moved, &report, &Native)
        .unwrap_err()
        .to_string();
    assert!(error.contains("process"));
}

#[test]
fn clear_after_binding_is_rejected_recorded_and_never_adopted() {
    let setup = Setup::new();
    setup.start().unwrap();
    setup.native_state();
    assert!(setup.session_start("lead", AgentKind::Claude, "lead-session", "startup"));
    assert!(setup.session_start("worker", AgentKind::Codex, "worker-session", "startup"));
    team::bind(&setup.team_dir(), "worker", &Native).unwrap();
    assert!(!setup.session_start("worker", AgentKind::Codex, "after-clear", "clear"));
    let registry = setup.base().join("registry");
    let view = feedback::show(&registry, &setup.instance("worker")).unwrap();
    let rejected = &view["latest_session_rejection"];
    assert_eq!(rejected["native_session_id"], "after-clear");
    assert_eq!(rejected["native"]["source"], "clear");
    assert_eq!(rejected["native"]["associated_session"], "worker-session");
    assert_eq!(view["agent_state"], "unknown");
    let path = setup.team_dir().join("roles/worker/context.json");
    let context: feedback::Context = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let session = feedback::registered_session(&registry, &context).unwrap();
    assert_eq!(session.as_deref(), Some("worker-session"));
    let path = setup.team_dir().join("roles/worker/binding.json");
    let binding: Binding = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let report = feedback::report_to(&binding, "after-clear", "agent", "progress", "x", &Native);
    assert!(report.is_err());
    assert!(!setup.socket.with_extension("submitted").exists());
    let view = feedback::show(&registry, &setup.instance("worker")).unwrap();
    assert_eq!(
        view["latest_session_rejection"]["native"]["from"],
        "agent-report"
    );
    assert_eq!(view["latest_business_report"], Value::Null);
}

#[test]
fn team_stop_closes_only_owned_panes_and_workspaces_and_never_the_host() {
    let setup = Setup::new();
    setup.start().unwrap();
    *setup.fakes[1].extra.borrow_mut() = true;
    let stopped = setup.stop().unwrap();
    assert_eq!(
        stopped["roles"][0]["result"],
        "target-stopped-host-retained"
    );
    for fake in &setup.fakes {
        assert!(*fake.closed.borrow());
        assert!(*fake.workspace_closed.borrow());
        assert!(!*fake.stopped.borrow());
        assert!(!fake.calls.borrow().iter().any(|c| c[0] == "session"));
    }
    assert!(setup.socket.exists());
    for role in ["lead", "worker"] {
        assert!(setup.record(role).binding().is_err());
    }
    let again = setup.stop().unwrap();
    assert_eq!(again["roles"][1]["result"], "target-stopped-host-retained");
}

#[test]
fn failed_role_start_and_stop_keep_the_team_directory_as_evidence() {
    let setup = Setup::new();
    *setup.fakes[1].fail.borrow_mut() = true;
    let error = setup.start().unwrap_err().to_string();
    assert!(error.contains("role worker start failed"));
    let dir = setup.team_dir();
    assert!(dir.join("roles/worker/context.json").exists());
    *setup.fakes[0].fail.borrow_mut() = true;
    let error = setup.stop().unwrap_err().to_string();
    assert!(error.contains("team stop incomplete"));
    assert!(dir.join("team.json").exists());
}

#[test]
fn team_configuration_declares_runtime_once_and_needs_an_existing_tui_host() {
    let setup = Setup::new();
    let path = setup.dir.path().join("bad.toml");
    setup.team();
    let text = fs::read_to_string(setup.dir.path().join("team.toml")).unwrap();
    let per_role = text.replace(
        "agent = \"claude\"",
        "agent = \"claude\"\nruntime = \"core\"",
    );
    fs::write(&path, per_role).unwrap();
    assert!(team::read(&path).is_err());
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/team-tui.toml");
    team::read(&example).unwrap().validate().unwrap();
    let mut team = setup.team();
    team.host = None;
    assert!(team.validate().is_err());
    let mut team = setup.team();
    team.runtime = Runtime::Core;
    assert!(team.validate().is_err());
    let mut team = setup.team();
    team.roles.truncate(1);
    assert!(team.validate().is_err());
    let mut team = setup.team();
    team.dispatcher = "absent".into();
    assert!(team.validate().is_err());
    let missing = setup.dir.path().join("missing").display().to_string();
    for args in [
        vec!["team"],
        vec!["team", "other"],
        vec!["team", "stop"],
        vec!["team", "bind", "/x"],
        vec!["team", "start", "/x"],
        vec!["team", "bind", &missing, "worker"],
        vec!["team", "stop", &missing],
        vec!["team", "start", &missing, &missing],
    ] {
        assert!(!crewmoor_cli(&args).status.success(), "{args:?}");
    }
    // A team directory whose roles never launched stops cleanly without native calls.
    let dir = setup.dir.path().join("teams/never");
    fs::create_dir_all(dir.join("roles")).unwrap();
    fs::write(
        dir.join("team.json"),
        serde_json::to_vec(&setup.team()).unwrap(),
    )
    .unwrap();
    let output = crewmoor_cli(&["team", "stop", dir.to_str().unwrap()]);
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["roles"][0]["result"], "not-launched");
}
fn crewmoor_cli(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn host_mode_never_stops_the_server_while_private_tui_is_unchanged() {
    let setup = Setup::new();
    let (dir, mut cfg, fake) = common::setup(Runtime::Tui);
    cfg.host = Some(Host {
        socket: setup.socket.clone(),
        server_version: "0.9.4".into(),
    });
    let root = dir
        .path()
        .join("a-long-state-root-that-would-not-fit-a-private-socket");
    let mut r = engine::launch(cfg.clone(), &root, &fake).unwrap();
    assert_eq!(r.session, "host");
    engine::stop(&mut r, &fake).unwrap();
    assert_eq!(r.phase, "target-stopped-host-retained");
    assert!(!*fake.stopped.borrow());
    let mut wrong = cfg.clone();
    wrong.host = Some(Host {
        socket: dir.path().join("other.sock"),
        server_version: "0.9.4".into(),
    });
    let other = UnixListener::bind(dir.path().join("other.sock")).unwrap();
    let fake = common::Fake::default();
    *fake.reported_socket.borrow_mut() = Some(setup.socket.clone());
    let error = engine::launch(wrong, &dir.path().join("w"), &fake).unwrap_err();
    assert!(error.to_string().contains("socket"));
    drop(other);
    let mut core = cfg;
    core.runtime = Runtime::Core;
    assert!(core.validate().is_err());
    let (_dir, r, fake) = common::launch(Runtime::Tui);
    assert!(r.server_owned);
    assert!(!fake.calls.borrow().iter().flatten().any(|a| a == "--env"));
}

#[test]
fn codex_accepts_one_initial_prompt_and_other_arguments_stay_forbidden() {
    let (_dir, mut cfg, _fake) = common::setup(Runtime::Tui);
    cfg.initial_prompt = Some("one task".into());
    cfg.validate().unwrap();
    assert_eq!(cfg.argv().last().unwrap(), "one task");
    for bad in ["", "  ", "--yolo", "line\nbreak"] {
        cfg.initial_prompt = Some(bad.into());
        assert!(cfg.validate().is_err(), "{bad:?}");
    }
    cfg.initial_prompt = Some("x".repeat(4097));
    assert!(cfg.validate().is_err());
    cfg.initial_prompt = None;
    cfg.agent_args = strings(&["first task"]);
    assert!(cfg.validate().is_err());
    cfg.agent = AgentKind::Claude;
    cfg.agent_args = Vec::new();
    cfg.initial_prompt = Some("one task".into());
    assert!(cfg.validate().is_err());
    assert!(config::validate_args(&AgentKind::Claude, &strings(&["--model", "m"])).is_ok());
    assert!(config::validate_args(&AgentKind::Claude, &strings(&["--no-daemon"])).is_err());
}

#[test]
fn core_markers_need_an_app_target_and_travel_in_the_terminal_env_field() {
    let (dir, mut cfg, fake) = common::setup(Runtime::Core);
    cfg.feedback_instance = Some(uuid());
    assert!(cfg.validate().is_err());
    cfg.app = Some(crewmoor::app::Binding {
        session: "app-stable".into(),
        socket: dir.path().join("existing.sock"),
        core_id: "own-core".into(),
        machine: "user-machine".into(),
        space_id: Some("user-machine/sp-existing".into()),
        tab_id: Some("tb-existing".into()),
    });
    cfg.validate().unwrap();
    cfg.feedback_instance = Some("not-a-uuid".into());
    assert!(cfg.validate().is_err());
    let instance = uuid();
    cfg.feedback_instance = Some(instance.clone());
    let r = Record::prepare(cfg, &dir.path().join("i")).unwrap();
    let args = crewmoor::core::launch_args(&r).unwrap();
    let (_, params) = crewmoor::app::request(&r, &args).unwrap();
    assert_eq!(params["env"], json!({INSTANCE_ENV: instance}));
    drop(fake);
}
fn uuid() -> String {
    "0123456789abcdef0123456789abcdef".into()
}
