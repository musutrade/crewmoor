//! Real independent checker subprocesses with synthetic native events; no Agent host.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Seek, Write},
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
const SELECTION_ENV: [&str; 3] = [
    "TOOL_RULE_HOOK_RULES",
    "TOOL_RULE_HOOK_CONTEXT",
    "TOOL_RULE_HOOK_VERIFY",
];
struct Fixture {
    root: tempfile::TempDir,
    rules: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let rules = root.path().join("rules.json");
        fs::write(&rules,json!({"base_dir":root.path(),"entries":[{"argv":["cargo","test"],"label":"suite"}],"feedback":{"reason":"Project chooses focused checks.","responsibility":"Owner integrates.","next":"Use one configured target."}}).to_string()).unwrap();
        Self { root, rules }
    }
    fn command(&self, mode: &str, path: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"));
        command
            .arg(mode)
            .env("TOOL_RULE_HOOK_RULES", path)
            .env_remove("TOOL_RULE_HOOK_CONTEXT")
            .env_remove("TOOL_RULE_HOOK_VERIFY")
            .env_remove("CREWMOOR_FEEDBACK_INSTANCE");
        command
    }
    fn event(&self, command: &str) -> Value {
        json!({"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":command},"cwd":self.root.path(),"session_id":"synthetic-independent"})
    }
    fn run(&self, command: &str) -> Value {
        output(self.command("hook", &self.rules), &self.event(command))
    }
    fn edit(&self, edit: impl FnOnce(&mut Value)) {
        let mut rules: Value = serde_json::from_slice(&fs::read(&self.rules).unwrap()).unwrap();
        edit(&mut rules);
        fs::write(&self.rules, rules.to_string()).unwrap();
    }
}
fn spawn(mut command: Command, input: &Value) -> std::process::Child {
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
fn decode(result: Output) -> Value {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn output(command: Command, input: &Value) -> Value {
    decode(spawn(command, input).wait_with_output().unwrap())
}
fn unselected(mut command: Command) -> Command {
    for name in SELECTION_ENV {
        command.env_remove(name);
    }
    command
}
fn raw_output(mut command: Command, bytes: &[u8]) -> Value {
    let mut input = tempfile::tempfile().unwrap();
    input.write_all(bytes).unwrap();
    input.rewind().unwrap();
    decode(command.stdin(Stdio::from(input)).output().unwrap())
}
fn fixed_command(text: &str, mode: &str, data: &Path) -> Command {
    let argv = shlex::split(text).unwrap();
    assert_eq!(argv, [env!("CARGO_BIN_EXE_tool-rule-hook"), mode]);
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    command.env("PLUGIN_DATA", data);
    unselected(command)
}
fn reason(value: &Value) -> &str {
    value["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap()
}
#[test]
fn standalone_native_feedback_and_no_match_need_no_crewmoor() {
    let fixture = Fixture::new();
    let denied = fixture.run("cargo test");
    assert_eq!(denied["hookSpecificOutput"]["permissionDecision"], "deny");
    for detail in [
        "suite",
        "Project chooses focused checks.",
        "Owner integrates.",
        "Use one configured target.",
    ] {
        assert!(reason(&denied).contains(detail));
    }
    assert!(!reason(&denied).contains("submission role"));
    assert_eq!(fixture.run("echo normal"), json!({}));
    let bridge = output(
        fixture.command("bridge", &fixture.rules),
        &json!({"tool":"shell","sessionID":"independent","input":{"command":"$DYNAMIC argument","workdir":fixture.root.path()}}),
    );
    assert_eq!(bridge["decision"], "no_match");
    assert_eq!(bridge["unresolved"], true);
    let bridge = output(
        fixture.command("bridge", &fixture.rules),
        &json!({"tool":"shell","input":{"command":"cargo test"}}),
    );
    assert_eq!(bridge["decision"], "deny");
}
#[test]
fn errors_always_return_checker_denial_and_never_claim_safe_execution() {
    let fixture = Fixture::new();
    for command in ["python absent.py", "python -c '('", "echo 'unterminated"] {
        assert!(reason(&fixture.run(command)).contains("checker failure"));
    }
    let malformed = output(fixture.command("hook", &fixture.rules), &json!({}));
    assert!(reason(&malformed).contains("checker failure"));
    fixture.edit(|value| value["entries"] = json!([]));
    assert!(reason(&fixture.run("echo normal")).contains("checker failure"));
    fs::write(&fixture.rules, "invalid JSON").unwrap();
    assert!(reason(&fixture.run("echo normal")).contains("checker failure"));
    fs::remove_file(&fixture.rules).unwrap();
    assert!(reason(&fixture.run("echo normal")).contains("checker failure"));
}
#[test]
fn verifier_failure_and_half_context_never_fall_back_to_independent_mode() {
    let fixture = Fixture::new();
    for mode in ["hook", "optional-hook"] {
        for verifier in [
            "",
            "[\"/bin/false\"]",
            "[\"/bin/true\"]",
            "invalid JSON",
            "[]",
        ] {
            let mut command = fixture.command(mode, &fixture.rules);
            command.env("TOOL_RULE_HOOK_VERIFY", verifier).env(
                "TOOL_RULE_HOOK_CONTEXT",
                fixture.root.path().join("context.json"),
            );
            let result = output(command, &fixture.event("echo normal"));
            assert!(reason(&result).contains("checker failure"));
        }
        for name in ["TOOL_RULE_HOOK_CONTEXT", "TOOL_RULE_HOOK_VERIFY"] {
            let mut command = fixture.command(mode, &fixture.rules);
            command.env(name, "/context.json");
            assert!(
                reason(&output(command, &fixture.event("echo normal"))).contains("no fallback")
            );
        }
        for context in ["", "relative-context.json"] {
            let mut command = fixture.command(mode, &fixture.rules);
            command
                .env("TOOL_RULE_HOOK_CONTEXT", context)
                .env("TOOL_RULE_HOOK_VERIFY", "[\"/bin/true\"]");
            assert!(
                reason(&output(command, &fixture.event("echo normal"))).contains("checker failure")
            );
        }
    }
}

#[test]
fn selected_optional_hook_verifier_timeout_remains_denial() {
    let fixture = Fixture::new();
    fixture.edit(|rules| rules["timeout_ms"] = json!(25));
    let mut command = fixture.command("optional-hook", &fixture.rules);
    command
        .env(
            "TOOL_RULE_HOOK_CONTEXT",
            fixture.root.path().join("context.json"),
        )
        .env("TOOL_RULE_HOOK_VERIFY", "[\"/bin/sleep\",\"2\"]");
    let denied = output(command, &fixture.event("echo normal"));
    assert!(reason(&denied).contains("context/checker process timed out"));
}

#[test]
fn unselected_optional_hook_returns_no_match_without_parsing_native_input() {
    let fixture = Fixture::new();
    for bytes in [b"".as_slice(), b"{invalid JSON", b"{}", b"null", b"\xff"] {
        let command = unselected(fixture.command("optional-hook", &fixture.rules));
        assert_eq!(raw_output(command, bytes), json!({}));
    }
    let mut command = unselected(fixture.command("optional-hook", &fixture.rules));
    let no_input = decode(command.stdin(Stdio::null()).output().unwrap());
    assert_eq!(no_input, json!({}));
}

#[test]
fn any_selection_variable_including_empty_or_non_utf8_stays_strict() {
    let fixture = Fixture::new();
    let event = fixture.event("echo normal").to_string();
    for mask in 1..8 {
        for value in ["", "/absent-selection"] {
            let mut command = unselected(fixture.command("optional-hook", &fixture.rules));
            for (bit, name) in SELECTION_ENV.into_iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    command.env(name, value);
                }
            }
            let denied = raw_output(command, event.as_bytes());
            assert!(reason(&denied).contains("checker failure"), "mask {mask}");
        }
    }
    for name in SELECTION_ENV {
        let mut command = unselected(fixture.command("optional-hook", &fixture.rules));
        command.env(name, std::ffi::OsString::from_vec(vec![0xff]));
        let denied = raw_output(command, event.as_bytes());
        assert!(reason(&denied).contains("checker failure"), "{name}");
    }
}

#[test]
fn selected_optional_hook_rejects_bad_rules_input_and_deleted_selection() {
    let fixture = Fixture::new();
    let event = fixture.event("echo normal").to_string();
    for path in [
        Path::new(""),
        Path::new("relative.json"),
        fixture.root.path(),
    ] {
        let command = fixture.command("optional-hook", path);
        assert!(reason(&raw_output(command, event.as_bytes())).contains("checker failure"));
    }
    for bytes in [b"".as_slice(), b"{invalid JSON", b"{}", b"null", b"\xff"] {
        let command = fixture.command("optional-hook", &fixture.rules);
        assert!(reason(&raw_output(command, bytes)).contains("checker failure"));
    }
    fixture.edit(|value| value["entries"] = json!([]));
    let command = fixture.command("optional-hook", &fixture.rules);
    assert!(reason(&raw_output(command, event.as_bytes())).contains("checker failure"));
    fs::write(&fixture.rules, "{bad JSON").unwrap();
    let command = fixture.command("optional-hook", &fixture.rules);
    assert!(reason(&raw_output(command, event.as_bytes())).contains("checker failure"));
    fs::remove_file(&fixture.rules).unwrap();
    let command = fixture.command("optional-hook", &fixture.rules);
    assert!(reason(&raw_output(command, event.as_bytes())).contains("checker failure"));
}

#[test]
fn optional_selection_does_not_relax_explicit_hook_bridge_or_compatibility() {
    let fixture = Fixture::new();
    let command = unselected(fixture.command("hook", &fixture.rules));
    assert!(reason(&output(command, &fixture.event("echo normal"))).contains("RULES is missing"));
    let command = unselected(fixture.command("bridge", &fixture.rules));
    let result = output(
        command,
        &json!({"tool":"shell","input":{"command":"echo normal"}}),
    );
    assert_eq!(result["decision"], "deny");
    for mode in ["compat-hook", "compat-bridge"] {
        let command = unselected(fixture.command(mode, &fixture.rules));
        let result = output(command, &json!({"input":fixture.event("echo normal")}));
        let denied = if mode == "compat-hook" {
            result["hookSpecificOutput"]["permissionDecision"].clone()
        } else {
            result["decision"].clone()
        };
        assert_eq!(denied, "deny");
    }
}
#[test]
fn same_cwd_parallel_processes_keep_different_rules_and_load_edits_on_next_call() {
    let fixture = Fixture::new();
    let other = fixture.root.path().join("other.json");
    fs::copy(&fixture.rules, &other).unwrap();
    let mut value: Value = serde_json::from_slice(&fs::read(&other).unwrap()).unwrap();
    value["entries"][0]["argv"] = json!(["unrelated"]);
    value["feedback"] = json!("Other agent policy");
    fs::write(&other, value.to_string()).unwrap();
    let first = spawn(
        fixture.command("hook", &fixture.rules),
        &fixture.event("cargo test"),
    );
    let second = spawn(
        fixture.command("hook", &other),
        &fixture.event("cargo test"),
    );
    assert!(reason(&decode(first.wait_with_output().unwrap())).contains("suite"));
    assert_eq!(decode(second.wait_with_output().unwrap()), json!({}));
    fixture.edit(|rules| rules["feedback"] = json!("changed on next hook invocation"));
    assert!(reason(&fixture.run("cargo test")).contains("changed on next hook invocation"));
}
#[test]
fn scan_timeout_and_packaged_asset_export_are_explicit() {
    let fixture = Fixture::new();
    let fifo = fixture.root.path().join("blocked.py");
    let path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    fixture.edit(|rules| rules["timeout_ms"] = json!(10));
    assert!(reason(&fixture.run("python blocked.py")).contains("timed out"));
    let command = fixture.command("optional-hook", &fixture.rules);
    assert!(reason(&output(command, &fixture.event("python blocked.py"))).contains("timed out"));
    let destination = fixture.root.path().join("exported-plugin");
    let status = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"))
        .arg("export-opencode")
        .arg(&destination)
        .output()
        .unwrap();
    assert!(status.status.success());
    let source = fs::read_to_string(destination.join("index.ts")).unwrap();
    assert!(source.contains("execute.before"));
    assert!(source.contains("[\"bridge-session\", store, scope]"));
    assert!(!source.contains("tool-policy"));
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"))
            .arg("export-opencode")
            .arg(&destination)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn exported_codex_marketplace_and_claude_hook_references_resolve_with_fixed_commands() {
    let fixture = Fixture::new();
    let root = tempfile::tempdir().unwrap();
    let catalog = root.path().join("catalog");
    let result = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"))
        .arg("export-marketplace")
        .arg(&catalog)
        .output()
        .unwrap();
    assert!(result.status.success());
    let document: Value = serde_json::from_slice(
        &fs::read(catalog.join(".agents/plugins/marketplace.json")).unwrap(),
    )
    .unwrap();
    let relative = document["plugins"][0]["source"]["path"].as_str().unwrap();
    let plugin = catalog.join(relative);
    let manifest: Value =
        serde_json::from_slice(&fs::read(plugin.join(".codex-plugin/plugin.json")).unwrap())
            .unwrap();
    assert_eq!(document["plugins"][0]["name"], manifest["name"]);
    let hook_path = plugin.join(manifest["hooks"].as_str().unwrap());
    for path in [hook_path, plugin.join("hooks/hooks.json")] {
        let codex = path.file_name().unwrap() == "codex-hooks.json";
        let mode = if codex { "codex-hook" } else { "optional-hook" };
        let data = fixture.root.path().join("native-codex-data");
        let hooks: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let command = hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(command.ends_with(&format!(" {mode}")));
        assert!(!command.contains("RULES"));
        assert!(!command.contains("stop-that-shit"));
        let ordinary = fixture.event("echo normal").to_string();
        let restricted = fixture.event("cargo test").to_string();
        assert_eq!(
            raw_output(fixed_command(command, mode, &data), ordinary.as_bytes()),
            json!({})
        );
        assert_eq!(
            raw_output(fixed_command(command, mode, &data), restricted.as_bytes()),
            json!({})
        );
        if codex {
            let mut control = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"));
            control
                .arg("codex-session-bind")
                .arg(data.join("tool-rule-hook"));
            let selection = json!({"scope":"synthetic-independent","session":"synthetic-independent","directory":fixture.root.path(),"rules":fixture.rules});
            output(control, &selection);
        }
        let mut selected = fixed_command(command, mode, &data);
        if !codex {
            selected.env("TOOL_RULE_HOOK_RULES", &fixture.rules);
        }
        assert_eq!(raw_output(selected, ordinary.as_bytes()), json!({}));
        let mut selected = fixed_command(command, mode, &data);
        if !codex {
            selected.env("TOOL_RULE_HOOK_RULES", &fixture.rules);
        }
        let denied = raw_output(selected, restricted.as_bytes());
        assert_eq!(denied["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(reason(&denied).contains("Project chooses focused checks."));
    }
    assert!(plugin.join(".claude-plugin/plugin.json").is_file());
}

#[test]
fn legacy_adapter_envelope_uses_the_same_independent_scanner_and_feedback() {
    let fixture = Fixture::new();
    let policy: Value = serde_json::from_slice(&fs::read(&fixture.rules).unwrap()).unwrap();
    let event = json!({"policy":policy,"input":fixture.event("cargo test"),"context":"synthetic legacy adapter"});
    let command = unselected(fixture.command("compat-hook", &fixture.rules));
    let result = output(command, &event);
    assert_eq!(result["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(reason(&result).contains("synthetic legacy adapter"));
    assert!(reason(&result).contains("Project chooses focused checks."));
}

fn update_rules(destination: &Path, source: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"))
        .arg("rules-update")
        .arg(destination)
        .arg(source)
        .output()
        .unwrap()
}

#[test]
fn rules_update_publishes_loadable_rules_and_changes_actual_checker_behavior() {
    let fixture = Fixture::new();
    let original_denial = fixture.run("cargo test");
    assert_eq!(
        original_denial["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
    let source = fixture.root.path().join("candidate.json");
    let mut candidate: Value = serde_json::from_slice(&fs::read(&fixture.rules).unwrap()).unwrap();
    candidate["entries"] = json!([{"argv":["npm","run","build"],"label":"published task"}]);
    candidate["feedback"] = json!("The newly published project rule applies.");
    let source_text = candidate.to_string();
    fs::write(&source, &source_text).unwrap();
    let result = update_rules(&fixture.rules, &source);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let validated = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"))
        .arg("validate")
        .arg(&fixture.rules)
        .output()
        .unwrap();
    assert!(validated.status.success());
    assert_eq!(fixture.run("cargo test"), json!({}));
    let new_denial = fixture.run("npm run build");
    assert_eq!(
        new_denial["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
    assert!(reason(&new_denial).contains("published task"));
    assert!(reason(&new_denial).contains("The newly published project rule applies."));
    assert_eq!(fs::read_to_string(&source).unwrap(), source_text);
}

fn assert_live_rules_unchanged(fixture: &Fixture, bytes: &[u8], decision: &Value) {
    assert_eq!(fs::read(&fixture.rules).unwrap(), bytes);
    assert_eq!(fixture.run("cargo test"), *decision);
    assert_eq!(fixture.run("echo normal"), json!({}));
}

#[test]
fn rules_update_rejects_invalid_or_missing_sources_and_preserves_live_policy() {
    let fixture = Fixture::new();
    let bytes = fs::read(&fixture.rules).unwrap();
    let decision = fixture.run("cargo test");
    let mut invalid_policy: Value = serde_json::from_slice(&bytes).unwrap();
    invalid_policy["entries"] = json!([]);
    let source = fixture.root.path().join("invalid-source.json");
    for text in ["{invalid JSON".to_owned(), invalid_policy.to_string()] {
        fs::write(&source, text).unwrap();
        let result = update_rules(&fixture.rules, &source);
        assert!(!result.status.success());
        assert_live_rules_unchanged(&fixture, &bytes, &decision);
    }
    let missing = fixture.root.path().join("missing-source.json");
    let result = update_rules(&fixture.rules, &missing);
    assert!(!result.status.success());
    assert_live_rules_unchanged(&fixture, &bytes, &decision);
}

#[test]
fn rules_update_reports_missing_destination_parent_without_creating_partial_rules() {
    let fixture = Fixture::new();
    let bytes = fs::read(&fixture.rules).unwrap();
    let decision = fixture.run("cargo test");
    let missing_parent = fixture.root.path().join("not-created");
    let destination = missing_parent.join("rules.json");
    let result = update_rules(&destination, &fixture.rules);
    assert!(!result.status.success());
    assert!(!missing_parent.exists());
    assert_live_rules_unchanged(&fixture, &bytes, &decision);
}
