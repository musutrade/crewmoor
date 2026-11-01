use crewmoor::{
    config::AgentKind,
    feedback::{self, Evidence, Registration},
    message::{self, Binding, Config, Message, Native, Peer},
};
use serde_json::{Value, json};
use std::{fs, os::unix::net::UnixListener, process::Command};

struct Fixture {
    dir: tempfile::TempDir,
    _socket: UnixListener,
    config: Config,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let dispatcher = register(dir.path(), "dispatcher", AgentKind::Codex, Some("codex-1"));
        let worker = register(dir.path(), "worker", AgentKind::Claude, Some("claude-1"));
        let config = Config {
            runtime_binary: env!("CARGO_BIN_EXE_transport-fixture").into(),
            socket,
            feedback_registry: dir.path().into(),
            dispatcher: Peer {
                instance: dispatcher,
                handle: "w1:p1".into(),
            },
            worker: Peer {
                instance: worker,
                handle: "w1:p2".into(),
            },
        };
        let result = Self {
            dir,
            _socket: listener,
            config,
        };
        result.state(&json!({
            "w1:p1": agent("codex", "codex-1", "w1:p1", "term_one"),
            "w1:p2": agent("claude", "claude-1", "w1:p2", "term_two")
        }));
        result
    }
    fn state(&self, value: &Value) {
        fs::write(
            self.config.socket.with_extension("json"),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    fn binding(&self) -> Binding {
        let config = serde_json::from_value(serde_json::to_value(&self.config).unwrap()).unwrap();
        Binding::bind(config, &Native).unwrap()
    }
    fn submitted(&self) -> Value {
        serde_json::from_slice(&fs::read(self.config.socket.with_extension("submitted")).unwrap())
            .unwrap()
    }
    fn no_submission(&self) {
        assert!(!self.config.socket.with_extension("submitted").exists());
    }
}
fn register(path: &std::path::Path, role: &str, kind: AgentKind, session: Option<&str>) -> String {
    feedback::register(
        path,
        Registration {
            team: "message-test".into(),
            role: role.into(),
            task: "issue-9".into(),
            cwd: path.into(),
            agent: kind,
            agent_version: "synthetic 1".into(),
            evidence: Evidence::Synthetic,
            native_session_id: session.map(Into::into),
        },
    )
    .unwrap()
    .instance
}
fn agent(kind: &str, session: &str, pane: &str, terminal: &str) -> Value {
    json!({"agent":kind,"pane_id":pane,"terminal_id":terminal,"workspace_id":"w1",
        "agent_session":{"source":"fixture","agent":kind,"kind":"id","value":session},
        "agent_status":"working"})
}
fn request() -> Message {
    Message {
        id: "request-9-1".into(),
        from: "dispatcher".into(),
        to: "worker".into(),
        reply_to: None,
        content: "literal quotes ' \" and $(printf nope)\n你好".into(),
    }
}
#[test]
fn same_entry_transports_request_and_explicit_reply_without_claiming_receipt_or_completion() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    let request = request();
    let receipt = message::send(&binding, &request, &Native).unwrap();
    assert_eq!(receipt.transport, "accepted");
    let submitted = fixture.submitted();
    assert_eq!(submitted["args"][2], "prompt");
    assert_eq!(submitted["args"][3], "w1:p2");
    let wire: Value = serde_json::from_str(submitted["args"][4].as_str().unwrap()).unwrap();
    assert_eq!(wire["message"]["content"], request.content);
    assert_eq!(wire["team"], "message-test");
    let reply = Message {
        id: "reply-9-1".into(),
        from: "worker".into(),
        to: "dispatcher".into(),
        reply_to: Some(request.id.clone()),
        content: "Received; explicit synthetic reply".into(),
    };
    message::send(&binding, &reply, &Native).unwrap();
    assert_eq!(fixture.submitted()["args"][3], "w1:p1");
    let view = feedback::show(fixture.dir.path(), &binding.worker.context.instance).unwrap();
    assert_eq!(view["agent_state"], "unknown");
    assert!(view["latest_native_event"].is_null());
    assert!(view["latest_business_report"].is_null());
}
#[test]
fn identity_changes_reject_before_prompt_for_each_original_identity_field() {
    for field in [
        "agent",
        "pane_id",
        "terminal_id",
        "workspace_id",
        "agent_session",
    ] {
        let fixture = Fixture::new();
        let binding = fixture.binding();
        let mut state = json!({"w1:p1":agent("codex","codex-1","w1:p1","term_one"),
            "w1:p2":agent("claude","claude-1","w1:p2","term_two")});
        state["w1:p2"][field] = json!("changed");
        fixture.state(&state);
        assert!(message::send(&binding, &request(), &Native).is_err());
        fixture.no_submission();
    }
}
#[test]
fn socket_replacement_and_native_failures_do_not_resend() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    fs::rename(
        &fixture.config.socket,
        fixture.config.socket.with_extension("old"),
    )
    .unwrap();
    let _replacement = UnixListener::bind(&fixture.config.socket).unwrap();
    assert!(message::send(&binding, &request(), &Native).is_err());
    fixture.no_submission();
    for mode in ["native-error", "bad-json"] {
        let fixture = Fixture::new();
        let binding = fixture.binding();
        fixture.state(&json!({"mode":mode}));
        assert!(message::send(&binding, &request(), &Native).is_err());
        fixture.no_submission();
    }
}
#[test]
fn invalid_role_correlation_and_content_are_rejected_before_transport() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    for (field, value) in [
        ("id", json!("")),
        ("from", json!("unknown")),
        ("to", json!("dispatcher")),
        ("reply_to", json!("request-9-1")),
        ("content", json!(" ")),
        ("content", json!("x".repeat(16_385))),
    ] {
        let mut value_message = serde_json::to_value(request()).unwrap();
        value_message[field] = value;
        let message = serde_json::from_value(value_message).unwrap();
        assert!(message::send(&binding, &message, &Native).is_err());
    }
    fixture.no_submission();
}
#[test]
fn binding_requires_one_fixed_team_pair_with_known_native_sessions() {
    let fixture = Fixture::new();
    for field in ["handle", "instance"] {
        let mut config = serde_json::to_value(&fixture.config).unwrap();
        config["worker"][field] = config["dispatcher"][field].clone();
        assert!(Binding::bind(serde_json::from_value(config).unwrap(), &Native).is_err());
    }
    let unknown = register(fixture.dir.path(), "unknown", AgentKind::Claude, None);
    let mut config = serde_json::to_value(&fixture.config).unwrap();
    config["worker"]["instance"] = json!(unknown);
    assert!(Binding::bind(serde_json::from_value(config).unwrap(), &Native).is_err());
    let mut config = serde_json::to_value(&fixture.config).unwrap();
    config["socket"] = json!("relative.sock");
    assert!(Binding::bind(serde_json::from_value(config).unwrap(), &Native).is_err());
    fixture.no_submission();
}
#[test]
fn different_feedback_teams_cannot_be_bound_together() {
    let fixture = Fixture::new();
    let context_path = fixture
        .dir
        .path()
        .join("instances")
        .join(&fixture.config.worker.instance)
        .join("context.json");
    let mut context: Value = serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
    context["registration"]["team"] = json!("other-team");
    fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
    let config = serde_json::from_value(serde_json::to_value(&fixture.config).unwrap()).unwrap();
    assert!(Binding::bind(config, &Native).is_err());
    fixture.no_submission();
}
#[test]
fn submission_error_or_changed_response_identity_stays_unconfirmed() {
    for (key, value) in [
        ("prompt_failure", json!(true)),
        ("prompt_session", json!("changed")),
    ] {
        let fixture = Fixture::new();
        let binding = fixture.binding();
        let mut state: Value = serde_json::from_slice(
            &fs::read(fixture.config.socket.with_extension("json")).unwrap(),
        )
        .unwrap();
        state[key] = value;
        fixture.state(&state);
        let error = message::send(&binding, &request(), &Native)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unconfirmed"));
        assert!(error.contains("do not automatically resend"));
        assert_eq!(fixture.submitted()["args"][2], "prompt");
    }
}
#[test]
fn public_cli_binds_once_and_uses_literal_argv_with_isolated_routing_environment() {
    let fixture = Fixture::new();
    let config_path = fixture.dir.path().join("config.json");
    let binding_path = fixture.dir.path().join("binding.json");
    let message_path = fixture.dir.path().join("message.json");
    fs::write(&config_path, serde_json::to_vec(&fixture.config).unwrap()).unwrap();
    fs::write(&message_path, serde_json::to_vec(&request()).unwrap()).unwrap();
    let bind_args = vec![
        "bind".into(),
        config_path.display().to_string(),
        binding_path.display().to_string(),
    ];
    message::cli(&bind_args).unwrap();
    assert!(message::cli(&bind_args).is_err());
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "message",
            "send",
            binding_path.to_str().unwrap(),
            message_path.to_str().unwrap(),
        ])
        .env("HERDR_SOCKET_PATH", "/not-the-bound-socket")
        .env("HERDR_SESSION", "other")
        .env("HERDR_PANE_ID", "other")
        .env("CODEX_SESSION_ID", "other")
        .env("HC_SOCKET", "other")
        .env(feedback::INSTANCE_ENV, "other")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["transport"], "accepted");
    assert_eq!(fixture.submitted()["inherited"], json!([]));
    for args in [vec![], vec!["unknown".into()], vec!["bind".into()]] {
        assert!(message::cli(&args).is_err());
    }
}
