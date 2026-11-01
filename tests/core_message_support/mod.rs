use crewmoor::{
    config::{AgentKind, Runtime},
    feedback::{self, Evidence, Registration},
    message::{Binding, Config, Native, Peer},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub config: Config,
    pub state: Arc<Mutex<Value>>,
    calls: Arc<Mutex<Vec<Value>>>,
    children: Vec<Child>,
    thread: Option<JoinHandle<()>>,
}
impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(dir.path().join("core.sock")).unwrap();
        let children = vec![spawn(dir.path()), spawn(dir.path())];
        let peers = [
            peer(dir.path(), "dispatcher", "t0", AgentKind::Codex),
            peer(dir.path(), "worker", "t1", AgentKind::Claude),
        ];
        let [dispatcher, worker] = peers;
        let config = Config {
            runtime: Runtime::Core,
            core_id: Some("test-core".into()),
            core_version: Some("0.1.15".into()),
            runtime_binary: "/bin/false".into(),
            socket: dir.path().join("core.sock"),
            feedback_registry: dir.path().into(),
            dispatcher,
            worker,
        };
        let state = Arc::new(Mutex::new(json!({"core_id":"test-core","version":"0.1.15",
            "t0":terminal(dir.path(),"t0",children[0].id()),"t1":terminal(dir.path(),"t1",children[1].id())})));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let state_thread = Arc::clone(&state);
        let calls_thread = Arc::clone(&calls);
        let thread = std::thread::spawn(move || serve(listener, &state_thread, &calls_thread));
        Self {
            dir,
            config,
            state,
            calls,
            children,
            thread: Some(thread),
        }
    }
    pub fn bind(&self) -> Binding {
        Binding::bind(
            serde_json::from_value(serde_json::to_value(&self.config).unwrap()).unwrap(),
            &Native,
        )
        .unwrap()
    }
    pub fn sends(&self) -> Vec<Value> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v["method"] == "terminal.send")
            .map(|v| v["params"].clone())
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let mut stream = UnixStream::connect(&self.config.socket).unwrap();
        stream
            .write_all(b"{\"method\":\"fixture.stop\"}\n")
            .unwrap();
        self.thread.take().unwrap().join().unwrap();
        for child in &mut self.children {
            drop(child.stdin.take());
            child.wait().unwrap();
        }
    }
}
fn spawn(cwd: &std::path::Path) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_transport-fixture"))
        .arg("hold")
        .env("CREWMOOR_TRANSPORT_HANDSHAKE", "1")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut marker = [0; 2];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut marker)
        .unwrap();
    assert_eq!(&marker, b"R\n");
    child
}
fn peer(cwd: &std::path::Path, role: &str, handle: &str, agent: AgentKind) -> Peer {
    let context = feedback::register(
        cwd,
        Registration {
            team: "core-message-test".into(),
            role: role.into(),
            task: "issue-11".into(),
            cwd: cwd.into(),
            agent,
            agent_version: "synthetic".into(),
            evidence: Evidence::Synthetic,
            native_session_id: Some(format!("native-{role}")),
        },
    )
    .unwrap();
    Peer {
        instance: context.instance,
        handle: handle.into(),
        agent_executable: Some(env!("CARGO_BIN_EXE_transport-fixture").into()),
    }
}
fn terminal(cwd: &std::path::Path, address: &str, pid: u32) -> Value {
    json!({"address":address,"argv":[env!("CARGO_BIN_EXE_transport-fixture"),"hold"],"cwd":cwd,"pid":pid,
        "created_ms":1,"labels":{},"state":"live","screen":"exact","cols":80,"rows":24})
}
fn serve(listener: UnixListener, state: &Mutex<Value>, calls: &Mutex<Vec<Value>>) {
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let mut line = String::new();
        BufReader::new(&mut stream).read_line(&mut line).unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        if request["method"] == "fixture.stop" {
            break;
        }
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 1);
        calls.lock().unwrap().push(request.clone());
        if let Some(response) = respond(&request, &mut state.lock().unwrap()) {
            stream
                .write_all(format!("{response}\n").as_bytes())
                .unwrap();
        }
    }
}
fn respond(request: &Value, state: &mut Value) -> Option<Value> {
    let result = match request["method"].as_str().unwrap() {
        "server.status" => {
            json!({"core":{"id":state["core_id"],"name":"fixture"},"version":state["version"],
            "pid":std::process::id(),"uptime_ms":0,"holders":[],"live":2,"finished":0})
        }
        "terminal.get" => state[request["params"]["terminal"].as_str().unwrap()].clone(),
        "terminal.send" => return send_result(state),
        _ => panic!("unexpected method"),
    };
    Some(json!({"jsonrpc":"2.0","id":1,"result":result}))
}
fn send_result(state: &mut Value) -> Option<Value> {
    match state["mode"].as_str().unwrap_or("") {
        "lost" => None,
        "blocked" => Some(json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,
            "message":"native dialog needs attention","data":{"kind":"blocked"}}})),
        "missing-turn" => Some(json!({"jsonrpc":"2.0","id":1,"result":{}})),
        "changed-after-send" => {
            state["t1"]["created_ms"] = json!(2);
            Some(accepted())
        }
        _ => Some(accepted()),
    }
}
fn accepted() -> Value {
    json!({"jsonrpc":"2.0","id":1,"result":{"turn":0,"status":null,"started":null}})
}
