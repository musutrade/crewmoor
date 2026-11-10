//! Synthetic native Codex input and real independent checker processes.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Seek, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
struct Fixture {
    root: tempfile::TempDir,
    data: PathBuf,
    store: PathBuf,
    rules: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("native-plugin-data");
        let store = data.join("tool-rule-hook");
        let rules = root.path().join("rules.json");
        fs::write(&rules,json!({"base_dir":root.path(),"entries":[{"argv":["restricted"],"label":"root-policy"}],"feedback":"Use the approved focused task."}).to_string()).unwrap();
        Self {
            root,
            data,
            store,
            rules,
        }
    }
    fn command(&self, mode: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"));
        command.arg(mode).env("PLUGIN_DATA", &self.data);
        for name in [
            "TOOL_RULE_HOOK_RULES",
            "TOOL_RULE_HOOK_CONTEXT",
            "TOOL_RULE_HOOK_VERIFY",
        ] {
            command.env_remove(name);
        }
        command
    }
    fn selection(&self, tree: &str, caller: &str, rules: Option<&Path>) -> Value {
        json!({"scope":tree,"session":caller,"directory":self.root.path(),"rules":rules})
    }
    fn control(&self, mode: &str, input: &Value) -> Output {
        capture(
            self.command(mode).arg(&self.store),
            input.to_string().as_bytes(),
        )
    }
    fn bind(&self, tree: &str, caller: &str, rules: Option<&Path>) {
        assert_eq!(
            decode(self.control("codex-session-bind", &self.selection(tree, caller, rules)))["revision"],
            1
        );
    }
    fn event(&self, tree: &str, agent: Option<&str>, command: &str) -> Value {
        json!({"hook_event_name":"PreToolUse","tool_name":"Bash","session_id":tree,
            "agent_id":agent,"cwd":self.root.path(),"tool_input":{"command":command},
            "role":"not-authority","rules":null})
    }
    fn call(&self, event: &Value) -> Value {
        decode(capture(
            &mut self.command("codex-hook"),
            event.to_string().as_bytes(),
        ))
    }
}
fn capture(command: &mut Command, input: &[u8]) -> Output {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(input).unwrap();
    file.rewind().unwrap();
    command.stdin(Stdio::from(file)).output().unwrap()
}
fn decode(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn denied(value: &Value) -> &str {
    assert_eq!(value["hookSpecificOutput"]["permissionDecision"], "deny");
    value["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap()
}
#[test]
fn native_store_selects_same_cwd_roots_without_any_client_selection() {
    let fixture = Fixture::new();
    fixture.bind("root-a", "root-a", Some(&fixture.rules));
    fixture.bind("root-b", "root-b", None);
    let blocked = fixture.call(&fixture.event("root-a", None, "restricted"));
    let reason = denied(&blocked);
    assert!(reason.contains("calling_session=root-a"));
    assert!(reason.contains("scope=root-a"));
    assert!(reason.contains("revision=1"));
    assert!(reason.contains(fixture.rules.to_str().unwrap()));
    assert_eq!(
        fixture.call(&fixture.event("root-b", None, "restricted")),
        json!({})
    );
    assert_eq!(
        fixture.call(&fixture.event("unselected-root", None, "restricted")),
        json!({})
    );
    // Runtime process selection may be another shared-host client's stale value.
    let mut stale = fixture.command("codex-hook");
    stale
        .env("TOOL_RULE_HOOK_RULES", "/missing-other-client.json")
        .env("TOOL_RULE_HOOK_CONTEXT", "")
        .env("TOOL_RULE_HOOK_VERIFY", "invalid");
    assert_eq!(
        decode(capture(
            &mut stale,
            fixture
                .event("root-b", None, "restricted")
                .to_string()
                .as_bytes()
        )),
        json!({})
    );
    let other = fixture.root.path().join("other-plugin-home");
    let mut isolated = fixture.command("codex-hook");
    isolated.env("PLUGIN_DATA", other);
    assert_eq!(
        decode(capture(
            &mut isolated,
            fixture
                .event("root-a", None, "restricted")
                .to_string()
                .as_bytes()
        )),
        json!({})
    );
}
#[test]
fn child_and_sibling_callers_need_their_own_binding() {
    let fixture = Fixture::new();
    fixture.bind("tree", "tree", Some(&fixture.rules));
    assert!(
        denied(&fixture.call(&fixture.event("tree", Some("child"), "restricted")))
            .contains("unbound")
    );
    fixture.bind("tree", "child", None);
    assert_eq!(
        fixture.call(&fixture.event("tree", Some("child"), "restricted")),
        json!({})
    );
    denied(&fixture.call(&fixture.event("tree", None, "restricted")));
    assert!(
        denied(&fixture.call(&fixture.event("tree", Some("sibling"), "ordinary")))
            .contains("unbound")
    );
    // Caller ID is not borrowed across another native tree.
    assert_eq!(
        fixture.call(&fixture.event("different-tree", Some("child"), "restricted")),
        json!({})
    );
}
#[test]
fn snapshots_updates_restart_and_end_require_explicit_control() {
    let fixture = Fixture::new();
    fixture.bind("root", "root", Some(&fixture.rules));
    fs::write(&fixture.rules,json!({"base_dir":fixture.root.path(),"entries":[{"argv":["other"],"label":"new"}],"feedback":"new policy"}).to_string()).unwrap();
    denied(&fixture.call(&fixture.event("root", None, "restricted")));
    assert!(
        !fixture
            .control(
                "codex-session-bind",
                &fixture.selection("root", "root", None)
            )
            .status
            .success()
    );
    let mut update = fixture.selection("root", "root", Some(&fixture.rules));
    update["expected_revision"] = json!(1);
    assert_eq!(
        decode(fixture.control("codex-session-update", &update))["revision"],
        2
    );
    assert!(
        !fixture
            .control("codex-session-update", &update)
            .status
            .success()
    );
    assert_eq!(
        fixture.call(&fixture.event("root", None, "restricted")),
        json!({})
    );
    let reason = fixture.call(&fixture.event("root", None, "other"));
    assert!(denied(&reason).contains("revision=2"));
    let identity = json!({"scope":"root","session":"root","expected_revision":2});
    assert_eq!(
        decode(fixture.control("codex-session-show", &identity))["rules"],
        json!(fixture.rules)
    );
    assert_eq!(
        decode(fixture.control("codex-session-end", &identity))["ended"],
        true
    );
    assert!(denied(&fixture.call(&fixture.event("root", None, "ordinary"))).contains("ended"));
    update["expected_revision"] = json!(3);
    assert!(
        !fixture
            .control("codex-session-update", &update)
            .status
            .success()
    );
    assert!(
        !fixture
            .control(
                "codex-session-bind",
                &fixture.selection("root", "root", None)
            )
            .status
            .success()
    );
    assert_eq!(
        fixture.call(&fixture.event("new-root", None, "restricted")),
        json!({})
    );
}
#[test]
fn concurrent_codex_updates_check_expected_revision_under_the_writer_lock() {
    let fixture = Fixture::new();
    fixture.bind("root", "root", Some(&fixture.rules));
    let mut blocked = fixture.selection("root", "root", Some(&fixture.rules));
    blocked["expected_revision"] = json!(1);
    let mut clear = fixture.selection("root", "root", None);
    clear["expected_revision"] = json!(1);
    let start = |request: Value| {
        let mut input = tempfile::tempfile().unwrap();
        input.write_all(request.to_string().as_bytes()).unwrap();
        input.rewind().unwrap();
        fixture
            .command("codex-session-update")
            .arg(&fixture.store)
            .stdin(Stdio::from(input))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = start(blocked);
    let second = start(clear);
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    assert_ne!(first.status.success(), second.status.success());
    let winning_block = first.status.success();
    let show = decode(fixture.control(
        "codex-session-show",
        &json!({"scope":"root","session":"root"}),
    ));
    assert_eq!(show["revision"], 2);
    let result = fixture.call(&fixture.event("root", None, "restricted"));
    if winning_block {
        denied(&result);
    } else {
        assert_eq!(result, json!({}));
    }
}
#[test]
fn only_native_codex_metadata_selects_rules_and_damaged_bindings_deny() {
    let fixture = Fixture::new();
    fixture.bind("root", "root", Some(&fixture.rules));
    for field in ["session_id", "agent_id", "hook_event_name"] {
        let mut event = fixture.event("root", None, "restricted");
        event[field] = json!(17);
        denied(&fixture.call(&event));
    }
    let mut workspace = fixture.selection("root", "other", None);
    workspace["workspace"] = json!("self-claimed");
    assert!(
        !fixture
            .control("codex-session-bind", &workspace)
            .status
            .success()
    );
    let path = fixture.store.join("bindings.json");
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for (field, replacement) in [
        ("scope", json!("wrong-tree")),
        ("session", json!("wrong-caller")),
        ("workspace", json!("wrong-workspace")),
        ("directory", json!("relative")),
        ("policy", Value::Null),
    ] {
        let mut damaged = original.clone();
        let binding = damaged["bindings"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        binding[field] = replacement;
        fs::write(&path, damaged.to_string()).unwrap();
        denied(&fixture.call(&fixture.event("root", None, "restricted")));
    }
    let only_binding = original["bindings"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    for invalid_key in [
        "invalid-key",
        "[\"codex\", \"root\", \"root\"]",
        "[\"unknown\",\"root\",\"root\"]",
    ] {
        let mut invalid = serde_json::Map::new();
        invalid.insert(invalid_key.into(), only_binding.clone());
        fs::write(&path, json!({"bindings":invalid}).to_string()).unwrap();
        denied(&fixture.call(&fixture.event("root", None, "restricted")));
    }
    fs::write(&path, "invalid state JSON").unwrap();
    denied(&fixture.call(&fixture.event("unselected", None, "ordinary")));
}
#[test]
fn unselected_store_is_neutral_and_invalid_native_data_never_guesses_a_store() {
    let fixture = Fixture::new();
    for input in [b"".as_slice(), b"{invalid JSON", b"\xff"] {
        assert_eq!(
            decode(capture(&mut fixture.command("codex-hook"), input)),
            json!({})
        );
    }
    for root in [None, Some(""), Some("relative")] {
        let mut command = fixture.command("codex-hook");
        command.env_remove("PLUGIN_DATA");
        if let Some(root) = root {
            command.env("PLUGIN_DATA", root);
        }
        denied(&decode(capture(&mut command, b"{}")));
    }
    // OpenCode's existing JSON key format and contracts remain a separate domain.
    assert_eq!(
        decode(fixture.control(
            "session-bind",
            &fixture.selection("root", "root", Some(&fixture.rules))
        ))["revision"],
        1
    );
    assert_eq!(
        fixture.call(&fixture.event("root", None, "restricted")),
        json!({})
    );
    let stored: Value =
        serde_json::from_slice(&fs::read(fixture.store.join("bindings.json")).unwrap()).unwrap();
    assert!(
        stored["bindings"]
            .as_object()
            .unwrap()
            .keys()
            .all(|key| key.starts_with("[\"opencode\","))
    );
}
#[test]
fn actual_tool_cwd_resolves_python_instead_of_bound_startup_directory() {
    let fixture = Fixture::new();
    let sub = fixture.root.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(fixture.root.path().join("check.py"), "print('ordinary')").unwrap();
    fs::write(
        sub.join("check.py"),
        "import subprocess\nsubprocess.run(['cargo','test'])",
    )
    .unwrap();
    fs::write(&fixture.rules,json!({"base_dir":fixture.root.path(),"python_calls":["subprocess.run"],
        "entries":[{"argv":["cargo","test"],"label":"inner suite"}],"feedback":"focused tasks only"}).to_string()).unwrap();
    fixture.bind("root", "root", Some(&fixture.rules));
    let mut event = fixture.event("root", None, "python3 check.py");
    assert_eq!(fixture.call(&event), json!({}));
    event["cwd"] = json!(sub);
    assert!(denied(&fixture.call(&event)).contains("inner suite"));
}
