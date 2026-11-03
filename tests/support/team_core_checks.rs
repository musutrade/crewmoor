//! #16 synthetic core team: start/bind/stop against an existing App core target.
use super::common;
use crewmoor::{
    config::AgentKind,
    engine,
    feedback::{self, INSTANCE_ENV},
    message::{self, Binding, Message, Transport},
    native::Runner,
    process,
    record::Record,
    team,
};
use serde_json::{Value, json};
use std::{cell::RefCell, fs, io::Read, os::unix::net::UnixListener, path::PathBuf};

const FIXTURE: &str = env!("CARGO_BIN_EXE_transport-fixture");

/// The existing App core: one socket, one clean server process per role fake.
struct Host<'a>(&'a common::Fake);
impl Runner for Host<'_> {
    fn call(&self, r: &Record, args: &[String]) -> crewmoor::Result<Value> {
        self.0.call(r, args)
    }
    fn boot(&self, _r: &Record) -> crewmoor::Result<process::Identity> {
        process::identity(common::child_pid(&self.0.child).ok_or("missing host")?)
    }
}
struct Setup {
    dir: tempfile::TempDir,
    _listener: UnixListener,
    socket: PathBuf,
    fakes: Vec<common::Fake>,
    layout_reads: std::cell::Cell<usize>,
}
fn clean_host() -> std::process::Child {
    let mut command = std::process::Command::new(FIXTURE);
    for marker in crewmoor::app::CHILD_MARKERS {
        command.env_remove(marker);
    }
    let mut child = command
        .arg("hold")
        .env("CREWMOOR_TRANSPORT_HANDSHAKE", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut byte = [0];
    child.stdout.take().unwrap().read_exact(&mut byte).unwrap();
    assert_eq!(&byte, b"R");
    child
}
impl Setup {
    fn new() -> Self {
        let dir = common::directory();
        let socket = dir.path().join("existing.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let fakes = (0..2)
            .map(|index| {
                let mut fake = common::Fake::default();
                fake.index = index;
                *fake.child.borrow_mut() = Some(clean_host());
                fake
            })
            .collect();
        let target = format!(
            "session = \"app-stable\"\nsocket = \"{}\"\ncore_id = \"own-core\"\n\
             machine = \"user-machine\"\nspace_id = \"user-machine/sp-existing\"\n\
             tab_id = \"tb-existing\"\n",
            socket.display()
        );
        fs::write(dir.path().join("app-target.toml"), target).unwrap();
        Self {
            _listener: listener,
            socket,
            fakes,
            dir,
            layout_reads: std::cell::Cell::new(0),
        }
    }
    fn base(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }
    fn team(&self) -> team::Team {
        let text = format!(
            r#"team = "core"
task = "issue-16"
runtime = "core"
runtime_binary = "/no-cli-spawn-allowed"
runtime_version = "hc 0.1.15"
app_target = "{base}/app-target.toml"
feedback_registry = "{base}/registry"
evidence = "synthetic"
dispatcher = "lead"
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
        );
        let path = self.dir.path().join("team.toml");
        fs::write(&path, text).unwrap();
        team::read(&path).unwrap()
    }
    /// A realistic state path: longer than the 45-byte private socket limit.
    fn state(&self) -> PathBuf {
        self.base()
            .join("a-state-root-longer-than-private-sockets-allow")
    }
    fn team_dir(&self) -> PathBuf {
        self.state().join("teams/core")
    }
    fn start(&self) -> crewmoor::Result<Value> {
        let mut next = 0;
        team::start(
            self.team(),
            &self.state(),
            &mut layout(self),
            &mut |cfg, root| {
                let fake = &self.fakes[next];
                next += 1;
                engine::launch(cfg, root, &Host(fake))
            },
        )
    }
    /// Roles start dispatcher first: lead uses fake 0, worker fake 1.
    fn fake(&self, role: &str) -> &common::Fake {
        &self.fakes[usize::from(role == "worker")]
    }
    fn stop(&self) -> crewmoor::Result<Value> {
        team::stop(&self.team_dir(), &mut |root| {
            let mut record = Record::load(root)?;
            let _lock = record.lock()?;
            let fake = self.fake(&record.config.role);
            engine::stop(&mut record, &Host(fake))?;
            Ok(record)
        })
    }
    fn record(&self, role: &str) -> Record {
        Record::load(&team::launch_root(&self.team_dir(), role)).unwrap()
    }
    fn instance(&self, role: &str) -> String {
        self.record(role).config.feedback_instance.unwrap()
    }
    fn session_start(&self, role: &str, kind: AgentKind, session: &str) {
        let input = json!({"session_id":session,"cwd":self.base(),
            "hook_event_name":"SessionStart","source":"startup"});
        feedback::ingest(
            &self.base().join("registry"),
            &self.instance(role),
            &kind,
            &serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
    }
}
/// The existing App layout as the dispatcher's fake core serves it (`doc.get`).
fn layout(setup: &Setup) -> impl FnMut(&crewmoor::app::Binding) -> crewmoor::Result<Value> + '_ {
    |app| {
        assert_eq!(app.socket, setup.socket);
        let fake = &setup.fakes[0];
        setup.layout_reads.set(setup.layout_reads.get() + 1);
        Ok(fake.app_doc.borrow().clone().unwrap_or_else(|| {
            json!({"version":1,"value":{"spaces":[{"id":"sp-existing","tabs":[{"id":"tb-existing"}]}]}})
        }))
    }
}
/// Message side of the same existing core: status, terminal get and send only.
struct Core<'a> {
    setup: &'a Setup,
    sends: RefCell<Vec<Vec<String>>>,
}
impl Transport for Core<'_> {
    fn call(&self, config: &message::Config, args: &[String]) -> crewmoor::Result<Value> {
        let socket = self.setup.socket.canonicalize()?;
        assert_eq!(config.socket, socket);
        match args.first().map(String::as_str) {
            Some("server") => Ok(json!({"core":{"id":"own-core"},"version":"0.1.15"})),
            Some("get") => self.terminal(&args[1]),
            Some("send") => {
                self.sends.borrow_mut().push(args.to_vec());
                Ok(json!({"turn":0}))
            }
            _ => Err("unexpected core message call".into()),
        }
    }
}
impl Core<'_> {
    fn new(setup: &Setup) -> Core<'_> {
        Core {
            setup,
            sends: RefCell::default(),
        }
    }
    fn terminal(&self, handle: &str) -> crewmoor::Result<Value> {
        for role in ["lead", "worker"] {
            let record = self.setup.record(role);
            if record.handle.as_deref() == Some(handle) {
                return Ok(self.setup.fake(role).terminal(&record));
            }
        }
        Err("terminal absent".into())
    }
}
fn report() -> Message {
    Message {
        id: "report-1".into(),
        from: "worker".into(),
        to: "lead".into(),
        reply_to: None,
        content: "synthetic".into(),
    }
}

#[test]
fn core_team_start_launches_each_role_in_the_existing_app_core_with_markers() {
    let setup = Setup::new();
    let started = setup.start().unwrap();
    let dir = setup.team_dir();
    assert_eq!(started["team_dir"], json!(dir));
    assert_eq!(started["roles"][0]["role"], "lead");
    assert_eq!(started["roles"][1]["role"], "worker");
    let snapshot: Value =
        serde_json::from_slice(&fs::read(dir.join("team.json")).unwrap()).unwrap();
    assert_eq!(snapshot["runtime"], "core");
    for role in ["lead", "worker"] {
        let record = setup.record(role);
        let fake = setup.fake(role);
        assert!(!record.server_owned);
        assert_eq!(record.binding_scope, "terminal");
        assert_eq!(record.session, "app-stable");
        assert_eq!(record.handle, Some(fake.address()));
        assert!(record.process_verified);
        assert!(record.config.app.is_some());
        assert!(record.config.host.is_none());
        record.binding().unwrap();
        assert!(dir.join(format!("roles/{role}/core.json")).exists());
        let calls = fake.calls.borrow();
        let run = calls.iter().find(|c| c[0] == "run").unwrap();
        let (method, params) = crewmoor::app::request(&record, run).unwrap();
        assert_eq!(method, "terminal.run");
        assert_eq!(params["env"], json!({INSTANCE_ENV: setup.instance(role)}));
        assert!(!calls.iter().any(|c| c[..] == ["server", "start"]));
    }
    let worker = setup.record("worker");
    assert_eq!(
        worker.config.argv().last().unwrap(),
        "Read the task file and report once."
    );
    assert!(worker.config.argv().contains(&"--no-daemon".to_owned()));
    let again = setup.start().unwrap_err().to_string();
    assert!(again.contains("team directory exists"));
}
#[test]
fn core_team_bind_uses_the_app_target_identity_and_messages_through_core() {
    let setup = Setup::new();
    setup.start().unwrap();
    let dir = setup.team_dir();
    let core = Core::new(&setup);
    let early = team::bind(&dir, "worker", &core).unwrap_err().to_string();
    assert!(early.contains("not registered"));
    setup.session_start("lead", AgentKind::Claude, "lead-session");
    setup.session_start("worker", AgentKind::Codex, "worker-session");
    let bound = team::bind(&dir, "worker", &core).unwrap();
    assert_eq!(bound["dispatcher_session"], "lead-session");
    assert_eq!(bound["worker_session"], "worker-session");
    assert!(team::bind(&dir, "worker", &core).is_err());
    let binding: Binding =
        serde_json::from_slice(&fs::read(dir.join("roles/worker/binding.json")).unwrap()).unwrap();
    assert_eq!(binding.config.core_id.as_deref(), Some("own-core"));
    assert_eq!(binding.config.core_version.as_deref(), Some("0.1.15"));
    assert!(binding.worker.terminal_id.is_none());
    assert!(binding.worker.process.is_none());
    assert_eq!(binding.worker.core.as_ref().unwrap().created_ms, 42);
    let registry = setup.base().join("registry");
    let reference = feedback::load_dispatcher_binding_ref(&registry, &setup.instance("lead"))
        .unwrap()
        .expect("dispatcher binding reference written on bind");
    assert_eq!(reference.binding, dir.join("roles/worker/binding.json"));
    let report = report();
    let receipt = message::send(&binding, &report, &core).unwrap();
    assert_eq!(receipt.transport, "accepted");
    assert_eq!(receipt.native_turn, Some(0));
    let sends = core.sends.borrow();
    assert_eq!(sends.len(), 1);
    assert_eq!(sends[0][1], setup.fake("lead").address());
}

#[test]
fn core_team_bind_refuses_a_terminal_generation_this_team_did_not_launch() {
    let setup = Setup::new();
    setup.start().unwrap();
    setup.session_start("lead", AgentKind::Claude, "lead-session");
    setup.session_start("worker", AgentKind::Codex, "worker-session");
    let path = team::launch_root(&setup.team_dir(), "worker").join("record.json");
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["created_ms"] = json!(41);
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    let error = team::bind(&setup.team_dir(), "worker", &Core::new(&setup))
        .unwrap_err()
        .to_string();
    assert!(error.contains("not the resource launched by this team"));
    assert!(!setup.team_dir().join("roles/worker/binding.json").exists());
}

#[test]
fn core_team_stop_removes_only_owned_terminals_and_keeps_the_app_core() {
    let setup = Setup::new();
    setup.start().unwrap();
    *setup.fakes[1].extra.borrow_mut() = true;
    let stopped = setup.stop().unwrap();
    for (index, role) in ["lead", "worker"].into_iter().enumerate() {
        assert_eq!(
            stopped["roles"][index]["result"],
            "target-stopped-host-retained"
        );
        let fake = setup.fake(role);
        assert!(*fake.removed.borrow());
        assert!(!*fake.stopped.borrow());
        let calls = fake.calls.borrow();
        assert!(!calls.iter().any(|c| c[..] == ["server", "stop"]));
        assert!(!calls.iter().any(|c| c[0] == "ls"));
        process::same(setup.record(role).server.as_ref().unwrap()).unwrap();
        assert!(setup.record(role).binding().is_err());
    }
    assert!(setup.socket.exists());
    let again = setup.stop().unwrap();
    assert_eq!(again["roles"][1]["result"], "target-stopped-host-retained");
}

#[test]
fn core_team_example_and_target_rules() {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/team-core.toml");
    team::read(&example).unwrap().validate().unwrap();
    let setup = Setup::new();
    let mut team = setup.team();
    team.app_target = None;
    assert!(team.validate().is_err());
    let mut team = setup.team();
    team.host = Some(crewmoor::config::Host {
        socket: setup.socket.clone(),
        server_version: "0.9.4".into(),
    });
    assert!(team.validate().is_err());
    // An unreadable target fails explicit placement preflight before any write/core call.
    let mut team = setup.team();
    team.app_target = Some(setup.dir.path().join("missing.toml"));
    team.validate().unwrap();
    team::start(
        team,
        &setup.state(),
        &mut layout(&setup),
        &mut |cfg, root| engine::launch(cfg, root, &Host(&setup.fakes[0])),
    )
    .unwrap_err();
    assert!(!setup.team_dir().exists());
    assert!(setup.fakes[0].calls.borrow().is_empty());
}

/// Labels the App adoption reads from this role's `terminal.run` request.
fn tab_labels(setup: &Setup, role: &str) -> (Value, Value) {
    let record = setup.record(role);
    let calls = setup.fake(role).calls.borrow();
    let run = calls.iter().find(|c| c[0] == "run").unwrap();
    let (_, params) = crewmoor::app::request(&record, run).unwrap();
    let labels = &params["labels"];
    (labels["herdr.space"].clone(), labels["herdr.tab"].clone())
}
fn tab_doc() -> Value {
    json!({"version":1,"value":{"spaces":[{"id":"sp-existing",
        "tabs":[{"id":"tb-existing"},{"id":"tb-lead"},{"id":"tb-worker"}]}]}})
}
fn start_team(setup: &Setup, team: team::Team) -> crewmoor::Result<Value> {
    let mut next = 0;
    team::start(
        team,
        &setup.state(),
        &mut layout(setup),
        &mut |cfg, root| {
            let fake = &setup.fakes[next];
            next += 1;
            engine::launch(cfg, root, &Host(fake))
        },
    )
}

#[test]
fn core_team_role_tab_id_overrides_the_target_tab_for_that_role_only() {
    let setup = Setup::new();
    for fake in &setup.fakes {
        *fake.app_doc.borrow_mut() = Some(tab_doc());
    }
    let mut team = setup.team();
    team.roles[0].tab_id = Some("tb-worker".into());
    team.roles[1].tab_id = Some("user-machine/tb-lead".into());
    start_team(&setup, team).unwrap();
    for (role, tab) in [("lead", "tb-lead"), ("worker", "tb-worker")] {
        assert_eq!(tab_labels(&setup, role), (json!("sp-existing"), json!(tab)));
        let record = setup.record(role);
        let app = record.config.app.as_ref().unwrap();
        assert_eq!(app.space_id.as_deref(), Some("user-machine/sp-existing"));
        assert!(record.config.app_target.is_none());
    }
}

#[test]
fn core_team_without_role_tab_id_keeps_the_target_tab() {
    let setup = Setup::new();
    start_team(&setup, setup.team()).unwrap();
    for role in ["lead", "worker"] {
        let labels = tab_labels(&setup, role);
        assert_eq!(labels, (json!("sp-existing"), json!("tb-existing")));
    }
    let snapshot: Value =
        serde_json::from_slice(&fs::read(setup.team_dir().join("team.json")).unwrap()).unwrap();
    assert_eq!(snapshot["roles"][0]["tab_id"], Value::Null);
}

#[test]
fn core_team_new_target_tab_and_new_role_override_preserve_each_source() {
    let setup = Setup::new();
    let target = setup.dir.path().join("app-target.toml");
    let text = fs::read_to_string(&target)
        .unwrap()
        .replace("tb-existing", "tb-new-target");
    fs::write(target, text).unwrap();
    let mut team = setup.team();
    team.roles[0].tab_id = Some("tb-new-worker".into());
    start_team(&setup, team).unwrap();
    assert_eq!(tab_labels(&setup, "lead").1, json!("tb-new-target"));
    assert_eq!(tab_labels(&setup, "worker").1, json!("tb-new-worker"));
    assert!(setup.record("lead").project.is_none());
    assert!(setup.record("worker").project.is_none());
    setup.stop().unwrap();
}

#[test]
fn core_team_role_tab_id_rules_are_checked_before_any_write() {
    let setup = Setup::new();
    let mut team = setup.team();
    for bad in [
        "tb bad",
        "other-machine/tb-new",
        "user-machine/tb/extra",
        "",
    ] {
        team.roles[0].tab_id = Some(bad.into());
        assert!(start_team(&setup, team.clone()).is_err());
        assert!(!setup.team_dir().exists());
    }
    fs::write(
        setup.dir.path().join("project-target.toml"),
        format!(
            "session = \"app-stable\"\nsocket = \"{}\"\ncore_id = \"own-core\"\n\
             machine = \"user-machine\"\n",
            setup.socket.display()
        ),
    )
    .unwrap();
    let mut team = setup.team();
    team.app_target = Some(setup.dir.path().join("project-target.toml"));
    team.roles[0].tab_id = Some("tb-worker".into());
    let error = start_team(&setup, team).unwrap_err().to_string();
    assert!(error.contains("needs space_id"));
    assert!(!setup.team_dir().exists());
    assert_eq!(setup.layout_reads.get(), 0);
}

#[test]
fn core_team_new_role_tabs_are_forwarded_without_precreation() {
    let setup = Setup::new();
    let mut team = setup.team();
    for role in &mut team.roles {
        role.tab_id = Some(format!("user-machine/tb-new-{}", role.role));
    }
    start_team(&setup, team).unwrap();
    assert_eq!(setup.layout_reads.get(), 1);
    for role in ["lead", "worker"] {
        assert!(setup.record(role).project.is_none());
        assert_eq!(
            tab_labels(&setup, role),
            (json!("sp-existing"), json!(format!("tb-new-{role}")))
        );
    }
    setup.stop().unwrap();
}

#[test]
fn core_team_new_target_tab_is_checked_by_space_even_without_role_overrides() {
    let setup = Setup::new();
    let path = setup.dir.path().join("app-target.toml");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("tb-existing", "user-machine/tb-new-target");
    fs::write(path, text).unwrap();
    start_team(&setup, setup.team()).unwrap();
    assert_eq!(setup.layout_reads.get(), 1);
    assert_eq!(tab_labels(&setup, "lead").1, json!("tb-new-target"));
    assert_eq!(tab_labels(&setup, "worker").1, json!("tb-new-target"));
    setup.stop().unwrap();
}

#[test]
fn core_team_missing_explicit_space_withholds_before_any_write_with_or_without_role_tabs() {
    let setup = Setup::new();
    *setup.fakes[0].app_doc.borrow_mut() = Some(json!({"version":1,"value":{"spaces":[
        {"id":"other-space","tabs":[]}]}}));
    for tab in [None, Some("tb-new".into())] {
        let mut team = setup.team();
        team.roles[0].tab_id = tab;
        let error = start_team(&setup, team).unwrap_err().to_string();
        assert!(error.contains("space does not exist"));
        assert!(!setup.team_dir().exists());
        assert!(setup.fakes.iter().all(|f| f.calls.borrow().is_empty()));
    }
}

#[test]
fn core_team_without_role_tab_id_checks_explicit_space_before_start() {
    let setup = Setup::new();
    start_team(&setup, setup.team()).unwrap();
    assert_eq!(setup.layout_reads.get(), 1);
}

#[test]
fn core_team_project_placement_without_role_tabs_skips_preflight_layout_read() {
    let setup = Setup::new();
    let target = setup.dir.path().join("app-target.toml");
    let mut app: crewmoor::app::Binding =
        toml::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    app.space_id = None;
    app.tab_id = None;
    fs::write(target, toml::to_string(&app).unwrap()).unwrap();
    start_team(&setup, setup.team()).unwrap();
    assert_eq!(setup.layout_reads.get(), 0);
    assert!(setup.record("lead").project.is_some());
    assert!(setup.record("worker").project.is_some());
    setup.stop().unwrap();
}
