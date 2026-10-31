use super::common;
use crewmoor::{
    app::{self, Binding},
    config::{AgentKind, Runtime},
    engine,
    native::{Native, strings},
    process,
    record::Record,
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixListener,
    process::{Child, Command, Stdio},
};

fn binding(base: &std::path::Path) -> Binding {
    Binding {
        session: "app-stable".into(),
        socket: base.join("existing.sock"),
        core_id: "own-core".into(),
        machine: "user-machine".into(),
        space_id: "user-machine/sp-existing".into(),
        tab_id: "tb-existing".into(),
    }
}
#[test]
fn app_both_agents_use_keep_local_labels_and_preserve_external_host_even_when_empty() {
    for agent in [AgentKind::Codex, AgentKind::Claude] {
        for extra in [false, true] {
            let (dir, mut cfg, fake) = common::setup(Runtime::Core);
            cfg.agent = agent.clone();
            cfg.agent_args = strings(&["--model", "two words ' ; $(data)"]);
            cfg.app = Some(binding(dir.path()));
            *fake.extra.borrow_mut() = extra;
            let mut r = engine::launch(cfg, &dir.path().join("i"), &fake).unwrap();
            assert_eq!(r.session, "app-stable");
            assert!(!r.server_owned);
            assert_eq!(r.binding_scope, "terminal");
            assert_eq!(r.app_terminal.as_deref(), Some("user-machine/t7"));
            assert!(r.process_verified);
            let args = crewmoor::core::launch_args(&r);
            assert!(args.contains(&"--keep".to_owned()));
            assert!(args.contains(&"herdr.space=sp-existing".to_owned()));
            assert!(args.contains(&"herdr.tab=tb-existing".to_owned()));
            let (method, params) = app::request(&r, &args).unwrap();
            assert_eq!(method, "terminal.run");
            assert_eq!(params["keep"], true);
            assert_eq!(params["argv"], json!(r.config.argv()));
            assert_eq!(params["labels"]["herdr.space"], "sp-existing");
            assert_eq!(params["labels"]["herdr.tab"], "tb-existing");
            assert_eq!(params["env"], json!({}));
            assert_eq!(params["wait"], false);
            if r.config.agent == AgentKind::Claude {
                assert!(!r.config.argv().contains(&"--no-daemon".to_owned()));
            }
            engine::stop(&mut r, &fake).unwrap();
            assert_eq!(r.phase, "target-stopped-host-retained");
            assert!(!r.process_verified);
            assert!(r.binding().is_err());
            assert!(*fake.removed.borrow());
            assert!(!*fake.stopped.borrow());
            process::same(r.server.as_ref().unwrap()).unwrap();
            assert!(r.socket().exists());
            let calls = fake.calls.borrow();
            assert!(!calls.contains(&strings(&["server", "stop"])));
            assert!(!calls.contains(&strings(&["ls", "--all"])));
        }
    }
}
#[test]
fn app_ids_are_explicit_local_or_exact_machine_prefix_and_never_guess_other_targets() {
    for invalid in [
        "another/sp-existing",
        "user-machine/",
        "user-machine/sp/x",
        "",
        "x\ny",
    ] {
        assert!(app::local_id("user-machine", invalid).is_err());
    }
    assert_eq!(
        app::local_id("user-machine", "sp-existing").unwrap(),
        "sp-existing"
    );
    assert_eq!(
        app::local_id("user-machine", "user-machine/sp-existing").unwrap(),
        "sp-existing"
    );
    let dir = common::directory();
    let app = binding(dir.path());
    assert!(app.validate(&Runtime::Tui).is_err());
    let mut app = app;
    app.socket = "relative".into();
    assert!(app.validate(&Runtime::Core).is_err());
}
#[test]
fn external_socket_missing_never_calls_runtime_cli_or_creates_host_configuration() {
    let dir = common::directory();
    let mut cfg = common::config(dir.path(), Runtime::Core);
    cfg.app = Some(binding(dir.path()));
    cfg.runtime_binary = "/definitely-not-a-runtime".into();
    let root = dir.path().join("i");
    assert!(engine::launch(cfg, &root, &Native).is_err());
    let saved = Record::load(&root).unwrap();
    assert!(!saved.request_issued);
    assert!(saved.binding().is_ok());
    assert!(!root.join("config").exists());
    assert!(!saved.socket().exists());
}
#[test]
fn app_rpc_allows_only_existing_socket_terminal_operations_and_decodes_real_errors() {
    let dir = common::directory();
    let mut cfg = common::config(dir.path(), Runtime::Core);
    cfg.app = Some(binding(dir.path()));
    let r = Record::prepare(cfg, &dir.path().join("i")).unwrap();
    for action in [
        strings(&["server", "start"]),
        strings(&["server", "stop"]),
        strings(&["workspace", "create"]),
        strings(&["get", "other/t7"]),
        strings(&["terminal.show"]),
    ] {
        assert!(app::request(&r, &action).is_err());
    }
    assert!(app::decode(json!({"jsonrpc":"2.0","id":2,"result":{}})).is_err());
    assert!(app::decode(json!({"jsonrpc":"2.0","id":1})).is_err());
    let error = app::decode(
        json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"missing","data":{"kind":"not_found"}}}),
    );
    assert!(crewmoor::failure::require_absent(error).is_ok());
}
fn clean_child(cwd: &std::path::Path, dirty: bool) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_transport-fixture"));
    for key in app::HOST_MARKERS {
        cmd.env_remove(key);
    }
    if dirty {
        cmd.env("CODEX_SESSION_ID", "synthetic-no-secret");
    }
    let mut child = cmd
        .arg("hold")
        .env("CREWMOOR_TRANSPORT_HANDSHAKE", "1")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut bytes = [0; 2];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut bytes)
        .unwrap();
    assert_eq!(&bytes, b"R\n");
    child
}
fn end(child: &mut Child) {
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
}
#[test]
fn host_marker_values_are_never_reported_and_contamination_or_missing_identity_withholds() {
    let dir = common::directory();
    let mut cfg = common::config(dir.path(), Runtime::Core);
    cfg.app = Some(binding(dir.path()));
    let mut r = Record::prepare(cfg, &dir.path().join("i")).unwrap();
    let mut clean = clean_child(dir.path(), false);
    let mut dirty = clean_child(dir.path(), true);
    r.server = Some(process::identity(clean.id()).unwrap());
    let mut status = json!({"core":{"id":"own-core"},"holders":[]});
    app::check_host(&r, &status).unwrap();
    status["holders"] = json!([{"instance":"user-holder","pid":dirty.id()}]);
    let error = app::check_host(&r, &status).unwrap_err().to_string();
    assert!(!error.contains("synthetic-no-secret"));
    assert!(!r.request_issued);
    status["holders"] = json!([]);
    r.server = Some(process::identity(dirty.id()).unwrap());
    assert!(app::check_host(&r, &status).is_err());
    r.server = None;
    assert!(app::check_host(&r, &status).is_err());
    status["core"]["id"] = json!("other-core");
    assert!(app::check_host(&r, &status).is_err());
    assert!(app::contaminated(b"CODEX_CI=1\0"));
    assert!(!app::contaminated(
        b"CODEX_HOME=retained\0HTTPS_PROXY=retained\0HC_SESSION=app-stable\0"
    ));
    end(&mut clean);
    end(&mut dirty);
}
#[test]
fn external_unknown_terminal_cannot_release_binding_or_claim_target_stopped() {
    let (dir, mut cfg, fake) = common::setup(Runtime::Core);
    cfg.app = Some(binding(dir.path()));
    let mut r = engine::launch(cfg, &dir.path().join("i"), &fake).unwrap();
    r.handle = None;
    assert!(engine::stop(&mut r, &fake).is_err());
    assert!(r.binding().is_ok());
    assert!(!*fake.stopped.borrow());
}
#[test]
fn app_missing_layout_or_wrong_space_tab_withholds_without_creating_layout() {
    let dir = common::directory();
    let app = binding(dir.path());
    for doc in [
        json!({}),
        json!({"value":{"spaces":[]}}),
        json!({"value":{"spaces":[{"id":"sp-existing","tabs":[{"id":"another-tab"}]}]}}),
    ] {
        assert!(app::existing_layout(&app, &doc).is_err());
    }
    let (dir, mut cfg, fake) = common::setup(Runtime::Core);
    let mut app = binding(dir.path());
    app.tab_id = "wrong-existing-target".into();
    cfg.app = Some(app);
    let root = dir.path().join("i");
    assert!(engine::launch(cfg, &root, &fake).is_err());
    let r = Record::load(&root).unwrap();
    assert!(!r.request_issued);
    assert!(r.binding().is_ok());
    assert!(!*fake.created.borrow());
    assert!(!*fake.stopped.borrow());
}
#[test]
fn wrong_existing_core_rpc_identity_refuses_before_run() {
    let dir = common::directory();
    let mut cfg = common::config(dir.path(), Runtime::Core);
    cfg.app = Some(binding(dir.path()));
    let r = Record::prepare(cfg, &dir.path().join("i")).unwrap();
    let listener = UnixListener::bind(r.socket()).unwrap();
    let fixture = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&socket).read_line(&mut line).unwrap();
        let call: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(call["method"], "server.status");
        let response = json!({"jsonrpc":"2.0","id":1,"result":{"core":{"id":"wrong-core"}}});
        socket
            .write_all(format!("{response}\n").as_bytes())
            .unwrap();
    });
    assert!(app::connect(&r).is_err());
    assert!(!r.request_issued);
    fixture.join().unwrap();
}

#[test]
fn native_app_path_uses_real_json_rpc_socket_without_cli_spawn_or_host_shutdown() {
    let dir = common::directory();
    let mut cfg = common::config(dir.path(), Runtime::Core);
    cfg.runtime_version = "hc 0.1.15".into();
    cfg.runtime_binary = "/no-cli-spawn-allowed".into();
    cfg.app = Some(binding(dir.path()));
    let listener = UnixListener::bind(&cfg.app.as_ref().unwrap().socket).unwrap();
    let mut server = clean_child(dir.path(), false);
    let mut holder = clean_child(dir.path(), false);
    let server_pid = server.id();
    let holder_pid = holder.id();
    let cwd = dir.path().to_path_buf();
    let fixture = std::thread::spawn(move || rpc_fixture(listener, &cwd, server_pid, holder_pid));
    let mut r = engine::launch(cfg, &dir.path().join("i"), &Native).unwrap();
    assert!(r.process_verified);
    assert_eq!(r.accepted, Some(true));
    engine::stop(&mut r, &Native).unwrap();
    assert_eq!(r.phase, "target-stopped-host-retained");
    assert!(r.binding().is_err());
    let calls = fixture.join().unwrap();
    assert_eq!(calls.iter().filter(|m| *m == "terminal.run").count(), 1);
    assert!(!calls.iter().any(|m| m == "server.stop"));
    process::identity(server.id()).unwrap();
    process::identity(holder.id()).unwrap();
    end(&mut server);
    end(&mut holder);
}
fn rpc_fixture(
    listener: UnixListener,
    cwd: &std::path::Path,
    server: u32,
    holder: u32,
) -> Vec<String> {
    let mut calls = Vec::new();
    let mut agent = None;
    let mut params = json!({});
    let mut finished = false;
    let mut removed = false;
    while calls.len() < 11 {
        let (mut socket, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&socket).read_line(&mut line).unwrap();
        if line.is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["jsonrpc"], "2.0");
        let method = request["method"].as_str().unwrap();
        assert_rpc_contract(&request);
        calls.push(method.to_owned());
        let result = match method {
            "doc.get" => {
                assert_eq!(request["params"], json!({"name":"herdr-app"}));
                json!({"version":1,"value":{"spaces":[{"id":"sp-existing","tabs":[{"id":"tb-existing"}]}]}})
            }
            "server.status" => {
                json!({"version":"0.1.15","pid":server,"core":{"id":"own-core","name":"fixture"},
                "holders":[{"instance":"existing-holder","pid":holder,"current":true,"terminals":1}],"uptime_ms":1,"live":1,"finished":0})
            }
            "terminal.run" => {
                params = request["params"].clone();
                agent = Some(clean_child(cwd, false));
                rpc_terminal(&params, &agent, false)
            }
            "terminal.kill" => {
                end(agent.as_mut().unwrap());
                finished = true;
                json!({})
            }
            "terminal.rm" => {
                removed = true;
                json!({"removed":"t7"})
            }
            "terminal.get" => rpc_terminal(&params, &agent, finished),
            _ => panic!("unexpected RPC method"),
        };
        let response = rpc_reply(result, removed, method);
        socket
            .write_all(format!("{response}\n").as_bytes())
            .unwrap();
    }
    assert!(removed);
    calls
}
fn rpc_reply(result: Value, removed: bool, method: &str) -> Value {
    if removed && method == "terminal.get" {
        json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"absent","data":{"kind":"not_found"}}})
    } else {
        json!({"jsonrpc":"2.0","id":1,"result":result})
    }
}
fn assert_rpc_contract(request: &Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/core-0.1.15-app-rpc.json");
    let contract: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let method = contract["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == request["method"])
        .unwrap();
    for key in request["params"].as_object().unwrap().keys() {
        assert!(method["allowed"].as_array().unwrap().contains(&json!(key)));
    }
    for key in method["required"].as_array().unwrap() {
        assert!(request["params"].get(key.as_str().unwrap()).is_some());
    }
}
fn rpc_terminal(params: &Value, agent: &Option<Child>, finished: bool) -> Value {
    let state = if finished { "finished" } else { "live" };
    json!({"address":"t7","created_ms":42,"labels":params["labels"],"argv":params["argv"],
        "cwd":params["cwd"],"state":state,"pid":agent.as_ref().map(Child::id),"keep":true,
        "screen":"exact","cols":80,"rows":24})
}
