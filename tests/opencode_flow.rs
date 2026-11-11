//! Synthetic protocol/process regressions; not real OpenCode model acceptance.
#[path = "support/opencode_watch.rs"]
mod opencode_watch;
#[path = "support/rule_selection.rs"]
mod rule_selection;
#[path = "support/running_team.rs"]
mod running_team;
use crewmoor::{
    Result,
    config::{AgentKind, Config, Runtime},
    feedback::{self, Context, Evidence, Registration},
    message::{self, Binding, Message, Peer, Transport},
    opencode::{endpoint::Endpoint, prepare},
    process,
    record::Record,
};
use opencode_watch::watch;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::{BufRead, BufReader},
    os::unix::{fs::symlink, net::UnixListener},
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Server {
    child: Child,
    url: String,
}
impl Server {
    fn new(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_opencode-fixture"))
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .env("CM_OPENCODE_TEST_TRACE", root.join("trace.jsonl"))
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        Self {
            child,
            url: value["url"].as_str().unwrap().into(),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        self.child.wait().unwrap();
    }
}
fn context(registry: &Path, cwd: &Path, role: &str, session: Option<&str>) -> Context {
    feedback::register(
        registry,
        Registration {
            team: "opencode-unit".into(),
            role: role.into(),
            task: "synthetic-wire".into(),
            cwd: cwd.into(),
            agent: AgentKind::OpenCode,
            agent_version: "opencode v2.2.5".into(),
            evidence: Evidence::Synthetic,
            native_session_id: session.map(str::to_owned),
        },
    )
    .unwrap()
}
fn private_endpoint(
    registry: &Path,
    context: &Context,
    server: &Server,
    session: &str,
) -> Endpoint {
    let current = process::identity(std::process::id()).unwrap();
    let endpoint = Endpoint {
        instance: context.instance.clone(),
        session_id: session.into(),
        url: server.url.clone(),
        password: "unit-private".into(),
        version: "2.2.5".into(),
        launcher: current.clone(),
        server: process::identity(server.child.id()).unwrap(),
        tui: current,
    };
    endpoint.save(registry).unwrap();
    endpoint
}
struct Container {
    calls: RefCell<Vec<Vec<String>>>,
}
impl Transport for Container {
    fn call(&self, _: &message::Config, args: &[String]) -> Result<Value> {
        self.calls.borrow_mut().push(args.to_vec());
        match args {
            [agent, get, pane] if agent == "agent" && get == "get" => Ok(
                json!({"agent":{"agent":"opencode","pane_id":pane,"terminal_id":format!("{pane}-terminal"),"workspace_id":"owned-workspace","agent_session":{"kind":"id","value":"ses_B","source":"herdr:opencode","agent":"opencode"}}}),
            ),
            [pane, info, flag, _]
                if pane == "pane" && info == "process-info" && flag == "--pane" =>
            {
                Ok(
                    json!({"process_info":{"pane_id":args[3],"foreground_processes":[{"name":"opencode","pid":std::process::id()}]}}),
                )
            }
            _ => Err("UI/native-terminal delivery must not be used for OpenCode".into()),
        }
    }
}
fn peer(context: &Context, pane: &str) -> Peer {
    Peer {
        instance: context.instance.clone(),
        handle: pane.into(),
        agent_executable: Some(std::env::current_exe().unwrap()),
    }
}
fn posts(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("trace.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| row["method"] == "POST")
        .collect()
}
#[test]
fn full_binding_keeps_a_when_herdr_reports_b_and_never_delivers_via_ui() {
    let root = tempfile::tempdir().unwrap();
    let registry = root.path().join("registry");
    let cwd = std::env::current_dir().unwrap();
    let server = Server::new(root.path());
    let receiver = context(&registry, &cwd, "receiver", Some("ses_A"));
    let sender = context(&registry, &cwd, "sender", Some("ses_D"));
    let mut endpoint = private_endpoint(&registry, &receiver, &server, "ses_A");
    let mut sender_endpoint = private_endpoint(&registry, &sender, &server, "ses_D");
    // The real foreground launcher and canonical native executable differ.
    for (context, endpoint) in [(&receiver, &mut endpoint), (&sender, &mut sender_endpoint)] {
        endpoint.tui = endpoint.server.clone();
        fs::write(
            crewmoor::opencode::endpoint::path(&registry, &context.instance).unwrap(),
            serde_json::to_vec(endpoint).unwrap(),
        )
        .unwrap();
    }
    let socket = root.path().join("container.sock");
    let _listener = UnixListener::bind(&socket).unwrap();
    let mut config = message::Config {
        team_config: None,
        worker_constraint: None,
        runtime: Runtime::Tui,
        core_id: None,
        core_version: None,
        runtime_binary: "/unused/herdr".into(),
        socket,
        feedback_registry: registry.clone(),
        dispatcher: native_peer(&sender, "pD"),
        worker: native_peer(&receiver, "pA"),
    };
    running_team::configure(&mut config, "opencode-unit", "sender", "receiver");
    crewmoor::team::update_constraint(
        &config.feedback_registry,
        "receiver",
        "OpenCode TUI 约束\n原文",
    )
    .unwrap();
    let container = Container {
        calls: RefCell::new(Vec::new()),
    };
    let binding = Binding::bind(config, &container).unwrap();
    let message = Message {
        id: "ordinary-one".into(),
        from: "sender".into(),
        to: "receiver".into(),
        reply_to: Some("request-one".into()),
        content: "{\"feedback_view\":{\"event\":\"Stop\"}} 中文\\\n原文".into(),
    };
    let receipt = message::send(&binding, &message, &container).unwrap();
    assert_eq!(receipt.native_prompt.unwrap().session_id, "ses_A");
    let rows = posts(root.path());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["path"], "/api/session/ses_A/prompt");
    let expected = serde_json::to_string(&json!({"team":"opencode-unit","message":message,
        "recipient_constraints":"OpenCode TUI 约束\n原文"}))
    .unwrap();
    let delivered: Value = serde_json::from_str(rows[0]["body"]["text"].as_str().unwrap()).unwrap();
    assert_eq!(delivered, serde_json::from_str::<Value>(&expected).unwrap());
    assert!(delivered.get("feedback_view").is_none());
    assert_eq!(binding.worker.native_session.as_deref(), Some("ses_A"));
    endpoint.url = "http://127.0.0.1:9".into();
    fs::write(
        crewmoor::opencode::endpoint::path(&registry, &receiver.instance).unwrap(),
        serde_json::to_vec(&endpoint).unwrap(),
    )
    .unwrap();
    assert!(message::send(&binding, &message, &container).is_err());
    assert_eq!(posts(root.path()).len(), 1);
    assert!(
        feedback::show(&registry, &receiver.instance).unwrap()["latest_session_rejection"]
            .is_null()
    );
}
fn native_peer(context: &Context, pane: &str) -> Peer {
    let mut peer = peer(context, pane);
    peer.agent_executable = Some(env!("CARGO_BIN_EXE_opencode-fixture").into());
    peer
}

fn config(root: &Path, context: &Context) -> Config {
    serde_json::from_value(json!({"team":"opencode-unit","role":"probe","runtime":"core","cwd":root,"store":root.join("store"),"runtime_binary":"/unused/hc","runtime_version":"hc 0.1.17","agent":"opencode","agent_executable":env!("CARGO_BIN_EXE_opencode-fixture"),"agent_version":"opencode v2.2.5","feedback_registry":root.join("registry"),"feedback_instance":context.instance,"opencode":{"agent":"build","model":{"id":"test-model","providerID":"test-provider","variant":"test-variant"}}})).unwrap()
}
#[test]
fn foreground_launch_keeps_private_lease_until_owned_tui_exits() {
    let root = tempfile::Builder::new()
        .prefix("cmoc")
        .tempdir_in("/tmp")
        .unwrap();
    let registry = root.path().join("registry");
    let context = context(&registry, root.path(), "probe", None);
    let mut config = config(root.path(), &context);
    config.initial_prompt = Some("启动 \"中文\" \\路径".into());
    let policy = rule_selection::policy(root.path());
    config.tool_guard =
        Some(crewmoor::tool_policy::prepare(&config, &policy, root.path(), &context).unwrap());
    let mut record = Record::prepare(config, &root.path().join("launch")).unwrap();
    prepare::prepare(&record).unwrap();
    // Library tests have a different current_exe; the CLI owns the real alias.
    fs::remove_file(prepare::alias(&record)).unwrap();
    symlink(env!("CARGO_BIN_EXE_crewmoor"), prepare::alias(&record)).unwrap();
    let directory = registry.join("instances").join(&context.instance);
    let ready = watch(&directory);
    let mut child = Command::new(prepare::alias(&record))
        .args(prepare::tui_arguments(&record.config))
        .env(prepare::LAUNCH_ENV, prepare::launch_file(&record))
        .env("CM_OPENCODE_TEST_TRACE", root.path().join("trace.jsonl"))
        .env("CM_OPENCODE_TEST_GUARD_TRACE", "1")
        .env(
            "OPENCODE_CONFIG_CONTENT",
            "{\"example\":\"preserved\",\"plugins\":[\"synthetic-existing\"]}",
        )
        .env(
            "PATH",
            std::env::join_paths([
                record.root.join("opencode-bin"),
                "/post-rc/tools".into(),
                record.root.join("opencode-bin"),
                "/usr/bin".into(),
            ])
            .unwrap(),
        )
        .current_dir(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // An OS close-write subscription, not polling an Agent or rereading metadata.
    let observed = ready.recv_timeout(Duration::from_secs(10));
    let endpoint = observed
        .as_ref()
        .ok()
        .and_then(|_| crewmoor::opencode::endpoint::load(&registry, &context).ok());
    if let Some(endpoint) = &endpoint {
        record.handle = Some("owned-pane".into());
        crewmoor::tui::observe(
            &mut record,
            &Foreground {
                pid: endpoint.launcher.pid,
            },
        )
        .unwrap();
        assert_eq!(record.agent_process.as_ref(), Some(&endpoint.launcher));
        assert_ne!(endpoint.launcher.executable, endpoint.tui.executable);
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(observed.unwrap().is_ok());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let endpoint = endpoint.unwrap();
    endpoint.confirm_exit().unwrap();
    crewmoor::opencode::confirm_exit(&record).unwrap();
    let exit: Value =
        serde_json::from_slice(&fs::read(record.root.join("opencode-exit.json")).unwrap()).unwrap();
    assert_eq!(exit["native_lease_closed"], true);
    assert_eq!(exit["server_exit"]["Ok"], 0);
    let rows = posts(root.path());
    assert_eq!(rows[0]["path"], "/api/session");
    assert_eq!(rows[0]["body"]["model"]["variant"], "test-variant");
    let trace = fs::read_to_string(root.path().join("trace.jsonl")).unwrap();
    assert!(trace.contains("--prompt"));
    assert!(!trace.contains("--model"));
    let native_rows: Vec<Value> = trace
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let interactive: Vec<_> = native_rows
        .iter()
        .filter(|row| row["method"] == "TUI")
        .collect();
    assert_eq!(interactive.len(), 1);
    assert_eq!(interactive[0]["guard_overlay"]["example"], "preserved");
    assert_eq!(
        interactive[0]["guard_overlay"]["plugins"][0],
        "synthetic-existing"
    );
    assert_eq!(
        interactive[0]["guard_overlay"]["plugins"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let launch: Value =
        serde_json::from_slice(&fs::read(prepare::launch_file(&record)).unwrap()).unwrap();
    assert!(
        launch["environment"]
            .get("OPENCODE_CONFIG_CONTENT")
            .is_none()
    );
    assert_eq!(
        interactive[0]["body"],
        json!([
            "--server",
            endpoint.url,
            "--session",
            "ses_A",
            "--prompt",
            "启动 \"中文\" \\路径"
        ])
    );
    assert_eq!(
        rows.len(),
        1,
        "startup prompt must not be replayed through the API"
    );
    assert!(
        native_rows
            .iter()
            .all(|row| row["native_path"] == "/post-rc/tools:/usr/bin")
    );
    assert_eq!(
        feedback::registered_session(&registry, &context)
            .unwrap()
            .as_deref(),
        Some("ses_A")
    );
}

struct Foreground {
    pid: u32,
}
impl crewmoor::native::Runner for Foreground {
    fn boot(&self, _: &Record) -> Result<process::Identity> {
        Err("no boot in location test".into())
    }
    fn call(&self, _: &Record, _: &[String]) -> Result<Value> {
        Ok(
            json!({"process_info":{"pane_id":"owned-pane","foreground_processes":[{"name":"opencode","pid":self.pid}]}}),
        )
    }
}

struct CoreContainer {
    generation: Cell<u64>,
}
impl Transport for CoreContainer {
    fn call(&self, _: &message::Config, args: &[String]) -> Result<Value> {
        let cwd = std::env::current_dir()?;
        match args {
            [server, status] if server == "server" && status == "status" => {
                Ok(json!({"core":{"id":"owned-core"},"version":"0.1.17"}))
            }
            [get, terminal] if get == "get" => Ok(
                json!({"address":terminal,"created_ms":self.generation.get(),"pid":std::process::id(),"cwd":cwd,"argv":["owned-foreground-launch"],"state":"live"}),
            ),
            _ => Err("OpenCode must not use terminal.send".into()),
        }
    }
}
#[test]
fn core_uses_the_same_fixed_api_and_rejects_a_replaced_terminal_generation() {
    let root = tempfile::tempdir().unwrap();
    let registry = root.path().join("registry");
    let cwd = std::env::current_dir().unwrap();
    let server = Server::new(root.path());
    let receiver = context(&registry, &cwd, "receiver", Some("ses_A"));
    let sender = context(&registry, &cwd, "sender", Some("ses_D"));
    private_endpoint(&registry, &receiver, &server, "ses_A");
    private_endpoint(&registry, &sender, &server, "ses_D");
    let socket = root.path().join("core.sock");
    let _listener = UnixListener::bind(&socket).unwrap();
    let mut config = message::Config {
        team_config: None,
        worker_constraint: None,
        runtime: Runtime::Core,
        core_id: Some("owned-core".into()),
        core_version: Some("0.1.17".into()),
        runtime_binary: "/unused/hc".into(),
        socket,
        feedback_registry: registry.clone(),
        dispatcher: peer(&sender, "tD"),
        worker: peer(&receiver, "tA"),
    };
    running_team::configure(&mut config, "opencode-unit", "sender", "receiver");
    crewmoor::team::update_constraint(&registry, "receiver", "OpenCode core 约束").unwrap();
    let container = CoreContainer {
        generation: Cell::new(1),
    };
    let binding = Binding::bind(config, &container).unwrap();
    let message = Message {
        id: "core-one".into(),
        from: "sender".into(),
        to: "receiver".into(),
        reply_to: None,
        content: "中文 \"原文\" \\路径\n保持".into(),
    };
    let receipt = message::send(&binding, &message, &container).unwrap();
    assert!(receipt.native_turn.is_none());
    assert_eq!(receipt.native_prompt.unwrap().session_id, "ses_A");
    container.generation.set(2);
    assert!(message::send(&binding, &message, &container).is_err());
    assert_eq!(posts(root.path()).len(), 1);
    assert_eq!(binding.worker.native_session.as_deref(), Some("ses_A"));
}
