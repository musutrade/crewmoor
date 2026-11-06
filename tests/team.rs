#[path = "support/common.rs"]
mod common;
#[path = "support/opencode_watch.rs"]
mod opencode_watch;
#[path = "support/team_core_checks.rs"]
mod team_core_checks;
#[path = "support/team_opencode_checks.rs"]
mod team_opencode_checks;
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
initial_prompt = "Claude 初始化：\"原样\""
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
        team::start(
            self.team(),
            self.dir.path(),
            &mut no_layout,
            &mut |cfg, root| {
                let fake = &self.fakes[next];
                next += 1;
                engine::launch(cfg, root, fake)
            },
        )
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
        "Claude 初始化：\"原样\""
    );
    let again = setup.start().unwrap_err().to_string();
    assert!(again.contains("team directory exists"));
    for fake in &setup.fakes {
        assert_eq!(
            fake.calls
                .borrow()
                .iter()
                .filter(|call| call[0] == "agent")
                .count(),
            1
        );
    }
    assert_eq!(
        snapshot["roles"][1]["initial_prompt"],
        "Claude 初始化：\"原样\""
    );
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
    for fake in &setup.fakes {
        assert_eq!(
            fake.calls
                .borrow()
                .iter()
                .filter(|call| call[0] == "agent")
                .count(),
            1
        );
    }
    let binding: Binding =
        serde_json::from_slice(&fs::read(dir.join("roles/worker/binding.json")).unwrap()).unwrap();
    assert!(binding.worker.process.is_some());
    // Only the worker references this binding for Hook forwarding.
    let registry = setup.base().join("registry");
    let reference = feedback::load_binding_ref(&registry, &setup.instance("worker"))
        .unwrap()
        .expect("worker binding reference written on bind");
    assert_eq!(reference.binding, dir.join("roles/worker/binding.json"));
    assert!(
        !registry
            .join("instances")
            .join(setup.instance("lead"))
            .join("dispatcher-binding-ref.json")
            .exists()
    );
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
    let calls_before: Vec<_> = setup.fakes.iter().map(|f| f.calls.borrow().len()).collect();
    assert!(
        setup
            .start()
            .unwrap_err()
            .to_string()
            .contains("team directory exists")
    );
    assert_eq!(
        calls_before,
        setup
            .fakes
            .iter()
            .map(|f| f.calls.borrow().len())
            .collect::<Vec<_>>()
    );
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
    // Core needs app_target instead of host; mixing either way is refused.
    let mut team = setup.team();
    team.runtime = Runtime::Core;
    assert!(team.validate().is_err());
    team.app_target = Some(setup.dir.path().join("app-target.toml"));
    assert!(team.validate().is_err());
    team.host = None;
    team.validate().unwrap();
    team.app_target = Some("relative/app-target.toml".into());
    assert!(team.validate().is_err());
    let mut team = setup.team();
    team.app_target = Some(setup.dir.path().join("app-target.toml"));
    assert!(team.validate().is_err());
    let mut team = setup.team();
    team.roles[0].tab_id = Some("tb-worker".into());
    let error = team.validate().unwrap_err().to_string();
    assert!(error.contains("tab_id needs runtime=core"));
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
/// TUI teams have no App layout; reading one is a test failure.
fn no_layout(_: &crewmoor::app::Binding) -> crewmoor::Result<Value> {
    Err("TUI team must not read an App layout".into())
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
fn all_agents_accept_the_same_initial_prompt_limits_and_keep_argument_whitelists() {
    let (_dir, mut cfg, _fake) = common::setup(Runtime::Tui);
    for agent in [AgentKind::Codex, AgentKind::Claude, AgentKind::OpenCode] {
        cfg.agent = agent;
        for prompt in [
            "初始化：\"中文\" '引号' \\路径 $(literal) `literal`".into(),
            "x".repeat(4096),
            format!("{}x", "中".repeat(1365)),
        ] {
            cfg.initial_prompt = Some(prompt.clone());
            cfg.validate().unwrap();
            assert_eq!(cfg.argv().last(), Some(&prompt));
        }
        for bad in [
            "".into(),
            "  ".into(),
            "\u{3000}".into(),
            "--yolo".into(),
            "line\nbreak".into(),
            "a\rb".into(),
            "a\tb".into(),
            "a\0b".into(),
            "a\u{7f}b".into(),
            "a\u{85}b".into(),
            "x".repeat(4097),
            format!("{}xy", "中".repeat(1365)),
            "中".repeat(1366),
        ] {
            cfg.initial_prompt = Some(bad.clone());
            assert!(cfg.validate().is_err(), "{:?}: {bad:?}", cfg.agent);
        }
        cfg.initial_prompt = None;
        cfg.validate().unwrap();
        assert!(!cfg.argv().contains(&"--prompt".into()));
        assert!(config::validate_args(&cfg.agent, &strings(&["first task"])).is_err());
        assert!(config::validate_args(&cfg.agent, &strings(&["--prompt", "task"])).is_err());
        assert!(config::validate_args(&cfg.agent, &strings(&["-p", "task"])).is_err());
    }
    assert!(config::validate_args(&AgentKind::Claude, &strings(&["--model", "m"])).is_ok());
    assert!(config::validate_args(&AgentKind::Claude, &strings(&["--no-daemon"])).is_err());
    assert!(
        config::validate_args(
            &AgentKind::Claude,
            &strings(&["--dangerously-skip-permissions"])
        )
        .is_ok()
    );
    assert!(
        config::validate_args(
            &AgentKind::Claude,
            &strings(&["--settings", "{\"key\":\"val\"}"])
        )
        .is_ok()
    );
    assert!(config::validate_args(&AgentKind::Claude, &strings(&["--settings"])).is_err());
    assert!(
        config::validate_args(&AgentKind::Claude, &strings(&["--add-dir", "/tmp/batch"])).is_ok()
    );
    assert!(
        config::validate_args(
            &AgentKind::Claude,
            &strings(&["--add-dir", "relative/path"])
        )
        .is_err()
    );
}

#[test]
fn native_startup_argv_preserves_optional_text_for_all_agents_and_runtimes() {
    for runtime in [Runtime::Tui, Runtime::Core] {
        for agent in [AgentKind::Codex, AgentKind::Claude, AgentKind::OpenCode] {
            for prompt in [None, Some("初始化：\"中文\" '引号' \\路径 $(literal)")] {
                check_startup_argv(runtime.clone(), agent.clone(), prompt);
            }
        }
    }
}
fn check_startup_argv(runtime: Runtime, agent: AgentKind, prompt: Option<&str>) {
    let (dir, mut cfg, _fake) = common::setup(runtime.clone());
    cfg.agent = agent.clone();
    cfg.initial_prompt = prompt.map(String::from);
    let mut record = Record::prepare(cfg, &dir.path().join("i")).unwrap();
    record.handle = Some("returned-pane".into());
    let args = match runtime {
        Runtime::Tui => crewmoor::tui::launch_args(&record).unwrap(),
        Runtime::Core => crewmoor::core::launch_args(&record).unwrap(),
    };
    let boundary = args.iter().position(|arg| arg == "--").unwrap();
    let launched = &args[boundary + 1..];
    let expected = expected_startup_argv(&record, prompt);
    if runtime == Runtime::Core && agent == AgentKind::OpenCode {
        assert!(launched.ends_with(&expected));
    } else {
        assert_eq!(launched, expected);
    }
    let inner = record.config.argv();
    let prompt_flags = usize::from(agent == AgentKind::OpenCode && prompt.is_some());
    assert_eq!(
        inner.iter().filter(|arg| *arg == "--prompt").count(),
        prompt_flags
    );
    if let Some(prompt) = prompt {
        assert_eq!(inner.iter().filter(|arg| arg.as_str() == prompt).count(), 1);
    }
}
fn expected_startup_argv(record: &Record, prompt: Option<&str>) -> Vec<String> {
    let mut expected = Vec::new();
    match (&record.config.runtime, &record.config.agent) {
        (Runtime::Tui, AgentKind::OpenCode) => expected.push("--standalone".into()),
        (Runtime::Core, AgentKind::OpenCode) => {
            expected.push(
                crewmoor::opencode::prepare::alias(record)
                    .display()
                    .to_string(),
            );
            if let Some(prompt) = prompt {
                expected.extend(["--prompt".into(), prompt.into()]);
            }
        }
        (Runtime::Core, agent) => {
            expected.push(FIXTURE.into());
            if *agent == AgentKind::Codex {
                expected.push("--no-daemon".into());
            }
            expected.extend(prompt.map(String::from));
        }
        _ => expected.extend(prompt.map(String::from)),
    }
    expected
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
#[test]
fn role_names_are_checked_before_any_write_and_again_when_loading() {
    let setup = Setup::new();
    let outside = setup.base().join("outside-team");
    for bad in [
        outside.display().to_string(),
        "..".into(),
        "a/b".into(),
        "Lead".into(),
    ] {
        let mut cfg = setup.team();
        cfg.roles[1].role = bad.clone();
        cfg.dispatcher = bad.clone();
        let result = team::start(cfg, setup.dir.path(), &mut no_layout, &mut |_, _| {
            Err("an invalid role must never reach launch".into())
        });
        assert!(result.is_err(), "{bad}");
        assert!(!setup.dir.path().join("teams").exists(), "{bad}");
        assert!(!outside.exists());
    }
    // A tampered snapshot is refused by bind and stop before any role path is used.
    let mut team = serde_json::to_value(setup.team()).unwrap();
    team["roles"][0]["role"] = json!(outside);
    let dir = setup.dir.path().join("teams/tampered");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("team.json"), team.to_string()).unwrap();
    assert!(team::bind(&dir, "lead", &Native).is_err());
    let mut visited = Vec::new();
    let stopped = team::stop(&dir, &mut |root| {
        visited.push(root.to_path_buf());
        Record::load(root)
    });
    assert!(stopped.is_err());
    assert!(visited.is_empty());
}
#[test]
fn hook_session_change_halts_messages_without_a_container_session() {
    let setup = Setup::new();
    setup.start().unwrap();
    setup.native_state();
    assert!(setup.session_start("lead", AgentKind::Claude, "lead-session", "startup"));
    assert!(setup.session_start("worker", AgentKind::Codex, "worker-session", "startup"));
    team::bind(&setup.team_dir(), "worker", &Native).unwrap();
    let path = setup.team_dir().join("roles/worker/binding.json");
    let saved = fs::read(&path).unwrap();
    let binding: Binding = serde_json::from_slice(&saved).unwrap();
    let request = Message {
        id: "after-clear".into(),
        from: "lead".into(),
        to: "worker".into(),
        reply_to: None,
        content: "must not reach the new session".into(),
    };
    // A rejected explicit report alone is not a Hook session fact.
    let report = feedback::report_to(&binding, "other", "agent", "progress", "x", &Native);
    assert!(report.is_err());
    assert!(message::send(&binding, &request, &Native).is_ok());
    fs::remove_file(setup.socket.with_extension("submitted")).unwrap();
    // Same Codex process, no container session: the Hook's rejection fact halts routing.
    assert!(!setup.session_start("worker", AgentKind::Codex, "after-clear", "clear"));
    let error = message::send(&binding, &request, &Native)
        .unwrap_err()
        .to_string();
    assert!(error.contains("team halted"));
    assert!(!setup.socket.with_extension("submitted").exists());
    let reverse = Message {
        from: "worker".into(),
        to: "lead".into(),
        ..request
    };
    assert!(message::send(&binding, &reverse, &Native).is_err());
    assert_eq!(fs::read(&path).unwrap(), saved);
    let registry = setup.base().join("registry");
    let path = setup.team_dir().join("roles/worker/context.json");
    let context: feedback::Context = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let session = feedback::registered_session(&registry, &context).unwrap();
    assert_eq!(session.as_deref(), Some("worker-session"));
}
#[test]
fn a_new_binding_is_refused_once_the_hook_reported_a_changed_session() {
    let setup = Setup::new();
    setup.start().unwrap();
    setup.native_state();
    assert!(setup.session_start("lead", AgentKind::Claude, "lead-session", "startup"));
    assert!(setup.session_start("worker", AgentKind::Codex, "worker-session", "startup"));
    assert!(!setup.session_start("worker", AgentKind::Codex, "after-clear", "clear"));
    let error = team::bind(&setup.team_dir(), "worker", &Native).unwrap_err();
    assert!(error.to_string().contains("team halted"));
    assert!(!setup.team_dir().join("roles/worker/binding.json").exists());
}
/// Bound team whose worker has no container session; returns the worker's journal root.
fn bound(setup: &Setup) -> (Binding, std::path::PathBuf) {
    setup.start().unwrap();
    setup.native_state();
    assert!(setup.session_start("lead", AgentKind::Claude, "lead-session", "startup"));
    assert!(setup.session_start("worker", AgentKind::Codex, "worker-session", "startup"));
    team::bind(&setup.team_dir(), "worker", &Native).unwrap();
    let path = setup.team_dir().join("roles/worker/binding.json");
    let binding = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let root = setup.base().join("registry/instances");
    (binding, root.join(setup.instance("worker")))
}
fn request(id: &str) -> Message {
    Message {
        id: id.into(),
        from: "lead".into(),
        to: "worker".into(),
        reply_to: None,
        content: "same registered session".into(),
    }
}
#[test]
fn the_session_check_reads_while_a_hook_append_holds_the_journal_lock() {
    let setup = Setup::new();
    let (binding, root) = bound(&setup);
    let held = crewmoor::record::Lock::take(&root).unwrap();
    message::send(&binding, &request("during-hook"), &Native).unwrap();
    drop(held);
    assert!(!root.join("operation.lock").exists());
}
#[test]
fn a_hook_append_during_the_session_check_is_recorded_and_seen() {
    use std::process::Command;
    let setup = Setup::new();
    let (binding, root) = bound(&setup);
    let journal = root.join("events.jsonl");
    let saved = fs::read(&journal).unwrap();
    fs::remove_file(&journal).unwrap();
    // A FIFO holds the check inside its journal read until the Hook's append arrives.
    assert!(
        Command::new("mkfifo")
            .arg(&journal)
            .status()
            .unwrap()
            .success()
    );
    let registry = setup.base().join("registry");
    let path = setup.team_dir().join("roles/worker/context.json");
    let context: feedback::Context = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let reader = std::thread::spawn(move || {
        feedback::require_unchanged(&registry, &context).map_err(|e| e.to_string())
    });
    let input = json!({"session_id":"after-clear","cwd":setup.base(),
        "hook_event_name":"SessionStart","source":"clear"});
    let registry = setup.base().join("registry");
    let bytes = serde_json::to_vec(&input).unwrap();
    let hook = feedback::ingest(
        &registry,
        &setup.instance("worker"),
        &AgentKind::Codex,
        &bytes,
    );
    let hook = hook.unwrap_err().to_string();
    if !hook.contains("rejection recorded") {
        // The Hook was refused before writing: release the waiting reader, then fail.
        drop(fs::OpenOptions::new().write(true).open(&journal).unwrap());
    }
    let checked = reader.join().unwrap();
    assert!(hook.contains("rejection recorded"), "{hook}");
    assert!(checked.unwrap_err().contains("team halted"));
    fs::remove_file(&journal).unwrap();
    fs::write(&journal, saved).unwrap();
    assert!(!setup.session_start("worker", AgentKind::Codex, "after-clear", "clear"));
    let error = message::send(&binding, &request("after-clear"), &Native).unwrap_err();
    assert!(error.to_string().contains("team halted"));
}
#[test]
fn the_session_check_skips_an_unfinished_last_line_but_not_corrupt_records() {
    use std::io::Write;
    let setup = Setup::new();
    let (_binding, root) = bound(&setup);
    let registry = setup.base().join("registry");
    let path = setup.team_dir().join("roles/worker/context.json");
    let context: feedback::Context = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let journal = root.join("events.jsonl");
    let append = |bytes: &[u8]| {
        let mut file = fs::OpenOptions::new().append(true).open(&journal).unwrap();
        file.write_all(bytes).unwrap();
    };
    append(br#"{"received_ms":1,"source":"session-rejec"#);
    feedback::require_unchanged(&registry, &context).unwrap();
    append(
        b"ted\",\"event\":\"SessionStart\",\"native_session_id\":\"x\",\
\"native\":{\"from\":\"native-hook\"},\"summary\":null}\n{\"partial\":",
    );
    let error = feedback::require_unchanged(&registry, &context).unwrap_err();
    assert!(error.to_string().contains("team halted"));
    append(b"1}\nnot json\n");
    let error = feedback::require_unchanged(&registry, &context).unwrap_err();
    assert!(!error.to_string().contains("team halted"));
    fs::remove_file(&journal).unwrap();
    fs::create_dir(&journal).unwrap();
    assert!(feedback::require_unchanged(&registry, &context).is_err());
}
#[test]
fn the_hook_halt_is_reported_before_a_changed_container_session() {
    let setup = Setup::new();
    let (binding, _root) = bound(&setup);
    assert!(!setup.session_start("worker", AgentKind::Codex, "after-clear", "clear"));
    // Herdr reports the new session too, as it does after a real /clear.
    let path = setup.socket.with_extension("json");
    let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let pane = setup.fakes[1].pane_id();
    state[&pane]["agent_session"] = json!({"kind":"id","value":"after-clear"});
    fs::write(&path, state.to_string()).unwrap();
    let error = message::send(&binding, &request("after-clear"), &Native).unwrap_err();
    let error = error.to_string();
    assert!(error.contains("team halted"), "{error}");
    assert!(!error.contains("native session changed"), "{error}");
    assert!(!setup.socket.with_extension("submitted").exists());
}
