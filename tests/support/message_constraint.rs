use super::{Binding, Config, Fixture, Message, Native, Value, json, message, request};
use crewmoor::{Result, message::Transport, team::update_constraint};
use std::{
    fs,
    process::Command,
    sync::{Barrier, Mutex},
};

struct Controlled {
    prompts: Mutex<Vec<Value>>,
    mode: &'static str,
}
impl Controlled {
    fn new(mode: &'static str) -> Self {
        Self {
            prompts: Mutex::new(Vec::new()),
            mode,
        }
    }
    fn count(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }
}
impl Transport for Controlled {
    fn call(&self, config: &Config, args: &[String]) -> Result<Value> {
        if args[..2] == ["agent", "prompt"] {
            self.prompts
                .lock()
                .unwrap()
                .push(serde_json::from_str(&args[3]).unwrap());
            match self.mode {
                "failed" => return Err(message::NotStarted.into()),
                "unknown" => return Err("synthetic lost response".into()),
                "save-during-submit" => {
                    update_constraint(&config.feedback_registry, "worker", "new")?
                }
                _ => {}
            }
        }
        Native.call(config, args)
    }
}
fn set(fixture: &Fixture, role: &str, text: &str) {
    update_constraint(fixture.dir.path(), role, text).unwrap();
}
fn assert_single(control: &Controlled, original: &Message, text: &str) {
    let prompts = control.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(
        prompts[0]["message"],
        serde_json::to_value(original).unwrap()
    );
    assert_eq!(prompts[0]["recipient_constraints"], text);
    assert!(prompts[0].get("feedback_view").is_none());
}

#[test]
fn both_directions_first_and_later_messages_preserve_originals_for_custom_roles() {
    let fixture = Fixture::new();
    let mut binding = fixture.binding();
    binding.dispatcher.context.registration.role = "coordinator".into();
    binding.worker.context.registration.role = "builder".into();
    super::running_team::configure(
        &mut binding.config,
        "message-test",
        "coordinator",
        "builder",
    );
    for (from, to, text) in [
        ("coordinator", "builder", "  中文 \"quote\" \\路径\nnext  "),
        ("builder", "coordinator", "{\"feedback_view\":{}}"),
    ] {
        set(&fixture, to, text);
        for reply_to in [None, Some("earlier-report".into())] {
            let original = Message {
                from: from.into(),
                to: to.into(),
                reply_to,
                ..request()
            };
            let control = Controlled::new("");
            let value = serde_json::to_value(message::send(&binding, &original, &control).unwrap())
                .unwrap();
            assert_eq!(value["transport"], "accepted");
            assert!(value.get("constraint").is_none());
            assert_single(&control, &original, text);
        }
    }
}

#[test]
fn absent_empty_whitespace_and_cleared_config_always_emit_empty_string() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    for text in [None, Some("configured"), Some(""), Some(" \t\r\n　")] {
        if let Some(text) = text {
            set(&fixture, "worker", text);
        }
        let control = Controlled::new("");
        let expected = text.filter(|s| !s.trim().is_empty()).unwrap_or("");
        let original = request();
        message::send(&binding, &original, &control).unwrap();
        assert_single(&control, &original, expected);
    }
}

#[test]
fn one_config_read_per_submission_and_next_send_sees_saved_value_without_rebind() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    set(&fixture, "worker", "old");
    set(&fixture, "dispatcher", "unchanged");
    let original = request();
    let control = Controlled::new("save-during-submit");
    message::send(&binding, &original, &control).unwrap();
    assert_single(&control, &original, "old");
    let control = Controlled::new("");
    message::send(&binding, &original, &control).unwrap();
    assert_single(&control, &original, "new");
    let reply = Message {
        from: "worker".into(),
        to: "dispatcher".into(),
        ..request()
    };
    let receipt = message::send(&binding, &reply, &Native).unwrap();
    assert_eq!(receipt.envelope.recipient_constraints, "unchanged");
}

#[test]
fn source_errors_never_fall_back_to_legacy_snapshot_or_submit_business() {
    let fixture = Fixture::new();
    let mut binding = fixture.binding();
    binding.config.worker_constraint = Some("obsolete snapshot".into());
    let path = binding.config.team_config.clone().unwrap();
    let valid: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for field in [
        "team",
        "dispatcher",
        "runtime",
        "feedback_registry",
        "missing-role",
        "duplicate-role",
        "oversize",
    ] {
        let mut document = valid.clone();
        match field {
            "missing-role" => {
                document["roles"].as_array_mut().unwrap().pop();
            }
            "duplicate-role" => {
                document["roles"]
                    .as_array_mut()
                    .unwrap()
                    .push(valid["roles"][1].clone());
            }
            "oversize" => document["roles"][1]["message_constraint"] = json!("x".repeat(16_385)),
            "runtime" => document[field] = json!("core"),
            _ => document[field] = json!("wrong"),
        }
        fs::write(&path, document.to_string()).unwrap();
        let control = Controlled::new("");
        assert!(
            message::send(&binding, &request(), &control).is_err(),
            "{field}"
        );
        assert_eq!(control.count(), 0);
    }
    for bytes in [b"{".as_slice(), b"null".as_slice()] {
        fs::write(&path, bytes).unwrap();
        let control = Controlled::new("");
        assert!(message::send(&binding, &request(), &control).is_err());
        assert_eq!(control.count(), 0);
    }
    fs::remove_file(path).unwrap();
    let control = Controlled::new("");
    assert!(message::send(&binding, &request(), &control).is_err());
    assert_eq!(control.count(), 0);
    binding.config.team_config = Some(fixture.dir.path().into());
    assert!(message::send(&binding, &request(), &control).is_err());
    assert_eq!(control.count(), 0);
    binding.config.team_config = None;
    let error = message::send(&binding, &request(), &control)
        .unwrap_err()
        .to_string();
    assert!(error.contains("message source BINDING TEAM_DIR"));
    assert_eq!(control.count(), 0);
}

#[test]
fn source_cli_only_adds_source_preserving_sessions_routes_and_legacy_text() {
    let fixture = Fixture::new();
    let mut binding = fixture.binding();
    set(&fixture, "worker", "current");
    binding.config.team_config = None;
    binding.config.worker_constraint = Some("old snapshot".into());
    let before = serde_json::to_value(&binding).unwrap();
    let path = fixture.dir.path().join("legacy.json");
    fs::write(&path, before.to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "message",
            "source",
            path.to_str().unwrap(),
            fixture.dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut after: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let binding: Binding = serde_json::from_value(after.clone()).unwrap();
    after["config"]
        .as_object_mut()
        .unwrap()
        .remove("team_config");
    assert_eq!(after, before);
    let original = request();
    let receipt = message::send(&binding, &original, &Native).unwrap();
    assert_eq!(receipt.envelope.recipient_constraints, "current");
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "message",
            "source",
            path.to_str().unwrap(),
            fixture.dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn utf8_body_and_constraint_limits_are_independent_and_validated_before_submit() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    for text in ["x".repeat(16_384), format!("{}x", "中".repeat(5461))] {
        set(&fixture, "worker", &text);
        let original = Message {
            content: text.clone(),
            ..request()
        };
        let control = Controlled::new("");
        message::send(&binding, &original, &control).unwrap();
        assert_single(&control, &original, &text);
    }
    for text in ["x".repeat(16_385), "中".repeat(5462)] {
        let before = fs::read(binding.config.team_config.as_ref().unwrap()).unwrap();
        assert!(update_constraint(fixture.dir.path(), "worker", &text).is_err());
        assert_eq!(
            fs::read(binding.config.team_config.as_ref().unwrap()).unwrap(),
            before
        );
        let original = Message {
            content: text,
            ..request()
        };
        let control = Controlled::new("");
        assert!(message::send(&binding, &original, &control).is_err());
        assert_eq!(control.count(), 0);
    }
}

#[test]
fn constraint_cli_saves_utf8_text_and_rejects_invalid_edits_without_changing_config() {
    let fixture = Fixture::new();
    let path = fixture.config.team_config.as_ref().unwrap();
    let before: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let text = "  中文 \"text\" \\path\nsecond  ";
    let input = fixture.dir.path().join("text.txt");
    fs::write(&input, text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "team",
            "constraint",
            fixture.dir.path().to_str().unwrap(),
            "worker",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut after: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(after["roles"][1]["message_constraint"], text);
    after["roles"][1]
        .as_object_mut()
        .unwrap()
        .remove("message_constraint");
    assert_eq!(after, before);
    let saved = fs::read(path).unwrap();
    assert!(update_constraint(fixture.dir.path(), "missing", "text").is_err());
    assert_eq!(fs::read(path).unwrap(), saved);
    fs::write(&input, "x".repeat(16_385)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .args([
            "team",
            "constraint",
            fixture.dir.path().to_str().unwrap(),
            "worker",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(path).unwrap(), saved);
}

#[test]
fn native_failed_unknown_and_accepted_are_single_submission_without_followup_or_retry() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    set(&fixture, "worker", "constraint");
    for mode in ["failed", "unknown", ""] {
        let control = Controlled::new(mode);
        let original = request();
        let result = message::send(&binding, &original, &control);
        match mode {
            "failed" => assert!(result.unwrap_err().is::<message::NotStarted>()),
            "unknown" => assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .starts_with(message::SUBMISSION_UNCONFIRMED)
            ),
            _ => assert_eq!(result.unwrap().transport, "accepted"),
        }
        assert_single(&control, &original, "constraint");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn json_expansion_is_checked_against_final_tui_argument_limit_before_submit() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    // Both raw fields are legal, but JSON encodes each byte as six ASCII bytes.
    let text = "\u{0001}".repeat(16_384);
    set(&fixture, "worker", &text);
    let original = Message {
        content: text,
        ..request()
    };
    let control = Controlled::new("");
    let result = message::send(&binding, &original, &control);
    let limit = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize * 32;
    if 16_384 * 12 > limit {
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("single-argument limit")
        );
        assert_eq!(control.count(), 0);
    } else {
        assert!(result.is_ok());
        assert_eq!(control.count(), 1);
    }
}

#[test]
fn atomic_constraint_saves_preserve_other_fields_and_concurrent_sends_read_whole_versions() {
    let fixture = Fixture::new();
    let binding = fixture.binding();
    set(&fixture, "dispatcher", "preserved");
    set(&fixture, "worker", "old");
    let before: Value =
        serde_json::from_slice(&fs::read(binding.config.team_config.as_ref().unwrap()).unwrap())
            .unwrap();
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            for n in 0..32 {
                set(&fixture, "worker", if n % 2 == 0 { "old" } else { "new" });
            }
        });
        barrier.wait();
        for _ in 0..8 {
            let original = request();
            let receipt = message::send(&binding, &original, &Native).unwrap();
            assert!(["old", "new"].contains(&receipt.envelope.recipient_constraints.as_str()));
        }
    });
    let mut after: Value =
        serde_json::from_slice(&fs::read(binding.config.team_config.as_ref().unwrap()).unwrap())
            .unwrap();
    assert_eq!(after["roles"][1]["message_constraint"], "new");
    after["roles"][1]["message_constraint"] = before["roles"][1]["message_constraint"].clone();
    assert_eq!(after, before);
}
