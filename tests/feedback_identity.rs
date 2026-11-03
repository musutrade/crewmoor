use crewmoor::{
    config::AgentKind,
    feedback::{self, Evidence, Registration},
};
use serde_json::{Value, json};
use std::path::Path;

#[path = "support/hook.rs"]
mod hook;

fn register(root: &Path, cwd: &Path, agent: AgentKind, session: Option<&str>) -> feedback::Context {
    feedback::register(
        root,
        Registration {
            team: "identity-test".into(),
            role: "worker".into(),
            task: "issue-39".into(),
            cwd: cwd.into(),
            agent,
            agent_version: "installed".into(),
            evidence: Evidence::Synthetic,
            native_session_id: session.map(String::from),
        },
    )
    .unwrap()
}
fn send(root: &Path, context: &feedback::Context, value: Value) -> crewmoor::Result<()> {
    feedback::ingest(
        root,
        &context.instance,
        &context.registration.agent,
        &serde_json::to_vec(&value).unwrap(),
    )
}
fn input(cwd: &Path, event: &str) -> Value {
    json!({"session_id":"native-one","cwd":cwd,"hook_event_name":event,
        "prompt":"PRIVATE_PROMPT","transcript_path":"/PRIVATE_TRANSCRIPT"})
}

struct NoCalls;
impl crewmoor::message::Transport for NoCalls {
    fn call(&self, _: &crewmoor::message::Config, _: &[String]) -> crewmoor::Result<Value> {
        panic!("unassociated input must never call the transport");
    }
}
impl crewmoor::issue::Gh for NoCalls {
    fn call(&self, _: &[String]) -> crewmoor::Result<Value> {
        panic!("unassociated input must never query issues");
    }
}

#[test]
fn both_wrong_agent_directions_are_outside_journal_scope_for_cli_and_direct_ingest() {
    for (registered, wrong) in [
        (AgentKind::Claude, AgentKind::Codex),
        (AgentKind::Codex, AgentKind::Claude),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let context = register(dir.path(), dir.path(), registered, None);
        let instance_dir = dir.path().join("instances").join(&context.instance);
        let valid = serde_json::to_vec(&input(dir.path(), "SessionStart")).unwrap();
        for bytes in [valid.as_slice(), b"malformed PRIVATE input"] {
            assert!(feedback::ingest(dir.path(), &context.instance, &wrong, bytes).is_err());
            let outcome = feedback::process_hook_event(
                wrong.name(),
                dir.path(),
                &context.instance,
                bytes,
                &NoCalls,
                &NoCalls,
            );
            assert!(!outcome.recorded);
            assert!(outcome.forward_error.is_none());
            assert!(outcome.delivery_error.is_none());
        }
        // Keep stdin open: completing this subprocess proves preflight never reads it.
        let result = hook::run(
            wrong.name(),
            dir.path(),
            &[(feedback::INSTANCE_ENV, &context.instance)],
            None,
        );
        assert!(result.status.success());
        assert_eq!(result.stdout, b"{}\n");
        assert!(!instance_dir.join("events.jsonl").exists());
        assert!(!instance_dir.join("native-session.json").exists());
        let before = feedback::show(dir.path(), &context.instance).unwrap();
        assert!(before["events"].as_array().unwrap().is_empty());

        // Once the actual Agent binds, the same foreign calls cannot append or change it.
        send(dir.path(), &context, input(dir.path(), "SessionStart")).unwrap();
        let journal = std::fs::read(instance_dir.join("events.jsonl")).unwrap();
        let session = std::fs::read(instance_dir.join("native-session.json")).unwrap();
        assert!(feedback::ingest(dir.path(), &context.instance, &wrong, &valid).is_err());
        assert!(feedback::ingest(dir.path(), &context.instance, &wrong, b"{").is_err());
        assert!(
            hook::run(
                wrong.name(),
                dir.path(),
                &[(feedback::INSTANCE_ENV, &context.instance)],
                None
            )
            .status
            .success()
        );
        assert_eq!(
            std::fs::read(instance_dir.join("events.jsonl")).unwrap(),
            journal
        );
        assert_eq!(
            std::fs::read(instance_dir.join("native-session.json")).unwrap(),
            session
        );
        feedback::require_unchanged(dir.path(), &context).unwrap();

        // Matching-Agent malformed input remains a visible, sanitized refusal.
        assert!(
            feedback::ingest(
                dir.path(),
                &context.instance,
                &context.registration.agent,
                b"PRIVATE malformed"
            )
            .is_err()
        );
        let view = feedback::show(dir.path(), &context.instance).unwrap();
        assert_eq!(
            view["latest_hook_rejection"]["native"],
            json!({"reason":"invalid-input"})
        );
        assert!(!view.to_string().contains("PRIVATE"));
        assert_eq!(view["latest_native_event"]["event"], "SessionStart");
        assert!(view["latest_session_rejection"].is_null());
    }
}

#[test]
fn unregistered_markers_create_no_instance_or_records_for_either_agent() {
    let dir = tempfile::tempdir().unwrap();
    for agent in [AgentKind::Claude, AgentKind::Codex] {
        for marker in ["invalid-instance", "c0a9560103b04e6cbe97cf3c5832db13"] {
            let result = hook::run(
                agent.name(),
                dir.path(),
                &[(feedback::INSTANCE_ENV, marker)],
                None,
            );
            assert!(result.status.success());
            assert_eq!(result.stdout, b"{}\n");
            assert!(feedback::ingest(dir.path(), marker, &agent, b"PRIVATE input").is_err());
        }
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn only_initial_main_sessionstart_needs_registration_cwd_then_identity_survives_cd() {
    for agent in [AgentKind::Claude, AgentKind::Codex] {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().join("launch");
        let child = cwd.join("child");
        std::fs::create_dir_all(&child).unwrap();
        let context = register(dir.path(), &cwd, agent, None);
        assert!(send(dir.path(), &context, input(dir.path(), "SessionStart")).is_err());
        for event in ["Stop", "UserPromptSubmit"] {
            assert!(send(dir.path(), &context, input(&cwd, event)).is_err());
        }
        assert!(
            feedback::registered_session(dir.path(), &context)
                .unwrap()
                .is_none()
        );
        send(dir.path(), &context, input(&cwd, "SessionStart")).unwrap();
        for changed in [
            dir.path().to_path_buf(),
            child,
            cwd.join("no-longer-exists"),
        ] {
            send(dir.path(), &context, input(&changed, "Stop")).unwrap();
        }
        let view = feedback::show(dir.path(), &context.instance).unwrap();
        assert_eq!(
            view["events"][0]["native"]["reason"],
            "initial-cwd-mismatch"
        );
        assert_eq!(view["events"][1]["native"]["reason"], "session-unbound");
        assert_eq!(view["events"][2]["native"]["reason"], "session-unbound");
        assert_eq!(view["latest_native_event"]["event"], "Stop");
        assert_eq!(
            feedback::registered_session(dir.path(), &context)
                .unwrap()
                .as_deref(),
            Some("native-one")
        );
        feedback::require_unchanged(dir.path(), &context).unwrap();
        let mut foreign = input(dir.path(), "Stop");
        foreign["session_id"] = json!("foreign-instance-session");
        assert!(send(dir.path(), &context, foreign).is_err());
        let view = feedback::show(dir.path(), &context.instance).unwrap();
        assert_eq!(
            view["latest_session_rejection"]["native"]["associated_session"],
            "native-one"
        );
        assert_eq!(
            view["latest_native_event"]["native_session_id"],
            "native-one"
        );
        assert!(feedback::require_unchanged(dir.path(), &context).is_err());
        assert_eq!(
            feedback::registered_session(dir.path(), &context)
                .unwrap()
                .as_deref(),
            Some("native-one")
        );
    }
}

#[test]
fn preknown_session_clear_from_another_cwd_is_a_session_halt_without_adoption() {
    let dir = tempfile::tempdir().unwrap();
    let context = register(
        dir.path(),
        dir.path(),
        AgentKind::Claude,
        Some("native-one"),
    );
    let mut clear = input(Path::new("/"), "SessionStart");
    clear["source"] = json!("clear");
    clear["session_id"] = json!("cleared-session");
    assert!(send(dir.path(), &context, clear).is_err());
    let view = feedback::show(dir.path(), &context.instance).unwrap();
    let rejected = &view["latest_session_rejection"];
    assert_eq!(rejected["source"], feedback::REJECTED);
    assert_eq!(rejected["native"]["source"], "clear");
    assert_eq!(rejected["native"]["associated_session"], "native-one");
    assert!(view["latest_native_event"].is_null());
    assert!(view["latest_hook_rejection"].is_null());
    assert!(feedback::require_unchanged(dir.path(), &context).is_err());
    assert_eq!(
        feedback::registered_session(dir.path(), &context)
            .unwrap()
            .as_deref(),
        Some("native-one")
    );
}

#[test]
fn known_session_needs_no_cwd_enrollment_and_refusals_keep_only_fixed_facts() {
    let dir = tempfile::tempdir().unwrap();
    let context = register(
        dir.path(),
        dir.path(),
        AgentKind::Claude,
        Some("native-one"),
    );
    send(dir.path(), &context, input(Path::new("/"), "Stop")).unwrap();
    let mut bad_session = input(dir.path(), "Stop");
    bad_session["session_id"] = json!("PRIVATE\nSESSION");
    let mut bad_metadata = input(dir.path(), "Stop");
    bad_metadata["reason"] = json!("PRIVATE\nREASON");
    for (value, reason, event) in [
        (bad_session, "invalid-session", "Stop"),
        (bad_metadata, "invalid-metadata", "Stop"),
        (
            input(dir.path(), "PRIVATE_EVENT"),
            "uncollected-event",
            "unknown",
        ),
    ] {
        assert!(send(dir.path(), &context, value).is_err());
        let view = feedback::show(dir.path(), &context.instance).unwrap();
        let rejection = &view["latest_hook_rejection"];
        assert_eq!(rejection["event"], event);
        assert_eq!(rejection["native"], json!({"reason":reason}));
        assert_eq!(rejection["native_session_id"], "");
        assert!(rejection["summary"].is_null());
        assert!(view["latest_session_rejection"].is_null());
        assert!(view["latest_business_report"].is_null());
        assert_eq!(view["latest_native_event"]["event"], "Stop");
        assert!(!view.to_string().contains("PRIVATE"));
        feedback::require_unchanged(dir.path(), &context).unwrap();
    }
}

#[test]
fn oversized_native_callback_is_neutral_and_leaves_a_visible_fixed_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let context = register(dir.path(), dir.path(), AgentKind::Claude, None);
    let result = hook::run(
        "claude",
        dir.path(),
        &[(feedback::INSTANCE_ENV, &context.instance)],
        Some(&vec![b'x'; 1_048_577]),
    );
    assert!(result.status.success());
    assert_eq!(result.stdout, b"{}\n");
    let view = feedback::show(dir.path(), &context.instance).unwrap();
    assert_eq!(
        view["latest_hook_rejection"]["native"]["reason"],
        "input-too-large"
    );
    assert!(view["latest_native_event"].is_null());
    assert!(
        feedback::registered_session(dir.path(), &context)
            .unwrap()
            .is_none()
    );
}
