use super::{Binding, Config, Fixture, Message, Native, Value, json, message, request};
use crewmoor::{Result, message::Transport};
use std::sync::{
    Barrier, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Controlled {
    prompts: Mutex<Vec<Value>>,
    mode: &'static str,
    changed: AtomicBool,
    barrier: Option<Barrier>,
}
impl Controlled {
    fn new(mode: &'static str) -> Self {
        Self {
            prompts: Mutex::new(Vec::new()),
            mode,
            changed: AtomicBool::new(false),
            barrier: None,
        }
    }
    fn count(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }
    fn prompt(&self, config: &Config, args: &[String]) -> Result<Value> {
        let envelope: Value = serde_json::from_str(&args[3]).unwrap();
        self.prompts.lock().unwrap().push(envelope.clone());
        let nth = self.count();
        match (self.mode, nth) {
            ("a-failed", 1) | ("b-failed", 2) => return Err(message::NotStarted.into()),
            ("a-unknown", 1) | ("b-unknown", 2) => return Err("synthetic lost response".into()),
            _ => {}
        }
        let mut result = Native.call(config, args);
        if self.mode == "b-post-session" && nth == 2 {
            result.as_mut().unwrap()["agent"]["agent_session"]["value"] = json!("changed");
        }
        if self.mode == "b-interrupt" && nth == 2 {
            panic!("controlled interruption after B native submission, before outcome recording");
        }
        if envelope["message"]["reply_to"].is_null()
            && let Some(barrier) = &self.barrier
        {
            barrier.wait();
        }
        result
    }
    fn change(&self, config: &Config) {
        if self.mode == "interrupt" {
            panic!("controlled interruption after A acceptance and before B submission");
        }
        let path = config.socket.with_extension("json");
        let mut state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        match self.mode {
            "session" => state["w1:p2"]["agent_session"]["value"] = json!("changed"),
            "location" => state["w1:p2"]["terminal_id"] = json!("changed"),
            "process" => state["processes"]["w1:p2"] = json!([]),
            "socket" => {
                std::fs::rename(&config.socket, config.socket.with_extension("old")).unwrap();
                let _replacement = std::os::unix::net::UnixListener::bind(&config.socket).unwrap();
            }
            _ => {}
        }
        std::fs::write(path, state.to_string()).unwrap();
    }
}
impl Transport for Controlled {
    fn call(&self, config: &Config, args: &[String]) -> Result<Value> {
        if args[..2] == ["agent", "prompt"] {
            return self.prompt(config, args);
        }
        if matches!(
            self.mode,
            "socket" | "location" | "process" | "session" | "interrupt"
        ) && self.count() == 1
            && !self.changed.swap(true, Ordering::SeqCst)
        {
            self.change(config);
        }
        Native.call(config, args)
    }
}
fn constrained(fixture: &Fixture, content: &str) -> Binding {
    let mut binding = fixture.binding();
    binding.config.worker_constraint = Some(content.into());
    binding
}
fn assert_pair(control: &Controlled, original: &Message, content: &str) {
    let prompts = control.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert_eq!(
        prompts[0]["message"],
        serde_json::to_value(original).unwrap()
    );
    assert_eq!(prompts[1]["message"]["content"], content);
    assert_ne!(prompts[1]["message"]["id"], original.id);
    assert_eq!(prompts[1]["message"]["reply_to"], original.id);
    assert_eq!(prompts[1]["message"]["from"], original.from);
    assert_eq!(prompts[1]["message"]["to"], original.to);
    assert_eq!(prompts[0]["team"], prompts[1]["team"]);
    assert!(prompts[1].get("feedback_view").is_none());
}

#[test]
fn first_and_later_downlinks_with_custom_roles_preserve_exact_independent_messages() {
    let fixture = Fixture::new();
    for (role, content) in [
        ("builder", "  中文 \"quote\" \\路径\nnext  "),
        ("checker", "{\"feedback_view\":{}}"),
    ] {
        let mut binding = constrained(&fixture, content);
        binding.dispatcher.context.registration.role = "coordinator".into();
        binding.worker.context.registration.role = role.into();
        for reply_to in [None, Some("earlier-report".into())] {
            let original = Message {
                from: "coordinator".into(),
                to: role.into(),
                reply_to,
                ..request()
            };
            let control = Controlled::new("");
            let receipt = message::send(&binding, &original, &control).unwrap();
            assert_eq!(receipt.transport, "accepted");
            assert!(receipt.constraint.as_ref().unwrap().outcome.accepted());
            assert_pair(&control, &original, content);
        }
    }
}
#[test]
fn absent_empty_whitespace_legacy_and_uplink_have_no_followup() {
    let fixture = Fixture::new();
    for content in [None, Some(""), Some(" \t\r\n　")] {
        let mut binding = fixture.binding();
        binding.config.worker_constraint = content.map(Into::into);
        let control = Controlled::new("");
        assert!(
            message::send(&binding, &request(), &control)
                .unwrap()
                .constraint
                .is_none()
        );
        assert_eq!(control.count(), 1);
    }
    let mut legacy = serde_json::to_value(fixture.binding()).unwrap();
    legacy["config"]
        .as_object_mut()
        .unwrap()
        .remove("worker_constraint");
    let binding: Binding = serde_json::from_value(legacy).unwrap();
    assert!(binding.config.worker_constraint.is_none());
    let mut binding = constrained(&fixture, "downlink only");
    // Even an invalid unused downlink reminder must not affect the worker's report/halt.
    binding.config.worker_constraint = Some("x".repeat(16_385));
    let original = Message {
        from: "worker".into(),
        to: "dispatcher".into(),
        ..request()
    };
    let control = Controlled::new("");
    assert!(
        message::send(&binding, &original, &control)
            .unwrap()
            .constraint
            .is_none()
    );
    assert!(
        message::send_halt_to_dispatcher(&binding, &original, &control)
            .unwrap()
            .constraint
            .is_none()
    );
    assert_eq!(control.count(), 2);
}
#[test]
fn each_body_has_its_own_utf8_limit_and_config_errors_precede_a() {
    let fixture = Fixture::new();
    for text in ["x".repeat(16_384), format!("{}x", "中".repeat(5461))] {
        let binding = constrained(&fixture, &text);
        let original = Message {
            content: text.clone(),
            ..request()
        };
        let control = Controlled::new("");
        message::send(&binding, &original, &control).unwrap();
        assert_pair(&control, &original, &text);
    }
    for text in ["x".repeat(16_385), "中".repeat(5462)] {
        for invalid_b in [false, true] {
            let binding = constrained(&fixture, if invalid_b { &text } else { "valid" });
            let original = Message {
                content: if invalid_b {
                    "valid".into()
                } else {
                    text.clone()
                },
                ..request()
            };
            let control = Controlled::new("");
            assert!(message::send(&binding, &original, &control).is_err());
            assert_eq!(control.count(), 0);
        }
        let mut config = serde_json::to_value(&fixture.config).unwrap();
        config["worker_constraint"] = json!(text);
        assert!(Binding::bind(serde_json::from_value(config).unwrap(), &Native).is_err());
    }
}
#[test]
fn a_failure_or_uncertainty_stops_and_b_failure_retains_a_without_retry() {
    let fixture = Fixture::new();
    let binding = constrained(&fixture, "reminder");
    for mode in ["a-failed", "a-unknown"] {
        let control = Controlled::new(mode);
        assert!(message::send(&binding, &request(), &control).is_err());
        assert_eq!(control.count(), 1);
    }
    for (mode, status) in [
        ("b-failed", "failed"),
        ("b-unknown", "unknown"),
        ("b-post-session", "unknown"),
    ] {
        let control = Controlled::new(mode);
        let original = request();
        let receipt = message::send(&binding, &original, &control).unwrap();
        let value = serde_json::to_value(receipt).unwrap();
        assert_eq!(value["transport"], "accepted");
        assert_eq!(value["constraint"]["transport"], status);
        assert!(value["constraint"]["error"].is_string());
        assert_pair(&control, &original, "reminder");
    }
}
#[test]
fn b_rechecks_socket_location_process_and_session_without_redirecting() {
    for mode in ["socket", "location", "process", "session"] {
        let fixture = Fixture::new();
        let binding = constrained(&fixture, "reminder");
        let control = Controlled::new(mode);
        let original = request();
        let value =
            serde_json::to_value(message::send(&binding, &original, &control).unwrap()).unwrap();
        assert_eq!(value["transport"], "accepted", "{mode}");
        assert_eq!(value["constraint"]["transport"], "failed", "{mode}");
        assert_eq!(control.count(), 1);
    }
}
#[test]
fn interruption_does_not_report_b_and_concurrent_calls_may_interleave() {
    let fixture = Fixture::new();
    let binding = constrained(&fixture, "reminder");
    for (mode, attempts) in [("interrupt", 1), ("b-interrupt", 2)] {
        let control = Controlled::new(mode);
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _receipt = message::send(&binding, &request(), &control);
        }));
        assert!(interrupted.is_err());
        assert_eq!(control.count(), attempts);
    }
    let mut control = Controlled::new("");
    control.barrier = Some(Barrier::new(2));
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            message::send(&binding, &request(), &control)
                .unwrap()
                .transport
        });
        let second = scope.spawn(|| {
            let original = Message {
                id: "request-concurrent".into(),
                ..request()
            };
            message::send(&binding, &original, &control)
                .unwrap()
                .transport
        });
        assert_eq!(first.join().unwrap(), "accepted");
        assert_eq!(second.join().unwrap(), "accepted");
    });
    let prompts = control.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 4);
    assert!(prompts[0]["message"]["reply_to"].is_null());
    assert!(prompts[1]["message"]["reply_to"].is_null());
    assert!(prompts[2]["message"]["reply_to"].is_string());
    assert!(prompts[3]["message"]["reply_to"].is_string());
}
