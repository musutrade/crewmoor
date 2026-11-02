#[path = "support/agents.rs"]
mod agents;
#[path = "support/hook.rs"]
mod hook;
use crewmoor::{
    config::{AgentKind, Runtime},
    feedback::{self, Evidence, Registration},
    issue::{self, Assignment, Gh},
    message::{Binding, Bounded, Config, Native, Peer, Transport},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    os::unix::net::UnixListener,
    path::Path,
    process::Command,
    time::Duration,
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
    gh: FakeGh,
}

/// `gh` stand-in: answers `issue view` and `issue list` from settable state, logs calls.
#[derive(Default)]
struct FakeGh {
    view: RefCell<Option<Value>>,
    list: RefCell<Option<Value>>,
    calls: RefCell<Vec<String>>,
}
impl FakeGh {
    fn answer(&self, state: Option<&str>, list: Option<Value>) {
        *self.view.borrow_mut() = state.map(|state| json!({"state": state}));
        *self.list.borrow_mut() = list;
    }
    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}
impl Gh for FakeGh {
    fn call(&self, args: &[String]) -> crewmoor::Result<Value> {
        let sub = args[1].clone();
        self.calls.borrow_mut().push(sub.clone());
        let answer = match sub.as_str() {
            "view" => self.view.borrow().clone(),
            _ => self.list.borrow().clone(),
        };
        answer.ok_or_else(|| "synthetic gh failure".into())
    }
}

#[derive(Default)]
struct Trace {
    submissions: Cell<usize>,
    fail_prompt: Cell<bool>,
    /// The shared deadline runs out after the route checks, right at the prompt call.
    exhaust_at_prompt: Cell<bool>,
}

impl Transport for Trace {
    fn call(&self, config: &Config, args: &[String]) -> crewmoor::Result<Value> {
        let prompt = args.get(1).map(String::as_str) == Some("prompt");
        if prompt && self.exhaust_at_prompt.get() {
            return Bounded::new(Duration::ZERO).call(config, args);
        }
        if prompt {
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
            gh: FakeGh::default(),
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
            &self.gh,
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
        &fixture.gh,
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
        &fixture.gh,
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
        &fixture.gh,
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
        &fixture.gh,
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

// ---------------------------------------------------------------------------
// Issue #8 step 2: next issue on the dispatcher's own Stop
// ---------------------------------------------------------------------------

const REPO: &str = "musutrade/crewmoor";

fn listing(number: u64) -> Value {
    json!([{"number": number, "title": format!("item {number}"), "body": "long body",
        "url": format!("https://github.com/{REPO}/issues/{number}")}])
}

impl Fixture {
    /// What `team bind` writes on the dispatcher side.
    fn bind_dispatcher(&self) {
        let path = self.dir.path().join("test-binding.json");
        feedback::write_dispatcher_binding_ref(self.dir.path(), &self.dispatcher_instance, &path)
            .unwrap();
    }
    fn assign(&self, number: u64) {
        let assignment = Assignment {
            repo: REPO.into(),
            number,
        };
        issue::assign(self.dir.path(), &self.dispatcher_instance, &assignment).unwrap();
    }
    fn dispatcher_stop(&self) -> feedback::HookOutcome {
        self.hook_event("dispatcher", "dispatcher-session", "Stop")
    }
    fn dispatcher_events(&self) -> Vec<Value> {
        feedback::show(self.dir.path(), &self.dispatcher_instance).unwrap()["events"]
            .as_array()
            .unwrap()
            .clone()
    }
    fn delivered(&self) -> Option<Value> {
        let path = self
            .dir
            .path()
            .join("instances")
            .join(&self.dispatcher_instance)
            .join(feedback::DELIVERED_FILE);
        fs::read(path)
            .ok()
            .map(|bytes| serde_json::from_slice(&bytes).unwrap())
    }
    fn last_message(&self) -> Value {
        let submitted: Value = serde_json::from_slice(
            &fs::read(self.binding.config.socket.with_extension("submitted")).unwrap(),
        )
        .unwrap();
        serde_json::from_str(submitted["args"][4].as_str().unwrap()).unwrap()
    }
}

#[test]
fn a_closed_assignment_delivers_the_next_issue_once_to_the_dispatcher() {
    let fixture = Fixture::new();
    fixture.bind_dispatcher();
    fixture.assign(8);
    fixture.gh.answer(Some("CLOSED"), Some(listing(21)));
    let outcome = fixture.dispatcher_stop();
    assert!(outcome.recorded);
    assert!(outcome.forward_error.is_none());
    assert!(
        outcome.delivery_error.is_none(),
        "{:?}",
        outcome.delivery_error
    );
    assert_eq!(fixture.gh.calls(), ["view", "list"]);
    assert_eq!(fixture.submissions(), 1);
    let envelope = fixture.last_message();
    assert_eq!(envelope["message"]["from"], "dispatcher");
    assert_eq!(envelope["message"]["to"], "dispatcher");
    let submitted: Value = serde_json::from_slice(
        &fs::read(fixture.binding.config.socket.with_extension("submitted")).unwrap(),
    )
    .unwrap();
    assert_eq!(submitted["args"][3], "w1:p1", "sent to the dispatcher pane");
    let content = fixture.last_message_content();
    assert_eq!(
        content,
        json!({"system_issue_delivery": true,
            "feedback_instance": fixture.dispatcher_instance,
            "completed": {"repo": REPO, "number": 8},
            "next": {"repo": REPO, "number": 21, "title": "item 21",
                "url": format!("https://github.com/{REPO}/issues/21")}})
    );
    assert_eq!(
        fixture.delivered(),
        Some(json!({"repo": REPO, "closed_number": 8}))
    );
    // A later Stop before a new assignment neither queries GitHub nor resends.
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert_eq!(fixture.gh.calls().len(), 2);
    assert_eq!(fixture.submissions(), 1);
    // Assigning the delivered item and closing it delivers again.
    fixture.assign(21);
    fixture.gh.answer(Some("CLOSED"), Some(listing(22)));
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert_eq!(fixture.submissions(), 2);
    assert_eq!(fixture.last_message_content()["next"]["number"], 22);
    let events = fixture.dispatcher_events();
    assert!(
        events.iter().all(|e| e["source"] == "native-hook"),
        "no unconfirmed notes: {events:?}"
    );
}

#[test]
fn open_unassigned_empty_or_unbound_dispatcher_stops_are_skipped_silently() {
    let fixture = Fixture::new();
    // Not bound on the dispatcher side: nothing is read from GitHub.
    fixture.assign(8);
    fixture.gh.answer(Some("CLOSED"), Some(listing(21)));
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert!(fixture.gh.calls().is_empty());
    fixture.bind_dispatcher();
    // Still open: one state query, no listing, no send.
    fixture.gh.answer(Some("OPEN"), Some(listing(21)));
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert_eq!(fixture.gh.calls(), ["view"]);
    // Closed but nothing labelled ready.
    fixture.gh.answer(Some("CLOSED"), Some(json!([])));
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert_eq!(fixture.gh.calls(), ["view", "view", "list"]);
    assert_eq!(fixture.submissions(), 0);
    assert!(fixture.delivered().is_none());
    assert!(
        fixture
            .dispatcher_events()
            .iter()
            .all(|e| e["source"] == "native-hook")
    );
}

#[test]
fn no_assignment_skips_and_only_the_dispatcher_stop_triggers_delivery() {
    let fixture = Fixture::new();
    fixture.bind_dispatcher();
    fixture.gh.answer(Some("CLOSED"), Some(listing(21)));
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert!(fixture.gh.calls().is_empty(), "no assignment, no query");
    fixture.assign(8);
    for event in ["SessionStart", "UserPromptSubmit", "SessionEnd"] {
        let outcome = fixture.hook_event("dispatcher", "dispatcher-session", event);
        assert!(outcome.recorded, "{event}");
        assert!(outcome.delivery_error.is_none(), "{event}");
    }
    // A worker Stop is forwarded but never delivers on the worker's behalf.
    assert!(fixture.worker_hook("Stop"));
    assert_eq!(fixture.submissions(), 1);
    assert_eq!(fixture.last_message_content()["record"]["event"], "Stop");
    // A rejected dispatcher session is not a recorded Stop.
    let rejected = fixture.hook_event("dispatcher", "after-clear", "Stop");
    assert!(!rejected.recorded);
    assert!(rejected.delivery_error.is_none());
    assert!(fixture.gh.calls().is_empty());
    assert_eq!(fixture.submissions(), 1);
}

#[test]
fn a_failed_github_query_is_noted_with_its_stage_and_retried_on_the_next_stop() {
    let fixture = Fixture::new();
    fixture.bind_dispatcher();
    fixture.assign(8);
    fixture.gh.answer(None, None);
    let error = fixture.dispatcher_stop().delivery_error.expect("noted");
    assert!(error.contains("synthetic gh failure"), "{error}");
    fixture.gh.answer(Some("CLOSED"), None);
    assert!(fixture.dispatcher_stop().delivery_error.is_some());
    let events = fixture.dispatcher_events();
    let notes: Vec<_> = events
        .iter()
        .filter(|e| e["source"] == feedback::DELIVERY_UNCONFIRMED)
        .collect();
    let [done, next] = notes.as_slice() else {
        panic!("two notes expected: {events:?}");
    };
    assert_eq!(done["event"], "Stop");
    assert_eq!(done["native_session_id"], "dispatcher-session");
    assert_eq!(
        done["native"],
        json!({"forwarded_received_ms": events[0]["received_ms"], "repo": REPO,
            "closed_number": 8, "next_number": null, "stage": "done", "error": "not-submitted"})
    );
    assert_eq!(next["native"]["stage"], "next");
    assert!(
        !done.to_string().contains("synthetic"),
        "no free text on disk"
    );
    assert_eq!(fixture.submissions(), 0);
    assert!(fixture.delivered().is_none());
    // Nothing was submitted, so the next Stop tries again and delivers.
    fixture.gh.answer(Some("CLOSED"), Some(listing(21)));
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert_eq!(fixture.submissions(), 1);
}

#[test]
fn an_unconfirmed_delivery_is_noted_marked_and_never_resent() {
    let fixture = Fixture::new();
    fixture.bind_dispatcher();
    fixture.assign(8);
    fixture.gh.answer(Some("CLOSED"), Some(listing(21)));
    fixture.trace.fail_prompt.set(true);
    let error = fixture.dispatcher_stop().delivery_error.expect("noted");
    assert!(error.contains("unconfirmed"), "{error}");
    assert_eq!(fixture.submissions(), 1);
    let note = fixture.dispatcher_events().last().cloned().unwrap();
    assert_eq!(note["source"], feedback::DELIVERY_UNCONFIRMED);
    assert_eq!(note["native"]["stage"], "send");
    assert_eq!(note["native"]["next_number"], 21);
    assert_eq!(note["native"]["error"], "submission-unconfirmed");
    assert!(fixture.delivered().is_some(), "possibly submitted: marked");
    fixture.trace.fail_prompt.set(false);
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert_eq!(fixture.submissions(), 1, "never resent");
}

#[test]
fn a_delivery_refused_before_the_prompt_is_not_marked_and_is_tried_again() {
    let fixture = Fixture::new();
    fixture.bind_dispatcher();
    fixture.assign(8);
    fixture.gh.answer(Some("CLOSED"), Some(listing(21)));
    // The dispatcher pane moved: the route check refuses before any prompt call.
    fixture.set_state("w1:p1", agent("w1:p9", "one", "dispatcher-session"));
    assert!(fixture.dispatcher_stop().delivery_error.is_some());
    assert_eq!(fixture.submissions(), 0);
    let note = fixture.dispatcher_events().last().cloned().unwrap();
    assert_eq!(
        (&note["native"]["stage"], &note["native"]["error"]),
        (&json!("send"), &json!("not-submitted"))
    );
    assert!(fixture.delivered().is_none());
    fixture.set_state("w1:p1", agent("w1:p1", "one", "dispatcher-session"));
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert_eq!(fixture.submissions(), 1);
}

#[test]
fn an_exhausted_budget_starts_no_gh_or_native_call() {
    let fixture = Fixture::new();
    fixture.bind_dispatcher();
    fixture.assign(8);
    let payload = json!({"session_id": "dispatcher-session", "cwd": fixture.dir.path(),
        "hook_event_name": "Stop"});
    let gh = issue::gh::Bounded {
        program: "/nonexistent/gh".into(),
        deadline: std::time::Instant::now(),
    };
    let outcome = feedback::process_hook_event(
        "codex",
        fixture.dir.path(),
        &fixture.dispatcher_instance,
        &serde_json::to_vec(&payload).unwrap(),
        &Bounded::new(Duration::ZERO),
        &gh,
    );
    assert!(outcome.recorded);
    let error = outcome.delivery_error.expect("noted");
    assert!(error.contains("budget exhausted"), "{error}");
    assert_eq!(
        fixture.dispatcher_events().last().unwrap()["native"]["stage"],
        "done"
    );
    assert!(
        !fixture
            .binding
            .config
            .socket
            .with_extension("submitted")
            .exists()
    );
}

#[test]
fn a_dispatcher_reference_naming_another_dispatcher_is_noted_not_used() {
    let fixture = Fixture::new();
    let path = fixture.dir.path().join("test-binding.json");
    // Written on the worker side: the binding's dispatcher is someone else.
    feedback::write_dispatcher_binding_ref(fixture.dir.path(), &fixture.worker_instance, &path)
        .unwrap();
    // First bind wins: a second reference is kept as it was.
    feedback::write_dispatcher_binding_ref(
        fixture.dir.path(),
        &fixture.worker_instance,
        Path::new("/x"),
    )
    .unwrap();
    let kept = feedback::load_dispatcher_binding_ref(fixture.dir.path(), &fixture.worker_instance);
    assert_eq!(kept.unwrap().unwrap().binding, path);
    let outcome = fixture.hook_event("worker", "worker-session", "Stop");
    assert!(outcome.forward_error.is_none());
    let error = outcome.delivery_error.expect("noted");
    assert!(error.contains("another dispatcher"), "{error}");
    let note = fixture.events().last().cloned().unwrap();
    assert_eq!(note["native"]["stage"], "local");
    assert!(fixture.gh.calls().is_empty());
}

#[test]
fn the_hook_subprocess_delivers_through_the_installed_gh_within_its_budget() {
    let fixture = Fixture::new();
    fixture.bind_dispatcher();
    fixture.assign(8);
    let bin = fixture.dir.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_gh-fixture"), bin.join("gh")).unwrap();
    fs::write(
        fixture.dir.path().join("answer-view"),
        r#"{"state":"CLOSED"}"#,
    )
    .unwrap();
    fs::write(
        fixture.dir.path().join("answer-list"),
        listing(21).to_string(),
    )
    .unwrap();
    fs::write(fixture.dir.path().join("status"), "0").unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let fixture_dir = fixture.dir.path().to_str().unwrap();
    let payload = json!({"session_id": "dispatcher-session", "cwd": fixture.dir.path(),
        "hook_event_name": "Stop"});
    let output = hook::run(
        "codex",
        fixture.dir.path(),
        &[
            (feedback::INSTANCE_ENV, &fixture.dispatcher_instance),
            ("PATH", &path),
            ("GH_FIXTURE_DIR", fixture_dir),
        ],
        Some(&serde_json::to_vec(&payload).unwrap()),
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unconfirmed"), "{stderr}");
    assert_eq!(fixture.last_message_content()["next"]["number"], 21);
    assert!(fixture.delivered().is_some());
    // A failing gh is reported on stderr and still answers {}.
    fixture.assign(21);
    fs::write(fixture.dir.path().join("status"), "1").unwrap();
    let output = hook::run(
        "codex",
        fixture.dir.path(),
        &[
            (feedback::INSTANCE_ENV, &fixture.dispatcher_instance),
            ("PATH", &path),
            ("GH_FIXTURE_DIR", fixture_dir),
        ],
        Some(&serde_json::to_vec(&payload).unwrap()),
    );
    assert_eq!(output.stdout, b"{}\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("issue delivery unconfirmed; not resent"),
        "{stderr}"
    );
}

#[test]
fn the_dispatcher_notice_route_refuses_any_other_sender_or_recipient() {
    let fixture = Fixture::new();
    for (from, to) in [("worker", "dispatcher"), ("dispatcher", "worker")] {
        let message = crewmoor::message::Message {
            id: "notice-1".into(),
            from: from.into(),
            to: to.into(),
            reply_to: None,
            content: "x".into(),
        };
        let error =
            crewmoor::message::send_to_dispatcher(&fixture.binding, &message, &fixture.trace)
                .unwrap_err()
                .to_string();
        assert!(
            error.contains("from and to the bound dispatcher"),
            "{error}"
        );
    }
    assert_eq!(fixture.submissions(), 0);
}

#[test]
fn a_budget_exhausted_at_the_prompt_is_not_submitted_unmarked_and_retried() {
    let fixture = Fixture::new();
    fixture.bind_dispatcher();
    fixture.assign(8);
    fixture.gh.answer(Some("CLOSED"), Some(listing(21)));
    fixture.trace.exhaust_at_prompt.set(true);
    let error = fixture.dispatcher_stop().delivery_error.expect("noted");
    assert!(error.contains("call not started"), "{error}");
    assert!(!error.contains("submission unconfirmed"), "{error}");
    let socket = &fixture.binding.config.socket;
    assert!(
        !socket.with_extension("prompts").exists(),
        "no prompt CLI ran"
    );
    let note = fixture.dispatcher_events().last().cloned().unwrap();
    assert_eq!(
        (&note["native"]["stage"], &note["native"]["error"]),
        (&json!("send"), &json!("not-submitted"))
    );
    assert!(fixture.delivered().is_none(), "never started: not marked");
    // The next normal Stop queries again and delivers the item that was never sent.
    fixture.trace.exhaust_at_prompt.set(false);
    assert!(fixture.dispatcher_stop().delivery_error.is_none());
    assert_eq!(fixture.gh.calls(), ["view", "list", "view", "list"]);
    assert_eq!(fixture.submissions(), 1);
    assert_eq!(fixture.last_message_content()["next"]["number"], 21);
    assert!(fixture.delivered().is_some());
}

#[test]
fn a_worker_forward_exhausted_at_the_prompt_is_noted_not_submitted() {
    let fixture = Fixture::new();
    fixture.trace.exhaust_at_prompt.set(true);
    let error = fixture
        .hook_event("worker", "worker-session", "Stop")
        .forward_error
        .expect("noted");
    assert!(error.contains("call not started"), "{error}");
    assert!(
        !fixture
            .binding
            .config
            .socket
            .with_extension("prompts")
            .exists()
    );
    let note = fixture.events().last().cloned().unwrap();
    assert_eq!(note["source"], feedback::UNCONFIRMED);
    assert_eq!(note["native"]["error"], "not-submitted");
}
