mod core_message_support;
#[path = "support/rule_selection.rs"]
mod rule_selection;
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
fn core_cli_and_public_entry_each_submit_one_envelope_with_current_receiver_constraints() {
    let fixture = Fixture::new();
    crewmoor::team::update_constraint(fixture.dir.path(), "worker", "  core 约束\n\\path  ")
        .unwrap();
    crewmoor::team::update_constraint(fixture.dir.path(), "dispatcher", "dispatch only").unwrap();
    let binding = fixture.bind();
    let original = request();
    let receipt = message::send(&binding, &original, &Native).unwrap();
    let value = serde_json::to_value(receipt).unwrap();
    assert_eq!(value["native_turn"], 0);
    assert_eq!(
        value["envelope"]["recipient_constraints"],
        "  core 约束\n\\path  "
    );
    assert!(value.get("constraint").is_none());
    let binding_path = fixture.dir.path().join("binding.json");
    let message_path = fixture.dir.path().join("message.json");
    fs::write(&binding_path, serde_json::to_vec(&binding).unwrap()).unwrap();
    fs::write(&message_path, serde_json::to_vec(&original).unwrap()).unwrap();
    crewmoor::team::update_constraint(fixture.dir.path(), "worker", "new value").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "message",
            "send",
            binding_path.to_str().unwrap(),
            message_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["envelope"]["recipient_constraints"], "new value");
    assert!(value.get("constraint").is_none());
    let reply = Message {
        from: "worker".into(),
        to: "dispatcher".into(),
        ..request()
    };
    let receipt = message::send(&binding, &reply, &Native).unwrap();
    assert_eq!(receipt.envelope.recipient_constraints, "dispatch only");
    let sends = fixture.sends();
    assert_eq!(sends.len(), 3);
    assert_eq!(sends[0]["terminal"], "t1");
    assert_eq!(sends[1]["terminal"], "t1");
    assert_eq!(sends[2]["terminal"], "t0");
    let first: serde_json::Value =
        serde_json::from_str(sends[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(first["message"], serde_json::to_value(original).unwrap());
}

#[test]
fn cli_unknown_submission_has_no_accepted_or_constraint_receipt_and_never_retries() {
    let fixture = Fixture::new();
    crewmoor::team::update_constraint(fixture.dir.path(), "worker", "constraint").unwrap();
    let binding = fixture.bind();
    fixture.state.lock().unwrap()["mode"] = json!("lost");
    let binding_path = fixture.dir.path().join("binding.json");
    let message_path = fixture.dir.path().join("message.json");
    fs::write(&binding_path, serde_json::to_vec(&binding).unwrap()).unwrap();
    fs::write(&message_path, serde_json::to_vec(&request()).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "message",
            "send",
            binding_path.to_str().unwrap(),
            message_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("submission unconfirmed"));
    assert_eq!(fixture.sends().len(), 1);
}
#[test]
fn core_forwards_readable_hook_and_authored_report_with_unchanged_send_parameters() {
    let fixture = Fixture::new();
    crewmoor::team::update_constraint(
        fixture.dir.path(),
        "dispatcher",
        "dispatcher feedback constraint",
    )
    .unwrap();
    let binding = fixture.bind();
    let binding_path = fixture.dir.path().join("binding.json");
    fs::write(&binding_path, serde_json::to_vec(&binding).unwrap()).unwrap();
    crewmoor::feedback::write_binding_ref(
        fixture.dir.path(),
        &binding.worker.context.instance,
        &binding_path,
    )
    .unwrap();
    let report = crewmoor::feedback::report_to(
        &binding,
        "native-worker",
        "skill",
        "progress",
        "中文 \"引号\" \\path\n下一行",
        &Native,
    )
    .unwrap();
    let payload =
        json!({"session_id":"native-worker","hook_event_name":"Stop","cwd":fixture.dir.path()});
    let outcome = crewmoor::feedback::process_hook_event(
        "claude",
        fixture.dir.path(),
        &binding.worker.context.instance,
        &serde_json::to_vec(&payload).unwrap(),
        &Native,
    );
    assert!(outcome.recorded);
    assert!(outcome.forward_error.is_none());
    let sends = fixture.sends();
    assert_eq!(sends.len(), 2);
    assert_readable_core_feedback(&sends[0], "progress");
    assert_readable_core_feedback(&sends[1], "Stop");
    let wire: serde_json::Value = serde_json::from_str(sends[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(wire, report["delivery"]["envelope"]);
}

#[test]
fn core_custom_receiver_roles_use_current_constraints_for_codex_and_claude() {
    for (role, kind, content) in [
        (
            "builder",
            crewmoor::config::AgentKind::Codex,
            "Codex reminder",
        ),
        (
            "checker",
            crewmoor::config::AgentKind::Claude,
            "Claude reminder",
        ),
    ] {
        let mut fixture = Fixture::new();
        let registration = crewmoor::feedback::Registration {
            team: "core-message-test".into(),
            role: role.into(),
            task: "issue-64".into(),
            cwd: fixture.dir.path().into(),
            agent: kind,
            agent_version: "synthetic".into(),
            evidence: crewmoor::feedback::Evidence::Synthetic,
            native_session_id: Some(format!("native-{role}")),
        };
        fixture.config.worker.instance =
            crewmoor::feedback::register(fixture.dir.path(), registration)
                .unwrap()
                .instance;
        let path = fixture.config.team_config.as_ref().unwrap();
        let mut team: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        team["roles"][1]["role"] = json!(role);
        fs::write(path, team.to_string()).unwrap();
        crewmoor::team::update_constraint(fixture.dir.path(), role, content).unwrap();
        let binding = fixture.bind();
        let original = Message {
            to: role.into(),
            ..request()
        };
        let receipt = message::send(&binding, &original, &Native).unwrap();
        assert_eq!(receipt.envelope.recipient_constraints, content);
        assert_eq!(fixture.sends().len(), 1);
    }
}
fn assert_readable_core_feedback(send: &serde_json::Value, event: &str) {
    assert_eq!(send["terminal"], "t0");
    assert_eq!(send["force"], false);
    assert_eq!(send["wait"], false);
    assert!(send.get("if").is_none());
    let text = send["text"].as_str().unwrap();
    if event == "Stop" {
        assert!(!text.contains('\n'));
    } else {
        assert!(text.contains("\"feedback_view\": {\n"));
    }
    let wire: serde_json::Value = serde_json::from_str(text).unwrap();
    let legacy: serde_json::Value =
        serde_json::from_str(wire["message"]["content"].as_str().unwrap()).unwrap();
    assert_eq!(
        wire["recipient_constraints"],
        "dispatcher feedback constraint"
    );
    assert_eq!(wire["feedback_view"]["version"], 1);
    assert_eq!(wire["feedback_view"]["fact"], legacy);
    assert_eq!(legacy["record"]["event"], event);
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
    // "changed-after-send": worker terminal identity changes between send and the post-send
    // check in submitted(); route() already verified both ends before send, so this is the
    // conservative second check documented on submitted(). Error must be SUBMISSION_UNCONFIRMED.
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
fn core_binding_accepts_only_the_fixed_child_environment_prefix_and_pins_its_full_argv() {
    let fixture = Fixture::new();
    for handle in ["t0", "t1"] {
        let mut argv = crewmoor::app::child_prefix(true);
        argv.extend([
            env!("CARGO_BIN_EXE_transport-fixture").to_owned(),
            "hold".into(),
        ]);
        fixture.state.lock().unwrap()[handle]["argv"] = json!(argv);
    }
    let binding = fixture.bind();
    message::send(&binding, &request(), &Native).unwrap();
    assert_eq!(fixture.sends().len(), 1);
    fixture.state.lock().unwrap()["t1"]["argv"][1] = json!("--ignore-environment");
    assert!(message::send(&binding, &request(), &Native).is_err());
    assert_eq!(fixture.sends().len(), 1);

    for change in ["unset", "program", "assignment", "feedback"] {
        let fixture = Fixture::new();
        let mut argv = crewmoor::app::child_prefix(true);
        match change {
            "unset" => {
                argv.remove(1);
            }
            "program" => {
                argv[0] = "/bin/true".into();
            }
            "assignment" => {
                argv.push("CODEX_THREAD_ID=wrong".into());
            }
            _ => {
                argv = crewmoor::app::child_prefix(false);
            }
        }
        argv.extend([
            env!("CARGO_BIN_EXE_transport-fixture").to_owned(),
            "hold".into(),
        ]);
        fixture.state.lock().unwrap()["t1"]["argv"] = json!(argv);
        let config =
            serde_json::from_value(serde_json::to_value(&fixture.config).unwrap()).unwrap();
        assert!(Binding::bind(config, &Native).is_err(), "{change}");
        assert!(fixture.sends().is_empty());
    }
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

/// Synthetic App-core metadata around real owned Rust fixture processes.
fn guarded_launch_argv(fixture: &Fixture, peer: &crewmoor::message::Peer) -> Vec<String> {
    let context: crewmoor::feedback::Context = serde_json::from_slice(
        &fs::read(
            fixture
                .dir
                .path()
                .join("instances")
                .join(&peer.instance)
                .join("context.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let root = fixture
        .dir
        .path()
        .join(format!("guard-{}", context.registration.role));
    fs::create_dir(&root).unwrap();
    let mut config: crewmoor::config::Config = serde_json::from_value(json!({
        "team":context.registration.team,"role":context.registration.role,"runtime":"core",
        "cwd":fixture.dir.path(),"store":fixture.dir.path(),"runtime_binary":"/bin/false",
        "runtime_version":"synthetic","agent":context.registration.agent,
        "agent_executable":peer.agent_executable,"agent_version":"synthetic",
        "feedback_registry":fixture.dir.path(),"feedback_instance":peer.instance
    }))
    .unwrap();
    config.tool_guard = Some(
        crewmoor::tool_policy::prepare(
            &config,
            &rule_selection::policy(fixture.dir.path()),
            &root,
            &context,
        )
        .unwrap(),
    );
    crewmoor::app::child_argv(&config)
}
#[test]
fn guarded_codex_and_claude_bind_through_public_core_entry_and_pin_all_rule_assignments() {
    let fixture = Fixture::new();
    for peer in [&fixture.config.dispatcher, &fixture.config.worker] {
        let argv = guarded_launch_argv(&fixture, peer);
        fixture.state.lock().unwrap()[&peer.handle]["argv"] = json!(argv);
    }
    let binding = fixture.bind();
    message::send(&binding, &request(), &Native).unwrap();
    assert_eq!(fixture.sends().len(), 1);
    let index = crewmoor::app::child_prefix(true).len();
    fixture.state.lock().unwrap()["t1"]["argv"][index] =
        json!("TOOL_RULE_HOOK_RULES=/different/rules.json");
    assert!(message::send(&binding, &request(), &Native).is_err());
    assert_eq!(fixture.sends().len(), 1);
}
fn rejects_core_guard_argv(fixture: &Fixture, argv: Vec<String>, change: &str) {
    fixture.state.lock().unwrap()["t1"]["argv"] = json!(argv);
    let config = serde_json::from_value(serde_json::to_value(&fixture.config).unwrap()).unwrap();
    assert!(Binding::bind(config, &Native).is_err(), "{change}");
}
#[test]
fn core_guard_prefix_rejects_unrelated_partial_duplicate_or_reordered_assignments() {
    let fixture = Fixture::new();
    let original = guarded_launch_argv(&fixture, &fixture.config.worker);
    let index = crewmoor::app::child_prefix(true).len();
    for change in [
        "unknown",
        "missing",
        "duplicate",
        "reordered",
        "extra",
        "no-fixed-prefix",
    ] {
        let mut argv = original.clone();
        match change {
            "unknown" => argv[index] = "LD_PRELOAD=/unrelated.so".into(),
            "missing" => {
                argv.remove(index + 1);
            }
            "duplicate" => argv[index + 1] = argv[index].clone(),
            "reordered" => argv.swap(index, index + 1),
            "extra" => argv.insert(index + 3, "CODEX_THREAD_ID=wrong".into()),
            _ => {
                argv.drain(..index);
            }
        }
        rejects_core_guard_argv(&fixture, argv, change);
    }
    assert!(fixture.sends().is_empty());
}
#[test]
fn core_guard_prefix_rejects_malformed_values_and_still_checks_the_agent_executable() {
    let fixture = Fixture::new();
    let original = guarded_launch_argv(&fixture, &fixture.config.worker);
    let index = crewmoor::app::child_prefix(true).len();
    for change in [
        "relative-rules",
        "relative-context",
        "bad-verifier",
        "other-verifier-mode",
        "relative-verifier",
        "wrong-verifier-args",
        "wrong-executable",
    ] {
        let mut argv = original.clone();
        match change {
            "relative-rules" => argv[index] = "TOOL_RULE_HOOK_RULES=relative.json".into(),
            "relative-context" => argv[index + 1] = "TOOL_RULE_HOOK_CONTEXT=relative.json".into(),
            "bad-verifier" => argv[index + 2] = "TOOL_RULE_HOOK_VERIFY=invalid JSON".into(),
            "other-verifier-mode" => {
                argv[index + 2] = format!(
                    "TOOL_RULE_HOOK_VERIFY={}",
                    json!([env!("CARGO_BIN_EXE_crewmoor"), "tool-policy", "hook"])
                )
            }
            "relative-verifier" => {
                argv[index + 2] = format!(
                    "TOOL_RULE_HOOK_VERIFY={}",
                    json!(["crewmoor", "tool-policy", "verify"])
                )
            }
            "wrong-verifier-args" => {
                argv[index + 2] = format!(
                    "TOOL_RULE_HOOK_VERIFY={}",
                    json!([
                        env!("CARGO_BIN_EXE_crewmoor"),
                        "tool-policy",
                        "verify",
                        "extra"
                    ])
                )
            }
            _ => argv[index + 3] = "/bin/false".into(),
        }
        rejects_core_guard_argv(&fixture, argv, change);
    }
    assert!(fixture.sends().is_empty());
}
