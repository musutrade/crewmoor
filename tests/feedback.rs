#[path = "support/hook.rs"]
mod hook;
use crewmoor::{
    config::AgentKind,
    feedback::{self, Evidence, Registration},
};
use serde_json::{Value, json};
use std::process::Command;

fn registration(dir: &std::path::Path) -> Registration {
    Registration {
        team: "feedback-test".into(),
        role: "developer".into(),
        task: "issue-4".into(),
        cwd: dir.to_path_buf(),
        agent: AgentKind::Claude,
        agent_version: "2.1.316 (Claude Code)".into(),
        evidence: Evidence::Synthetic,
        native_session_id: None,
    }
}
fn payload(dir: &std::path::Path, event: &str) -> Value {
    json!({"session_id":"claude-native-1","cwd":dir,"hook_event_name":event,
        "source":"startup","prompt":"SECRET_PROMPT","last_assistant_message":"SECRET_REPLY",
        "transcript_path":"/private/never-read","tool_input":{"key":"SECRET_TOOL"}})
}
#[test]
fn claude_native_events_and_explicit_business_reports_have_separate_meanings() {
    let dir = tempfile::tempdir().unwrap();
    let context = feedback::register(dir.path(), registration(dir.path())).unwrap();
    for name in [
        "SessionStart",
        "UserPromptSubmit",
        "Stop",
        "StopFailure",
        "SessionEnd",
    ] {
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Claude,
            &serde_json::to_vec(&payload(dir.path(), name)).unwrap(),
        )
        .unwrap();
    }
    let view = feedback::show(dir.path(), &context.instance).unwrap();
    assert_eq!(view["agent_state"], "unknown");
    assert!(view["latest_business_report"].is_null());
    let encoded = view.to_string();
    for private in [
        "SECRET_",
        "transcript_path",
        "last_assistant_message",
        "tool_input",
    ] {
        assert!(!encoded.contains(private));
    }
    feedback::report(
        dir.path(),
        &context.instance,
        "claude-native-1",
        "skill",
        "progress",
        "Review complete; tests still running",
    )
    .unwrap();
    feedback::report(
        dir.path(),
        &context.instance,
        "claude-native-1",
        "agent",
        "complete",
        "Implemented requested change and validated it",
    )
    .unwrap();
    let view = feedback::show(dir.path(), &context.instance).unwrap();
    assert_eq!(view["latest_business_report"]["source"], "agent-report");
    assert_eq!(view["latest_business_report"]["event"], "complete");
    assert_eq!(view["context"]["registration"]["evidence"], "synthetic");
}
#[test]
fn hook_callback_is_a_real_rust_subprocess_but_fixture_evidence_stays_synthetic() {
    let dir = tempfile::tempdir().unwrap();
    let context = feedback::register(dir.path(), registration(dir.path())).unwrap();
    let input = serde_json::to_vec(&payload(dir.path(), "SessionStart")).unwrap();
    let output = hook::run(
        "claude",
        dir.path(),
        &[(feedback::INSTANCE_ENV, &context.instance)],
        Some(&input),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
    let view = feedback::show(dir.path(), &context.instance).unwrap();
    assert_eq!(view["events"].as_array().unwrap().len(), 1);
}
#[test]
fn unassociated_global_hook_does_not_wait_for_input_or_create_records() {
    let dir = tempfile::tempdir().unwrap();
    // A global hook without context must not read stdin.
    let output = hook::run("claude", dir.path(), &[], None);
    assert!(output.status.success());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn mismatched_session_kind_cwd_and_unassociated_business_reports_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let context = feedback::register(dir.path(), registration(dir.path())).unwrap();
    assert!(
        feedback::report(
            dir.path(),
            &context.instance,
            "unbound",
            "agent",
            "complete",
            "not proven"
        )
        .is_err()
    );
    let input = payload(dir.path(), "SessionStart");
    assert!(
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Codex,
            &serde_json::to_vec(&input).unwrap()
        )
        .is_err()
    );
    feedback::ingest(
        dir.path(),
        &context.instance,
        &AgentKind::Claude,
        &serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    let mut other = payload(dir.path(), "Stop");
    other["session_id"] = json!("different");
    assert!(
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Claude,
            &serde_json::to_vec(&other).unwrap()
        )
        .is_err()
    );
    assert!(
        feedback::report(
            dir.path(),
            &context.instance,
            "different",
            "agent",
            "complete",
            "wrong session"
        )
        .is_err()
    );
    other["cwd"] = json!("/");
    assert!(
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Claude,
            &serde_json::to_vec(&other).unwrap()
        )
        .is_err()
    );
    assert!(
        feedback::report(
            dir.path(),
            &context.instance,
            "claude-native-1",
            "container",
            "complete",
            "wrong source"
        )
        .is_err()
    );
}

#[test]
fn codex_has_its_own_event_set_and_repeated_sessionstart_is_not_ready_or_a_new_instance() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = registration(dir.path());
    config.agent = AgentKind::Codex;
    config.agent_version = "codex-cli 0.166.0".into();
    let context = feedback::register(dir.path(), config).unwrap();
    for source in ["startup", "resume", "compact"] {
        let mut input = payload(dir.path(), "SessionStart");
        input["source"] = json!(source);
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Codex,
            &serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
    }
    for name in ["Stop", "Interrupt", "PermissionRequest"] {
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Codex,
            &serde_json::to_vec(&payload(dir.path(), name)).unwrap(),
        )
        .unwrap();
    }
    for unsupported in [
        "StopFailure",
        "Notification",
        "TaskCompleted",
        "SubagentStart",
        "SubagentStop",
    ] {
        assert!(
            feedback::ingest(
                dir.path(),
                &context.instance,
                &AgentKind::Codex,
                &serde_json::to_vec(&payload(dir.path(), unsupported)).unwrap()
            )
            .is_err()
        );
    }
    let view = feedback::show(dir.path(), &context.instance).unwrap();
    assert_eq!(view["context"]["instance"], context.instance);
    assert_eq!(view["events"].as_array().unwrap().len(), 6);
    assert_eq!(view["agent_state"], "unknown");
    assert!(view["latest_business_report"].is_null());
}
#[test]
fn unknown_marker_does_not_read_stdin_and_known_context_failure_never_blocks_agent() {
    let dir = tempfile::tempdir().unwrap();
    let unknown = [(feedback::INSTANCE_ENV, "not-registered")];
    assert!(
        hook::run("claude", dir.path(), &unknown, None)
            .status
            .success()
    );
    let context = feedback::register(dir.path(), registration(dir.path())).unwrap();
    // A Codex callback on a Claude context is refused before stdin is read.
    let known = [(feedback::INSTANCE_ENV, context.instance.as_str())];
    let mismatched = hook::run("codex", dir.path(), &known, None);
    assert!(mismatched.status.success());
    assert!(String::from_utf8_lossy(&mismatched.stderr).contains("not recorded"));
    let output = hook::run("claude", dir.path(), &known, Some(b""));
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("not recorded"));
    assert!(
        feedback::show(dir.path(), &context.instance).unwrap()["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn public_cli_register_report_show_keeps_business_source_and_reports_errors() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("registration.json");
    let mut registration = registration(dir.path());
    registration.native_session_id = Some("known-native-session".into());
    std::fs::write(&config, serde_json::to_vec(&registration).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args(["feedback", "register"])
        .arg(dir.path())
        .arg(&config)
        .output()
        .unwrap();
    assert!(output.status.success());
    let context: Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = context["instance"].as_str().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args(["feedback", "report"])
        .arg(dir.path())
        .args([
            id,
            "known-native-session",
            "skill",
            "ready",
            "I loaded the task conventions",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args(["feedback", "show"])
        .arg(dir.path())
        .arg(id)
        .output()
        .unwrap();
    assert!(output.status.success());
    let view: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(view["latest_native_event"].is_null());
    assert_eq!(view["latest_business_report"]["event"], "ready");
    for args in [
        vec!["feedback"],
        vec!["feedback", "nonsense"],
        vec!["feedback", "hooks"],
        vec!["feedback", "hooks", "nonsense"],
    ] {
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_crewmoor"))
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
#[test]
fn main_binding_requires_expected_session_and_subagent_facts_never_create_an_instance() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = registration(dir.path());
    config.native_session_id = Some("expected".into());
    let context = feedback::register(dir.path(), config).unwrap();
    let mut input = payload(dir.path(), "SessionStart");
    assert!(
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Claude,
            &serde_json::to_vec(&input).unwrap()
        )
        .is_err()
    );
    input["session_id"] = json!("expected");
    input["agent_id"] = json!("subagent-1");
    assert!(
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Claude,
            &serde_json::to_vec(&input).unwrap()
        )
        .is_err()
    );
    input.as_object_mut().unwrap().remove("agent_id");
    feedback::ingest(
        dir.path(),
        &context.instance,
        &AgentKind::Claude,
        &serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    input["hook_event_name"] = json!("SubagentStop");
    assert!(
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Claude,
            &serde_json::to_vec(&input).unwrap()
        )
        .is_err()
    );
    let view = feedback::show(dir.path(), &context.instance).unwrap();
    let events = view["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["source"], feedback::REJECTED);
    assert_eq!(events[0]["native"]["associated_session"], "expected");
    assert_eq!(events[1]["source"], "native-hook");
    assert_eq!(view["latest_business_report"], Value::Null);
}

#[test]
fn free_form_tool_failure_error_never_reaches_disk_or_show() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = registration(dir.path());
    config.native_session_id = Some("claude-native-1".into());
    let context = feedback::register(dir.path(), config).unwrap();
    for error in [
        Some("command failed: fake_secret=PRIVATE_TOOL_OUTPUT".to_owned()),
        Some("PRIVATE_TOOL_OUTPUT\n".repeat(512)),
        None,
    ] {
        let mut input = payload(dir.path(), "PostToolUseFailure");
        input["error"] = json!(error);
        input["error_details"] = json!("PRIVATE_DETAILS");
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Claude,
            &serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        let view = feedback::show(dir.path(), &context.instance).unwrap();
        let event = view["events"].as_array().unwrap().last().unwrap();
        assert_eq!(event["native"]["error_present"], error.is_some());
        assert!(event["native"].get("error").is_none());
        let disk = std::fs::read_to_string(
            dir.path()
                .join("instances")
                .join(&context.instance)
                .join("events.jsonl"),
        )
        .unwrap();
        assert!(!disk.contains("PRIVATE_"));
        assert!(!view.to_string().contains("PRIVATE_"));
    }
}
#[test]
fn stop_failure_error_accepts_only_documented_kinds_and_other_events_ignore_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = registration(dir.path());
    config.native_session_id = Some("claude-native-1".into());
    let context = feedback::register(dir.path(), config).unwrap();
    for (kind, expected) in [
        (Some("rate_limit"), "rate_limit"),
        (Some("cloud_credential_error"), "cloud_credential_error"),
        (Some("PRIVATE_API_ERROR=fake_secret"), "unknown"),
        (None, "unknown"),
    ] {
        let mut input = payload(dir.path(), "StopFailure");
        input["error"] = json!(kind);
        input["error_details"] = json!("PRIVATE_DETAILS");
        feedback::ingest(
            dir.path(),
            &context.instance,
            &AgentKind::Claude,
            &serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        let view = feedback::show(dir.path(), &context.instance).unwrap();
        assert_eq!(view["latest_native_event"]["native"]["error"], expected);
        assert!(view["latest_business_report"].is_null());
    }
    let mut input = payload(dir.path(), "Stop");
    input["error"] = json!("PRIVATE_UNEXPECTED_FIELD");
    feedback::ingest(
        dir.path(),
        &context.instance,
        &AgentKind::Claude,
        &serde_json::to_vec(&input).unwrap(),
    )
    .unwrap();
    let view = feedback::show(dir.path(), &context.instance).unwrap();
    assert!(view["latest_native_event"]["native"].get("error").is_none());
    let disk = std::fs::read_to_string(
        dir.path()
            .join("instances")
            .join(&context.instance)
            .join("events.jsonl"),
    )
    .unwrap();
    assert!(!disk.contains("PRIVATE_"));
    assert!(!view.to_string().contains("PRIVATE_"));
}
