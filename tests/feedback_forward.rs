use crewmoor::{
    config::{AgentKind, Runtime},
    feedback::{self, Evidence, Registration},
    message::{Binding, Config, Native, Peer, Transport},
};
use serde_json::{Value, json};
use std::{cell::Cell, fs, os::unix::net::UnixListener, process::Command};

struct Fixture {
    dir: tempfile::TempDir,
    _socket: UnixListener,
    binding: Binding,
    trace: Trace,
}
#[derive(Default)]
struct Trace {
    submissions: Cell<usize>,
}
impl Transport for Trace {
    fn call(&self, config: &Config, args: &[String]) -> crewmoor::Result<Value> {
        if args.get(1).map(String::as_str) == Some("prompt") {
            self.submissions.set(self.submissions.get() + 1);
        }
        Native.call(config, args)
    }
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let dispatcher = register(dir.path(), "dispatcher", "dispatcher-session");
        let worker = register(dir.path(), "worker", "worker-session");
        let config = Config {
            runtime: Runtime::Tui,
            core_id: None,
            core_version: None,
            runtime_binary: env!("CARGO_BIN_EXE_transport-fixture").into(),
            socket,
            feedback_registry: dir.path().into(),
            dispatcher: Peer {
                instance: dispatcher,
                handle: "w1:p1".into(),
                agent_executable: None,
            },
            worker: Peer {
                instance: worker,
                handle: "w1:p2".into(),
                agent_executable: None,
            },
        };
        let state = json!({"w1:p1":agent("w1:p1","one","dispatcher-session"),"w1:p2":agent("w1:p2","two","worker-session")});
        fs::write(
            config.socket.with_extension("json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let binding = Binding::bind(config, &Native).unwrap();
        Self {
            dir,
            _socket: listener,
            binding,
            trace: Trace::default(),
        }
    }
    fn report(&self, summary: &str) -> crewmoor::Result<Value> {
        feedback::report_to(
            &self.binding,
            "worker-session",
            "agent",
            "progress",
            summary,
            &self.trace,
        )
    }
    fn view(&self) -> Value {
        feedback::show(self.dir.path(), &self.binding.worker.context.instance).unwrap()
    }
    fn change_context(&self, key: &str, value: Value) {
        let path = self
            .dir
            .path()
            .join("instances")
            .join(&self.binding.worker.context.instance)
            .join("context.json");
        let mut context: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        context["registration"][key] = value;
        fs::write(path, serde_json::to_vec(&context).unwrap()).unwrap();
    }
}
fn register(dir: &std::path::Path, role: &str, session: &str) -> String {
    feedback::register(
        dir,
        Registration {
            team: "forward-test".into(),
            role: role.into(),
            task: "issue6-step1".into(),
            cwd: dir.into(),
            agent: AgentKind::Codex,
            agent_version: "synthetic".into(),
            evidence: Evidence::Synthetic,
            native_session_id: Some(session.into()),
        },
    )
    .unwrap()
    .instance
}
fn agent(pane: &str, terminal: &str, session: &str) -> Value {
    json!({"agent":"codex","pane_id":pane,"terminal_id":terminal,"workspace_id":"w1",
        "agent_session":{"kind":"id","value":session},"agent_status":"working"})
}
fn fact(receipt: &Value) -> Value {
    serde_json::from_str(
        receipt["delivery"]["envelope"]["message"]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn system_forwards_exact_new_record_with_fixed_identity_and_no_unrelated_history() {
    let fixture = Fixture::new();
    feedback::report(
        fixture.dir.path(),
        &fixture.binding.worker.context.instance,
        "worker-session",
        "agent",
        "progress",
        "PRIVATE_OLD_HISTORY",
    )
    .unwrap();
    let receipt = fixture
        .report("Implementation checked; actual feedback forward underway")
        .unwrap();
    assert_eq!(receipt["recorded"], true);
    assert_eq!(receipt["delivery"]["transport"], "accepted");
    assert_eq!(receipt["delivery"]["envelope"]["message"]["from"], "worker");
    assert_eq!(
        receipt["delivery"]["envelope"]["message"]["to"],
        "dispatcher"
    );
    let fact = fact(&receipt);
    let view = fixture.view();
    assert_eq!(
        fact["feedback_instance"],
        fixture.binding.worker.context.instance
    );
    assert_eq!(fact["task"], "issue6-step1");
    assert_eq!(fact["agent"], "codex");
    assert_eq!(fact["evidence"], "synthetic");
    assert_eq!(fact["record"], view["events"][1]);
    assert_eq!(fact["record"]["source"], "agent-report");
    assert_eq!(fact["record"]["event"], "progress");
    assert_eq!(fact["record"]["native_session_id"], "worker-session");
    assert_eq!(view["agent_state"], "unknown");
    assert!(view["latest_native_event"].is_null());
    assert!(!fact.to_string().contains("PRIVATE_OLD_HISTORY"));
    assert_eq!(fixture.trace.submissions.get(), 1);
    let submitted: Value = serde_json::from_slice(
        &fs::read(fixture.binding.config.socket.with_extension("submitted")).unwrap(),
    )
    .unwrap();
    let actual: Value = serde_json::from_str(submitted["args"][4].as_str().unwrap()).unwrap();
    assert_eq!(actual, receipt["delivery"]["envelope"]);
}
#[test]
fn source_session_type_and_summary_errors_create_no_report_or_submission() {
    let fixture = Fixture::new();
    for (session, source, kind, summary) in [
        ("dispatcher-session", "agent", "progress", "text"),
        ("worker-session", "screen", "progress", "text"),
        ("worker-session", "agent", "stopped", "text"),
        ("worker-session", "agent", "progress", ""),
    ] {
        assert!(
            feedback::report_to(
                &fixture.binding,
                session,
                source,
                kind,
                summary,
                &fixture.trace
            )
            .is_err()
        );
    }
    let view = fixture.view();
    let events = view["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["source"], feedback::REJECTED);
    assert_eq!(events[0]["native_session_id"], "dispatcher-session");
    assert_eq!(view["latest_business_report"], Value::Null);
    assert_eq!(fixture.trace.submissions.get(), 0);
}
#[test]
fn changed_fixed_worker_context_is_rejected_without_writing_or_forwarding() {
    for key in ["team", "role", "task", "native_session_id"] {
        let fixture = Fixture::new();
        fixture.change_context(key, json!("changed"));
        let error = fixture
            .report("intentional report")
            .unwrap_err()
            .to_string();
        assert!(error.contains("report not recorded or forwarded"));
        assert_eq!(fixture.view()["events"], json!([]));
        assert_eq!(fixture.trace.submissions.get(), 0);
    }
}
#[test]
fn recorded_feedback_survives_uncertain_submission_without_retry_or_replay() {
    let fixture = Fixture::new();
    let path = fixture.binding.config.socket.with_extension("json");
    let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    state["prompt_failure"] = json!(true);
    fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
    let error = fixture
        .report("Persist this actual report once")
        .unwrap_err()
        .to_string();
    assert!(error.contains("feedback recorded"));
    assert!(error.contains("forwarding unconfirmed"));
    assert!(error.contains("do not repeat report-to or replay"));
    assert_eq!(fixture.trace.submissions.get(), 1);
    assert_eq!(fixture.view()["events"].as_array().unwrap().len(), 1);
}
#[test]
fn public_cli_records_and_transfers_an_explicit_skill_report_through_same_entry() {
    let fixture = Fixture::new();
    let binding = fixture.dir.path().join("binding.json");
    fs::write(&binding, serde_json::to_vec(&fixture.binding).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "feedback",
            "report-to",
            binding.to_str().unwrap(),
            "worker-session",
            "skill",
            "progress",
            "Authored skill progress fact",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(fact(&receipt)["record"]["source"], "skill-report");
    assert_eq!(fixture.view()["events"][0], fact(&receipt)["record"]);
    assert!(feedback::cli(&["report-to".into()]).is_err());
}
