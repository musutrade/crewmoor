//! CrewMoor selection/identity/launch simulations; no real native host is run.
#[path = "support/rule_selection.rs"]
mod rule_selection;
use crewmoor::{
    config::{AgentKind, Config, Runtime},
    feedback::{self, Evidence, Registration},
    tool_policy::{self, Policy},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};
struct Fixture {
    directory: tempfile::TempDir,
    config: Config,
    context: feedback::Context,
}
impl Fixture {
    fn new(agent: AgentKind) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let context = feedback::register(
            root,
            Registration {
                team: "policy-team".into(),
                role: "arbitrary-builder".into(),
                task: "issue-68".into(),
                cwd: root.into(),
                agent: agent.clone(),
                agent_version: "synthetic".into(),
                evidence: Evidence::Synthetic,
                native_session_id: Some("native-main".into()),
            },
        )
        .unwrap();
        let mut config: Config = serde_json::from_value(json!({"team":"policy-team","role":"arbitrary-builder","runtime":"tui","cwd":root,"store":root,"runtime_binary":"/bin/false","runtime_version":"synthetic","agent":agent,"agent_executable":"/bin/false","agent_version":"synthetic","feedback_registry":root,"feedback_instance":context.instance})).unwrap();
        config.tool_guard = Some(
            tool_policy::prepare(&config, &rule_selection::policy(root), root, &context).unwrap(),
        );
        Self {
            directory,
            config,
            context,
        }
    }
    fn event(&self) -> Value {
        json!({"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"echo data"},"session_id":"native-main","cwd":self.directory.path()})
    }
    fn invoke(&self, args: &[&str], value: &Value, marker: &str) -> Output {
        let guard = self.config.tool_guard.as_ref().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
            .args(args)
            .env(feedback::INSTANCE_ENV, marker)
            .env("TOOL_RULE_HOOK_CONTEXT", &guard.snapshot)
            .env_remove("TOOL_RULE_HOOK_EXECUTABLE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(value.to_string().as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}
#[test]
fn verifier_preserves_role_instance_session_and_agent_checks() {
    for kind in [AgentKind::Codex, AgentKind::Claude, AgentKind::OpenCode] {
        let fixture = Fixture::new(kind.clone());
        let mut event = fixture.event();
        let bridge = kind == AgentKind::OpenCode;
        if bridge {
            event = json!({"tool":"shell","sessionID":"native-main","instance":fixture.context.instance,"input":{"command":"echo data"}});
        }
        event["role"] = json!("release");
        let envelope = json!({"bridge":bridge,"input":event});
        let result = fixture.invoke(
            &["tool-policy", "verify"],
            &envelope,
            &fixture.context.instance,
        );
        assert!(result.status.success());
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert!(
            value["context"]
                .as_str()
                .unwrap()
                .contains("arbitrary-builder")
        );
        assert!(
            !fixture
                .invoke(&["tool-policy", "verify"], &envelope, "wrong-instance")
                .status
                .success()
        );
        let mut changed = envelope.clone();
        let field = if bridge { "sessionID" } else { "session_id" };
        changed["input"][field] = json!("other-session");
        assert!(
            !fixture
                .invoke(
                    &["tool-policy", "verify"],
                    &changed,
                    &fixture.context.instance
                )
                .status
                .success()
        );
        changed = envelope;
        changed["bridge"] = json!(!bridge);
        assert!(
            !fixture
                .invoke(
                    &["tool-policy", "verify"],
                    &changed,
                    &fixture.context.instance
                )
                .status
                .success()
        );
    }
}
#[test]
fn changed_registry_snapshot_and_missing_context_are_never_standalone_fallback() {
    let fixture = Fixture::new(AgentKind::Codex);
    let guard = fixture.config.tool_guard.as_ref().unwrap();
    let mut snapshot: Value = serde_json::from_slice(&fs::read(&guard.snapshot).unwrap()).unwrap();
    snapshot["context"]["registration"]["role"] = json!("foreign");
    fs::write(&guard.snapshot, snapshot.to_string()).unwrap();
    let result = fixture.invoke(
        &["tool-policy", "verify"],
        &json!({"bridge":false,"input":fixture.event()}),
        &fixture.context.instance,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("identity changed"));
    fs::remove_file(&guard.snapshot).unwrap();
    assert!(
        !fixture
            .invoke(
                &["tool-policy", "verify"],
                &json!({"bridge":false,"input":fixture.event()}),
                &fixture.context.instance
            )
            .status
            .success()
    );
}
#[test]
fn legacy_cli_without_explicit_checker_returns_native_denial() {
    let fixture = Fixture::new(AgentKind::Codex);
    let guard = fixture.config.tool_guard.as_ref().unwrap();
    let output = fixture.invoke(
        &["tool-policy", "hook", guard.snapshot.to_str().unwrap()],
        &fixture.event(),
        &fixture.context.instance,
    );
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(
        value["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("TOOL_RULE_HOOK_EXECUTABLE")
    );
}
#[test]
fn fixed_commands_process_environment_and_shared_plugin_preserve_role_isolation() {
    let first = Fixture::new(AgentKind::Codex);
    let second = Fixture::new(AgentKind::Codex);
    let mut config = second.config.clone();
    config.tool_guard.as_mut().unwrap().checker =
        first.config.tool_guard.as_ref().unwrap().checker.clone();
    assert_eq!(first.config.native_args(), config.native_args());
    assert!(config.native_args().contains(&"--no-daemon".into()));
    let first_env = first.config.tool_guard.as_ref().unwrap().environment();
    let second_env = config.tool_guard.as_ref().unwrap().environment();
    assert_ne!(first_env, second_env);
    for fixture in [first, second] {
        let record = crewmoor::record::Record::prepare(
            fixture.config.clone(),
            &fixture.directory.path().join("record"),
        )
        .unwrap();
        let workspace = crewmoor::tui::workspace_args(&record);
        for (key, value) in fixture.config.tool_guard.as_ref().unwrap().environment() {
            assert!(workspace.contains(&format!("{key}={value}")));
        }
        let mut core = fixture.config.clone();
        core.runtime = Runtime::Core;
        let argv = crewmoor::app::child_argv(&core);
        assert!(
            argv.iter()
                .any(|arg| arg.starts_with("TOOL_RULE_HOOK_RULES="))
        );
        core.tool_guard = None;
        let plain = crewmoor::app::child_argv(&core);
        assert!(
            !plain
                .iter()
                .any(|arg| arg.starts_with("TOOL_RULE_HOOK_RULES="))
        );
    }
    let open = Fixture::new(AgentKind::OpenCode);
    let guard = open.config.tool_guard.as_ref().unwrap();
    assert_eq!(
        guard.plugin.as_ref().unwrap(),
        &open.directory.path().join("shared-rule-plugin")
    );
    assert!(!open.directory.path().join("tool-policy-opencode").exists());
}
#[test]
fn claude_settings_are_merged_and_source_preserved() {
    let mut fixture = Fixture::new(AgentKind::Claude);
    let source = fixture.directory.path().join("native.json");
    let original = json!({"env":{"KEEP":"yes"},"hooks":{"PreToolUse":[{"matcher":"Read","hooks":[{"type":"command","command":"existing"}]}]}}).to_string();
    fs::write(&source, &original).unwrap();
    fixture.config.agent_args = vec!["--settings".into(), "native.json".into()];
    let root = fixture.directory.path().join("new-launch");
    fs::create_dir(&root).unwrap();
    let guard = tool_policy::prepare(
        &fixture.config,
        &rule_selection::policy(fixture.directory.path()),
        &root,
        &fixture.context,
    )
    .unwrap();
    let settings: Value =
        serde_json::from_slice(&fs::read(guard.settings.as_ref().unwrap()).unwrap()).unwrap();
    assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
    assert_eq!(settings["env"]["KEEP"], "yes");
    for (key, value) in guard.environment() {
        assert_eq!(settings["env"][&key], value);
    }
    assert_eq!(
        settings["hooks"]["PreToolUse"][1]["hooks"][0]["onFailure"],
        "block"
    );
    assert_eq!(fs::read_to_string(source).unwrap(), original);
}
#[test]
fn selection_paths_resolve_from_role_cwd_and_old_inline_configs_require_migration() {
    let dir = tempfile::tempdir().unwrap();
    let policy: Policy =
        serde_json::from_value(json!({"checker":"bin/hook","rules":"rules.json"})).unwrap();
    let resolved = policy.resolve(dir.path());
    assert_eq!(resolved.rules, dir.path().join("rules.json"));
    assert_eq!(resolved.checker, dir.path().join("bin/hook"));
    assert!(serde_json::from_value::<Policy>(json!({"checker":"/bin/true","entries":[]})).is_err());
    let mut invalid = rule_selection::policy(dir.path());
    invalid.checker = dir.path().join("missing");
    assert!(invalid.validate().is_err());
    let mut fixture = Fixture::new(AgentKind::Claude);
    fixture.config.tool_guard.as_mut().unwrap().settings = None;
    assert!(fixture.config.validate().is_err());
}

#[test]
fn claude_conflicting_process_selection_settings_are_rejected_without_source_edits() {
    let fixture = Fixture::new(AgentKind::Claude);
    let mut config = fixture.config.clone();
    for input in [
        "{\"env\":false}",
        "{\"env\":{\"TOOL_RULE_HOOK_RULES\":\"/other/rules.json\"}}",
    ] {
        config.agent_args = vec!["--settings".into(), input.into()];
        let root = fixture.directory.path().join("invalid-launch");
        assert!(
            tool_policy::prepare(
                &config,
                &rule_selection::policy(fixture.directory.path()),
                &root,
                &fixture.context
            )
            .is_err()
        );
        assert!(!root.exists());
    }
}

#[test]
fn legacy_snapshot_forwarding_preserves_identity_and_translates_into_independent_rules() {
    for kind in [AgentKind::Codex, AgentKind::OpenCode] {
        let fixture = Fixture::new(kind.clone());
        let guard = fixture.config.tool_guard.as_ref().unwrap();
        let mut snapshot: Value =
            serde_json::from_slice(&fs::read(&guard.snapshot).unwrap()).unwrap();
        snapshot["policy"] = json!({"checker":"/old/crewmoor","entries":[{"argv":["cargo","test"],"label":"legacy tests"}],"feedback":"Old explicit policy"});
        fs::write(&guard.snapshot, snapshot.to_string()).unwrap();
        let bridge = kind == AgentKind::OpenCode;
        let event = if bridge {
            json!({"tool":"shell","sessionID":"native-main","instance":fixture.context.instance,"input":{"command":"echo normal"}})
        } else {
            fixture.event()
        };
        let mode = if bridge { "bridge" } else { "hook" };
        let mut child = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
            .args(["tool-policy", mode, guard.snapshot.to_str().unwrap()])
            .env(feedback::INSTANCE_ENV, &fixture.context.instance)
            .env(
                "TOOL_RULE_HOOK_EXECUTABLE",
                env!("CARGO_BIN_EXE_transport-fixture"),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(event.to_string().as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success());
        let result: Value = serde_json::from_slice(&result.stdout).unwrap();
        if bridge {
            assert_eq!(result["decision"], "no_match");
        } else {
            assert_eq!(result, json!({}));
        }
    }
}
