mod core_message_support;
use core_message_support::Fixture;
use crewmoor::{
    message::{self, Binding, Message, Native, Transport},
    native::strings,
};
use serde_json::json;
use std::{fs, process::Command};

fn request() -> Message {
    Message {
        id: "core-request-1".into(),
        from: "dispatcher".into(),
        to: "worker".into(),
        reply_to: None,
        content: "literal JSON quote \" newline\n中文 $(no shell)".into(),
    }
}
#[test]
fn native_socket_protocol_delivers_both_directions_without_cli_autostart_or_state_fields() {
    let fixture = Fixture::new();
    let binding = fixture.bind();
    assert_eq!(binding.config.runtime, crewmoor::config::Runtime::Core);
    assert!(binding.worker.terminal_id.is_none());
    let original = request();
    let receipt = message::send(&binding, &original, &Native).unwrap();
    assert_eq!(receipt.transport, "accepted");
    assert_eq!(receipt.native_turn, Some(0));
    let reply = Message {
        id: "core-reply-1".into(),
        from: "worker".into(),
        to: "dispatcher".into(),
        reply_to: Some("core-request-1".into()),
        content: "Agent-authored synthetic reply".into(),
    };
    message::send(&binding, &reply, &Native).unwrap();
    let sends = fixture.sends();
    assert_eq!(sends.len(), 2);
    assert_eq!(sends[0]["terminal"], "t1");
    assert_eq!(sends[1]["terminal"], "t0");
    assert_eq!(sends[0]["force"], false);
    assert_eq!(sends[0]["wait"], false);
    assert!(sends[0].get("if").is_none());
    assert!(sends[0].get("caller").is_none());
    let wire: serde_json::Value = serde_json::from_str(sends[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(wire["message"]["content"], request().content);
    assert_eq!(wire["team"], "core-message-test");
    let view =
        crewmoor::feedback::show(fixture.dir.path(), &binding.worker.context.instance).unwrap();
    assert_eq!(view["agent_state"], "unknown");
    assert!(view["latest_native_event"].is_null());
}
#[test]
fn changed_native_terminal_or_core_identity_refuses_before_send() {
    for (field, value) in [
        ("address", json!("t-other")),
        ("created_ms", json!(99)),
        ("argv", json!(["/bin/false"])),
        ("pid", json!(0)),
        ("cwd", json!("/")),
    ] {
        let fixture = Fixture::new();
        let binding = fixture.bind();
        fixture.state.lock().unwrap()["t1"][field] = value;
        assert!(message::send(&binding, &request(), &Native).is_err());
        assert!(fixture.sends().is_empty());
    }
    for (field, value) in [
        ("core_id", json!("changed")),
        ("version", json!("different")),
    ] {
        let fixture = Fixture::new();
        let binding = fixture.bind();
        fixture.state.lock().unwrap()[field] = value;
        assert!(message::send(&binding, &request(), &Native).is_err());
        assert!(fixture.sends().is_empty());
    }
}
#[test]
fn native_rejection_lost_reply_and_invalid_acceptance_never_resend() {
    for mode in ["blocked", "lost", "missing-turn", "changed-after-send"] {
        let fixture = Fixture::new();
        let binding = fixture.bind();
        fixture.state.lock().unwrap()["mode"] = json!(mode);
        let error = message::send(&binding, &request(), &Native)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unconfirmed"));
        assert!(error.contains("do not automatically resend"));
        assert_eq!(fixture.sends().len(), 1);
        if mode == "blocked" {
            assert!(error.contains("blocked"));
            assert!(error.contains("native dialog needs attention"));
        }
    }
}
#[test]
fn core_binding_needs_local_address_session_association_and_real_executable() {
    let fixture = Fixture::new();
    let original = serde_json::to_value(&fixture.config).unwrap();
    for (field, value) in [("core_id", json!(null)), ("core_version", json!(null))] {
        let mut config = original.clone();
        config[field] = value;
        assert!(Binding::bind(serde_json::from_value(config).unwrap(), &Native).is_err());
    }
    for (field, value) in [
        ("handle", json!("machine/t1")),
        ("handle", json!("alias")),
        ("agent_executable", json!(null)),
        ("agent_executable", json!("/bin/false")),
    ] {
        let mut config = original.clone();
        config["worker"][field] = value;
        assert!(Binding::bind(serde_json::from_value(config).unwrap(), &Native).is_err());
    }
    let context = fixture
        .dir
        .path()
        .join("instances")
        .join(&fixture.config.worker.instance)
        .join("context.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&context).unwrap()).unwrap();
    value["registration"]["native_session_id"] = json!(null);
    fs::write(context, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Binding::bind(serde_json::from_value(original).unwrap(), &Native).is_err());
    assert!(fixture.sends().is_empty());
}
#[test]
fn endpoint_never_starts_services_routes_remotely_or_forces_state() {
    let fixture = Fixture::new();
    for args in [
        strings(&["server", "start"]),
        strings(&["get", "elsewhere/t1"]),
        strings(&["send", "elsewhere/t1", "message"]),
        strings(&["kill", "t1"]),
    ] {
        assert!(Native.call(&fixture.config, &args).is_err());
    }
    let mut config: message::Config =
        serde_json::from_value(serde_json::to_value(&fixture.config).unwrap()).unwrap();
    config.socket = fixture.dir.path().join("absent.sock");
    assert!(Native.call(&config, &strings(&["get", "t1"])).is_err());
    assert!(!config.socket.exists());
    assert!(fixture.sends().is_empty());
}
#[test]
fn public_cli_core_binding_and_send_share_the_existing_message_entry() {
    let fixture = Fixture::new();
    let config = fixture.dir.path().join("config.json");
    let binding = fixture.dir.path().join("binding.json");
    let message = fixture.dir.path().join("request.json");
    fs::write(&config, serde_json::to_vec(&fixture.config).unwrap()).unwrap();
    fs::write(&message, serde_json::to_vec(&request()).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "message",
            "bind",
            config.to_str().unwrap(),
            binding.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "message",
            "send",
            binding.to_str().unwrap(),
            message.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(receipt["transport"], "accepted");
    assert_eq!(fixture.sends().len(), 1);
}
