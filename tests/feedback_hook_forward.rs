#[path = "support/agents.rs"]
mod agents;
#[path = "support/hook.rs"]
mod hook;
use crewmoor::{
    config::{AgentKind, Runtime},
    feedback::{self, Evidence, Registration},
    message::{Binding, Bounded, Config, Native, Peer, Transport},
};
use serde_json::{Value, json};
use std::{
    cell::Cell, fs, os::unix::net::UnixListener, path::Path, process::Command, time::Duration,
};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

struct Fixture {
    dir: tempfile::TempDir,
    _socket: UnixListener,
    binding: Binding,
    trace: Trace,
    _agents: agents::Agents,
    worker_instance: String,
    dispatcher_instance: String,
}

#[derive(Default)]
struct Trace {
    submissions: Cell<usize>,
    fail_prompt: Cell<bool>,
}

impl Transport for Trace {
    fn call(&self, config: &Config, args: &[String]) -> crewmoor::Result<Value> {
        if args.get(1).map(String::as_str) == Some("prompt") {
            self.submissions.set(self.submissions.get() + 1);
            if self.fail_prompt.get() {
                return Err("synthetic timeout".into());
            }
        }
        Native.call(config, args)
    }
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let dispatcher_instance = register(dir.path(), "dispatcher", "dispatcher-session");
        let worker_instance = register(dir.path(), "worker", "worker-session");
        let config = Config {
            runtime: Runtime::Tui,
            core_id: None,
            core_version: None,
            runtime_binary: env!("CARGO_BIN_EXE_transport-fixture").into(),
            socket: socket.clone(),
            feedback_registry: dir.path().into(),
            dispatcher: Peer {
                instance: dispatcher_instance.clone(),
                handle: "w1:p1".into(),
                agent_executable: Some(agents::EXECUTABLE.into()),
            },
            worker: Peer {
                instance: worker_instance.clone(),
                handle: "w1:p2".into(),
                agent_executable: Some(agents::EXECUTABLE.into()),
            },
        };
        let spawned = agents::Agents::spawn(dir.path(), &[("w1:p1", "codex"), ("w1:p2", "codex")]);
        let state = json!({
            "w1:p1": agent("w1:p1", "one", "dispatcher-session"),
            "w1:p2": agent("w1:p2", "two", "worker-session"),
            "processes": spawned.processes(),
        });
        fs::write(
            socket.with_extension("json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let binding = Binding::bind(config, &Native).unwrap();
        // Write binding-ref so the hook callback can forward events.
        let binding_path = dir.path().join("test-binding.json");
        fs::write(&binding_path, serde_json::to_vec_pretty(&binding).unwrap()).unwrap();
        feedback::write_binding_ref(dir.path(), &worker_instance, &binding_path).unwrap();
        Self {
            _socket: listener,
            binding,
            trace: Trace::default(),
            _agents: spawned,
            worker_instance,
            dispatcher_instance,
            dir,
        }
    }

    fn hook_event(&self, role: &str, session: &str, event: &str) -> feedback::HookOutcome {
        let instance = if role == "worker" {
            &self.worker_instance
        } else {
            &self.dispatcher_instance
        };
        let payload = json!({
            "session_id": session,
            "cwd": self.dir.path(),
            "hook_event_name": event,
        });
        feedback::process_hook_event(
            "codex",
            self.dir.path(),
            instance,
            &serde_json::to_vec(&payload).unwrap(),
            &self.trace,
        )
    }

    fn worker_hook(&self, event: &str) -> bool {
        self.hook_event("worker", "worker-session", event).recorded
    }

    fn worker_root(&self) -> std::path::PathBuf {
        self.dir
            .path()
            .join("instances")
            .join(&self.worker_instance)
    }

    fn set_state(&self, key: &str, value: Value) {
        let path = self.binding.config.socket.with_extension("json");
        let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        state[key] = value;
        fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
    }

    fn events(&self) -> Vec<Value> {
        feedback::show(self.dir.path(), &self.worker_instance).unwrap()["events"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn submissions(&self) -> usize {
        self.trace.submissions.get()
    }

    fn last_message_content(&self) -> Value {
        let submitted: Value = serde_json::from_slice(
            &fs::read(self.binding.config.socket.with_extension("submitted")).unwrap(),
        )
        .unwrap();
        serde_json::from_str(submitted["args"][4].as_str().unwrap())
            .map(|v: Value| {
                serde_json::from_str(v["message"]["content"].as_str().unwrap_or("{}"))
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    fn last_event(&self) -> Value {
        let view = feedback::show(self.dir.path(), &self.worker_instance).unwrap();
        view["events"]
            .as_array()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or(Value::Null)
    }
}

fn register(dir: &Path, role: &str, session: &str) -> String {
    feedback::register(
        dir,
        Registration {
            team: "hook-forward-test".into(),
            role: role.into(),
            task: "issue6-step3".into(),
            cwd: dir.to_path_buf(),
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
    json!({
        "agent": "codex",
        "pane_id": pane,
        "terminal_id": terminal,
        "workspace_id": "w1",
        "agent_session": {"kind": "id", "value": session},
        "agent_status": "working",
    })
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

#[test]
fn only_bound_worker_events_are_forwarded_unbound_instance_is_skipped() {
    let fixture = Fixture::new();
    // Register an unbound worker (no binding-ref written).
    let unbound = register(fixture.dir.path(), "other", "other-session");
    let payload = json!({
        "session_id": "other-session",
        "cwd": fixture.dir.path(),
        "hook_event_name": "Stop",
    });
    let outcome = feedback::process_hook_event(
        "codex",
        fixture.dir.path(),
        &unbound,
        &serde_json::to_vec(&payload).unwrap(),
        &fixture.trace,
    );
    assert!(outcome.recorded);
    assert!(outcome.forward_error.is_none());
    assert_eq!(fixture.submissions(), 0, "unbound worker must not forward");

    // Bound worker Stop fires transport.
    assert!(fixture.worker_hook("Stop"));
    assert_eq!(fixture.submissions(), 1, "bound worker Stop must forward");
}

#[test]
fn event_filter_stop_and_sessionend_forwarded_others_are_not() {
    let fixture = Fixture::new();
    // SessionStart and UserPromptSubmit are collected but not forwarded.
    for event in ["SessionStart", "UserPromptSubmit"] {
        assert!(fixture.worker_hook(event));
    }
    assert_eq!(
        fixture.submissions(),
        0,
        "SessionStart/UserPromptSubmit must not forward"
    );

    // Stop and SessionEnd are forwarded.
    for event in ["Stop", "SessionEnd"] {
        assert!(fixture.worker_hook(event));
    }
    assert_eq!(fixture.submissions(), 2, "Stop and SessionEnd must forward");
}

#[test]
fn halt_exception_session_rejected_forwarded_as_system_halt_notification() {
    let fixture = Fixture::new();
    // Normal Stop first.
    assert!(fixture.worker_hook("Stop"));
    assert_eq!(fixture.submissions(), 1);

    // Simulate /clear: a new session arrives. Ingest fails; rejection written.
    let rejected = fixture.hook_event("worker", "after-clear", "Stop");
    assert!(
        !rejected.recorded,
        "session mismatch is not a recorded Hook event"
    );
    assert!(rejected.forward_error.is_none());
    assert_eq!(
        fixture.submissions(),
        2,
        "session-rejected must still forward as halt"
    );

    let content = fixture.last_message_content();
    assert_eq!(content["system_halt_notification"], true);
    assert_eq!(content["record"]["source"], feedback::REJECTED);
    assert_eq!(content["record"]["native_session_id"], "after-clear");
}

#[test]
fn forwarding_transport_failure_marks_result_unknown_but_record_exists_and_hook_returns_empty() {
    let fixture = Fixture::new();
    fixture.trace.fail_prompt.set(true);
    let outcome = fixture.hook_event("worker", "worker-session", "Stop");
    assert!(outcome.recorded, "the Stop itself is recorded");
    let error = outcome.forward_error.expect("failed forward is reported");
    assert!(error.contains("unconfirmed"), "{error}");
    assert_eq!(
        fixture.submissions(),
        1,
        "exactly one attempt, never resent"
    );
    let events = fixture.events();
    let [stop, note] = events.as_slice() else {
        panic!("expected the Stop and one unconfirmed note: {events:?}");
    };
    assert_eq!(
        (&stop["source"], &stop["event"]),
        (&json!("native-hook"), &json!("Stop"))
    );
    assert_eq!(note["source"], feedback::UNCONFIRMED);
    assert_eq!(note["event"], "Stop");
    assert_eq!(note["native"]["error"], "submission-unconfirmed");
    assert_eq!(note["native"]["forwarded_received_ms"], stop["received_ms"]);
    assert_eq!(note["native"]["halt"], false);
    // The note is not a session change: a later normal send still passes the sender check.
    fixture.trace.fail_prompt.set(false);
    assert!(
        fixture
            .hook_event("worker", "worker-session", "Stop")
            .forward_error
            .is_none()
    );
    assert_eq!(fixture.submissions(), 2);
}

#[test]
fn dispatcher_events_are_never_forwarded() {
    let fixture = Fixture::new();
    // Send Stop on the dispatcher instance. No binding-ref exists for it.
    let payload = json!({
        "session_id": "dispatcher-session",
        "cwd": fixture.dir.path(),
        "hook_event_name": "Stop",
    });
    let outcome = feedback::process_hook_event(
        "codex",
        fixture.dir.path(),
        &fixture.dispatcher_instance,
        &serde_json::to_vec(&payload).unwrap(),
        &fixture.trace,
    );
    assert!(outcome.recorded);
    assert!(outcome.forward_error.is_none());
    assert_eq!(
        fixture.submissions(),
        0,
        "dispatcher Stop must never be forwarded"
    );
}

#[test]
fn hook_forward_subprocess_returns_empty_json_records_and_forwards() {
    let fixture = Fixture::new();
    let payload = json!({
        "session_id": "worker-session",
        "cwd": fixture.dir.path(),
        "hook_event_name": "Stop",
    });
    let input = serde_json::to_vec(&payload).unwrap();
    let output = hook::run(
        "codex",
        fixture.dir.path(),
        &[(feedback::INSTANCE_ENV, &fixture.worker_instance)],
        Some(&input),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
    let last = fixture.last_event();
    assert_eq!(last["source"], "native-hook");
    assert_eq!(last["event"], "Stop");
    assert_eq!(fixture.last_message_content()["record"]["event"], "Stop");
}

#[test]
fn exhausted_forward_budget_starts_no_native_call_and_keeps_the_record() {
    let fixture = Fixture::new();
    let payload = json!({
        "session_id": "worker-session",
        "cwd": fixture.dir.path(),
        "hook_event_name": "Stop",
    });
    let outcome = feedback::process_hook_event(
        "codex",
        fixture.dir.path(),
        &fixture.worker_instance,
        &serde_json::to_vec(&payload).unwrap(),
        &Bounded::new(Duration::ZERO),
    );
    assert!(outcome.recorded);
    assert!(outcome.forward_error.is_some());
    let socket = &fixture.binding.config.socket;
    assert!(!socket.with_extension("submitted").exists());
    let events = fixture.events();
    assert_eq!(events[0]["event"], "Stop");
    assert_eq!(events[1]["source"], feedback::UNCONFIRMED);
    assert_eq!(events[1]["native"]["error"], "not-submitted");
}

#[test]
fn a_failed_append_under_a_held_lock_never_resends_an_old_rejection() {
    let fixture = Fixture::new();
    assert!(!fixture.hook_event("worker", "after-clear", "Stop").recorded);
    assert_eq!(
        fixture.submissions(),
        1,
        "the native rejection is forwarded once"
    );
    let before = fixture.events().len();
    let lock = fixture.worker_root().join("operation.lock");
    fs::write(&lock, b"").unwrap();
    for session in ["worker-session", "after-clear"] {
        let outcome = fixture.hook_event("worker", session, "Stop");
        assert!(!outcome.recorded, "{session}");
        assert!(outcome.forward_error.is_none(), "{session}");
    }
    fs::remove_file(lock).unwrap();
    assert_eq!(
        fixture.submissions(),
        1,
        "nothing written, nothing forwarded"
    );
    assert_eq!(fixture.events().len(), before);
}

#[test]
fn a_hook_appended_while_the_binding_is_read_does_not_replace_the_forwarded_stop() {
    let fixture = Fixture::new();
    let fifo = fixture.dir.path().join("binding.fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let binding = fs::read(fixture.dir.path().join("test-binding.json")).unwrap();
    fs::write(
        fixture.worker_root().join("binding-ref.json"),
        serde_json::to_vec(&json!({"binding": fifo})).unwrap(),
    )
    .unwrap();
    let (dir, instance) = (
        fixture.dir.path().to_path_buf(),
        fixture.worker_instance.clone(),
    );
    // Opening the FIFO blocks until the callback reads the binding, i.e. after its append.
    let writer = std::thread::spawn(move || {
        let mut pipe = fs::OpenOptions::new().write(true).open(&fifo).unwrap();
        let other = json!({"session_id":"worker-session","cwd":dir,
            "hook_event_name":"UserPromptSubmit"});
        feedback::ingest(
            &dir,
            &instance,
            &AgentKind::Codex,
            &serde_json::to_vec(&other).unwrap(),
        )
        .unwrap();
        std::io::Write::write_all(&mut pipe, &binding).unwrap();
    });
    let outcome = fixture.hook_event("worker", "worker-session", "Stop");
    writer.join().unwrap();
    assert!(outcome.recorded);
    assert!(outcome.forward_error.is_none());
    let events = fixture.events();
    assert_eq!(
        events.last().unwrap()["event"],
        "UserPromptSubmit",
        "the insert happened"
    );
    assert_eq!(fixture.submissions(), 1);
    assert_eq!(fixture.last_message_content()["record"]["event"], "Stop");
    assert_eq!(
        fixture.last_message_content()["record"],
        events[events.len() - 2]
    );
}

#[test]
fn an_agent_report_rejection_is_never_forwarded_as_a_hook_halt() {
    let fixture = Fixture::new();
    let (dir, instance) = (fixture.dir.path(), &fixture.worker_instance);
    assert!(feedback::report(dir, instance, "other-session", "agent", "progress", "x").is_err());
    let rejected = fixture.events();
    assert_eq!(rejected.last().unwrap()["native"]["from"], "agent-report");
    // Same session, wrong cwd: the Hook input is refused before anything is written.
    let wrong = json!({"session_id":"other-session","cwd":"/","hook_event_name":"Stop"});
    let outcome = feedback::process_hook_event(
        "codex",
        dir,
        instance,
        &serde_json::to_vec(&wrong).unwrap(),
        &fixture.trace,
    );
    assert!(!outcome.recorded);
    assert!(outcome.forward_error.is_none());
    assert_eq!(fixture.submissions(), 0);
    assert_eq!(fixture.events().len(), rejected.len());
    // The same session with the right cwd writes its own native rejection and forwards it.
    assert!(
        !fixture
            .hook_event("worker", "other-session", "Stop")
            .recorded
    );
    assert_eq!(fixture.submissions(), 1);
    let content = fixture.last_message_content();
    assert_eq!(content["record"]["native"]["from"], "native-hook");
    assert_eq!(content["system_halt_notification"], true);
}

#[test]
fn a_subprocess_forward_failure_is_noted_once_and_reported_on_stderr() {
    let fixture = Fixture::new();
    fixture.set_state("prompt_failure", json!(true));
    let payload = json!({"session_id":"worker-session","cwd":fixture.dir.path(),
        "hook_event_name":"Stop"});
    let output = hook::run(
        "codex",
        fixture.dir.path(),
        &[(feedback::INSTANCE_ENV, &fixture.worker_instance)],
        Some(&serde_json::to_vec(&payload).unwrap()),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("forward unconfirmed; not resent"),
        "{stderr}"
    );
    let prompts = fs::read_to_string(fixture.binding.config.socket.with_extension("prompts"));
    assert_eq!(prompts.unwrap().lines().count(), 1, "one attempt only");
    let events = fixture.events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["source"], feedback::UNCONFIRMED);
    assert_eq!(events[1]["native"]["error"], "submission-unconfirmed");
}

#[test]
fn halt_send_keeps_the_fixed_worker_to_dispatcher_direction() {
    let fixture = Fixture::new();
    let halt = |from: &str, to: &str| crewmoor::message::Message {
        id: "halt-direction-1".into(),
        from: from.into(),
        to: to.into(),
        reply_to: None,
        content: "halt".into(),
    };
    for (from, to) in [
        ("outsider", "dispatcher"),
        ("dispatcher", "dispatcher"),
        ("dispatcher", "worker"),
        ("worker", "worker"),
    ] {
        let message = halt(from, to);
        let sent =
            crewmoor::message::send_halt_to_dispatcher(&fixture.binding, &message, &fixture.trace);
        assert!(sent.is_err(), "{from} -> {to} must be refused");
    }
    assert_eq!(fixture.submissions(), 0);
    let message = halt("worker", "dispatcher");
    assert!(
        crewmoor::message::send_halt_to_dispatcher(&fixture.binding, &message, &fixture.trace)
            .is_ok()
    );
    assert_eq!(fixture.submissions(), 1);
}
