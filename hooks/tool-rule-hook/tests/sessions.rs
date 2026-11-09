//! Rust controller/checker subprocess tests. Native session metadata is simulated.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
fn spawn(mode: &str, root: &Path, input: &Value, scope: Option<&str>) -> std::process::Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"));
    command.arg(mode).arg(root);
    if let Some(scope) = scope {
        command.arg(scope);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    child
}
fn value(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn call(mode: &str, root: &Path, input: &Value) -> Output {
    spawn(mode, root, input, None).wait_with_output().unwrap()
}
fn rules(base: &Path, name: &str, entry: &str) -> std::path::PathBuf {
    let file = base.join(format!("{name}.json"));
    fs::write(&file,json!({"base_dir":base,"entries":[{"argv":[entry],"label":name}],"feedback":format!("Policy {name}")}).to_string()).unwrap();
    file
}
fn selection(base: &Path, session: &str, file: &Path) -> Value {
    json!({"scope":"server-location-A","session":session,"directory":base,"rules":file})
}
fn bridge(root: &Path, base: &Path, session: &str, scope: &str) -> Value {
    let event = json!({"tool":"shell","sessionID":session,"directory":base,"input":{"command":"restricted"}});
    value(
        spawn("bridge-session", root, &event, Some(scope))
            .wait_with_output()
            .unwrap(),
    )
}
#[test]
fn same_store_cwd_and_server_keep_roles_children_and_unknown_sessions_separate() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("owned-state");
    let blocked = rules(base.path(), "blocked", "restricted");
    let clear = rules(base.path(), "clear", "unrelated");
    value(call(
        "session-bind",
        &root,
        &selection(base.path(), "native-a", &blocked),
    ));
    value(call(
        "session-bind",
        &root,
        &selection(base.path(), "native-b", &clear),
    ));
    assert_eq!(
        bridge(&root, base.path(), "native-a", "server-location-A")["decision"],
        "deny"
    );
    assert_eq!(
        bridge(&root, base.path(), "native-b", "server-location-A")["decision"],
        "no_match"
    );
    for (session, scope) in [
        ("child-of-native-a", "server-location-A"),
        ("native-a", "other-server"),
        ("unknown", "server-location-A"),
    ] {
        let decision = bridge(&root, base.path(), session, scope);
        assert_eq!(decision["decision"], "deny");
        assert!(decision["reason"].as_str().unwrap().contains("unbound"));
    }
    let wrong = base.path().join("different");
    assert_eq!(
        bridge(&root, &wrong, "native-a", "server-location-A")["decision"],
        "deny"
    );
    assert!(
        !call(
            "session-bind",
            &root,
            &selection(base.path(), "native-a", &clear)
        )
        .status
        .success()
    );
    assert_eq!(
        bridge(&root, base.path(), "native-a", "server-location-A")["decision"],
        "deny"
    );
}
#[test]
fn persistent_revision_updates_require_explicit_control_and_end_is_not_resurrected() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("owned-state");
    let file = rules(base.path(), "blocked", "restricted");
    let request = selection(base.path(), "native-a", &file);
    assert_eq!(value(call("session-bind", &root, &request))["revision"], 1);
    // Editing the source alone never silently changes a session snapshot.
    rules(base.path(), "blocked", "unrelated");
    assert_eq!(
        bridge(&root, base.path(), "native-a", "server-location-A")["decision"],
        "deny"
    );
    let mut update = request.clone();
    update["expected_revision"] = json!(1);
    assert_eq!(value(call("session-update", &root, &update))["revision"], 2);
    assert!(!call("session-update", &root, &update).status.success());
    // Each check is a new process: reload/restart/resume retain the exact binding.
    assert_eq!(
        bridge(&root, base.path(), "native-a", "server-location-A")["decision"],
        "no_match"
    );
    let identity = json!({"scope":"server-location-A","session":"native-a","expected_revision":2});
    assert_eq!(value(call("session-show", &root, &identity))["revision"], 2);
    assert_eq!(value(call("session-end", &root, &identity))["revision"], 3);
    assert_eq!(
        bridge(&root, base.path(), "native-a", "server-location-A")["decision"],
        "deny"
    );
    assert!(!call("session-bind", &root, &request).status.success());
    update["expected_revision"] = json!(3);
    assert!(!call("session-update", &root, &update).status.success());
}
#[test]
fn simultaneous_binding_and_compare_revision_updates_have_one_winner() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("owned-state");
    let file = rules(base.path(), "blocked", "restricted");
    let mut request = selection(base.path(), "native-a", &file);
    let first = spawn("session-bind", &root, &request, None);
    let second = spawn("session-bind", &root, &request, None);
    let outcomes = [
        first.wait_with_output().unwrap().status.success(),
        second.wait_with_output().unwrap().status.success(),
    ];
    assert_eq!(outcomes.into_iter().filter(|success| *success).count(), 1);
    request["expected_revision"] = json!(1);
    let first = spawn("session-update", &root, &request, None);
    let second = spawn("session-update", &root, &request, None);
    let outcomes = [
        first.wait_with_output().unwrap().status.success(),
        second.wait_with_output().unwrap().status.success(),
    ];
    assert_eq!(outcomes.into_iter().filter(|success| *success).count(), 1);
    let identity = json!({"scope":"server-location-A","session":"native-a"});
    assert_eq!(value(call("session-show", &root, &identity))["revision"], 2);
    assert_eq!(
        bridge(&root, base.path(), "native-a", "server-location-A")["decision"],
        "deny"
    );
}

#[test]
fn explicit_unrestricted_child_binding_still_checks_location_and_corrupt_state_denies() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("owned-state");
    let request =
        json!({"scope":"server-location-A","session":"child","directory":base.path(),"rules":null});
    let mut missing = request.clone();
    missing.as_object_mut().unwrap().remove("rules");
    assert!(!call("session-bind", &root, &missing).status.success());
    value(call("session-bind", &root, &request));
    assert_eq!(
        bridge(&root, base.path(), "child", "server-location-A")["decision"],
        "no_match"
    );
    let event = json!({"tool":"shell","sessionID":"child","directory":base.path(),"workspace":"different-workspace","parentID":"unrelated-parent","input":{"command":"restricted"}});
    assert_eq!(
        value(
            spawn("bridge-session", &root, &event, Some("server-location-A"))
                .wait_with_output()
                .unwrap()
        )["decision"],
        "deny"
    );
    fs::write(root.join("bindings.json"), "invalid JSON").unwrap();
    assert_eq!(
        bridge(&root, base.path(), "child", "server-location-A")["decision"],
        "deny"
    );
}
