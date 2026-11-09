//! Rust scanner/CLI/configuration simulations; no real Agent or native Hook is run.
use crewmoor::{
    config::{AgentKind, Config},
    feedback::{self, Evidence, Registration},
    tool_policy::{self, Policy},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn policy() -> Policy {
    serde_json::from_value(json!({
        "checker":env!("CARGO_BIN_EXE_crewmoor"),
        "entries":[{"argv":["cargo","test"],"label":"full Cargo tests"},{"argv":["cargo","llvm-cov"],"label":"coverage"},{"argv":["make","full-check"],"label":"project full check"},{"argv":["full.sh"],"label":"known script"}],
        "allow":[["cargo","test","--test","message","only_this_test"]],
        "feedback":"Full checks belong to the submission role.","timeout_ms":3000
    })).unwrap()
}
fn scan(command: &str) -> tool_policy::Scan {
    tool_policy::inspect(&policy(), Path::new("/tmp"), command).unwrap()
}
#[test]
fn direct_wrapped_combined_and_configured_project_entries_match() {
    for command in [
        "cargo test",
        "cargo test --nocapture",
        "cargo test --test message arbitrary",
        "cargo +stable test",
        "cargo --color always test",
        "cargo --manifest-path other/Cargo.toml test",
        "cargo --color=never test",
        "/usr/bin/cargo test --locked",
        "env A=one cargo test",
        "ROLE=release cargo test",
        "env -i -u NAME cargo test",
        "env -- cargo test",
        "command -p cargo test",
        "exec -a other-name cargo test",
        "exec -c cargo test",
        "command cargo test",
        "nohup cargo test",
        "bash -lc 'cargo test'",
        "echo data; cargo llvm-cov",
        "true && (cargo test)",
        "false || cargo test",
        "printf data | cargo test",
        "echo $(cargo test)",
        "make full-check",
        "./scripts/full.sh",
    ] {
        assert!(scan(command).entry.is_some(), "{command}");
    }
}
#[test]
fn exceptions_are_exact_and_must_have_a_real_cargo_target_or_filter() {
    for command in [
        "cargo test --test message only_this_test",
        "cargo +stable test --test message only_this_test",
    ] {
        assert!(scan(command).entry.is_none());
    }
    for args in [
        vec!["cargo", "test"],
        vec!["cargo", "test", "--nocapture"],
        vec!["cargo", "test", "--test", "*"],
        vec!["cargo", "test", "--test", "message", "--tests"],
    ] {
        let mut policy = policy();
        policy.allow = vec![args.into_iter().map(String::from).collect()];
        assert!(policy.validate().is_err());
    }
    let mut policy = policy();
    policy.allow = vec![vec![
        "cargo".into(),
        "test".into(),
        "specific_filter".into(),
    ]];
    policy.validate().unwrap();
    assert!(
        tool_policy::inspect(&policy, Path::new("/tmp"), "cargo test specific_filter")
            .unwrap()
            .entry
            .is_none()
    );
}
#[test]
fn python_string_list_tuple_nested_and_configured_calls_share_the_same_rules() {
    for source in [
        "import subprocess; subprocess.run('cargo test', shell=True)",
        "import subprocess; subprocess.run(['cargo', 'test'])",
        "import subprocess; subprocess.run(('cargo', 'test'))",
        "import subprocess; subprocess.check_call(args=['cargo', 'llvm-cov'])",
        "import os; os.system('cargo test')",
        "import subprocess; subprocess.Popen(['bash', '-c', 'cargo test'])",
        "import subprocess; subprocess.run(['cargo', 'test', dynamic_filter])",
    ] {
        let command = format!("python3 -c {}", shlex::try_quote(source).unwrap());
        assert!(scan(&command).entry.is_some(), "{source}");
    }
    let command = "python -c 'import subprocess; subprocess.run([\"cargo\", \"test\", \"--test\", \"message\", \"only_this_test\"])'";
    assert!(scan(command).entry.is_none());
    let mut policy = policy();
    policy.python_calls.push("custom.launch".into());
    let result = tool_policy::inspect(
        &policy,
        Path::new("/tmp"),
        "python -c 'custom.launch((\"make\", \"full-check\"))'",
    )
    .unwrap();
    assert_eq!(result.entry.as_deref(), Some("project full check"));
}
#[test]
fn python_files_stdin_heredoc_and_herestring_sources_are_static() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("checks.py"),
        "import subprocess\nsubprocess.run(['cargo', 'test'])\n",
    )
    .unwrap();
    for command in [
        "python3 checks.py",
        "python3 - < checks.py",
        "python3 < checks.py",
        "python3 - <<'PY'\nimport subprocess\nsubprocess.run(['cargo', 'test'])\nPY\n",
        "python3 - <<< 'import os; os.system(\"cargo test\")'",
        "env A=one python3 - <<'PY'\nimport subprocess\nsubprocess.run(('cargo', 'llvm-cov'))\nPY\n",
    ] {
        let result = tool_policy::inspect(&policy(), directory.path(), command).unwrap();
        assert!(result.entry.is_some(), "{command}");
    }
}
#[test]
fn comments_data_and_normal_python_do_not_become_commands() {
    for command in [
        "echo 'cargo test'",
        "# cargo test\necho done",
        "printf '%s' '{\"command\":\"cargo test\"}'",
        "python -c '# subprocess.run([\"cargo\", \"test\"])\nprint(42)'",
        "python -c 'data=[\"cargo\", \"test\"]; text=\"cargo test\"; print(data, text)'",
        "python -c 'import subprocess; subprocess.run([\"echo\", \"cargo test\"])'",
        "cat < /dev/null",
        "command -v cargo test",
        "python -c 'print(42)' < /not-read-for-c-source",
    ] {
        assert!(scan(command).entry.is_none(), "{command}");
    }
}
#[test]
fn unreadable_invalid_missing_and_dynamic_inputs_have_explicit_boundaries() {
    for command in [
        "python missing.py",
        "python -",
        "python -c '('",
        "bash -c",
        "echo 'unterminated",
        "cd sub && python relative.py",
    ] {
        assert!(
            tool_policy::inspect(&policy(), Path::new("/tmp"), command).is_err(),
            "{command}"
        );
    }
    for command in [
        "python -c 'import subprocess; subprocess.run(command)'",
        "python -m indirect",
        "env -S 'cargo test'",
        "bash indirect.sh",
        "$DYNAMIC command",
    ] {
        let result = scan(command);
        assert!(result.entry.is_none());
        assert!(result.unresolved);
    }
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("unknown.py");
    fs::write(&script, "value = 'cargo ' + 'test'; print(value)").unwrap();
    assert!(
        tool_policy::inspect(&policy(), directory.path(), "python unknown.py")
            .unwrap()
            .entry
            .is_none()
    );
    // Static scan is not pinned execution: this file can change after the check.
    fs::write(script, "import os; os.system('cargo test')").unwrap();
    assert!(
        tool_policy::inspect(&policy(), directory.path(), "python unknown.py")
            .unwrap()
            .entry
            .is_some()
    );
}

struct Fixture {
    directory: tempfile::TempDir,
    config: Config,
    snapshot: std::path::PathBuf,
}
impl Fixture {
    fn new(agent: AgentKind) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let context = feedback::register(
            root,
            Registration {
                team: "policy-team".into(),
                role: "builder".into(),
                task: "issue-65".into(),
                cwd: root.into(),
                agent: agent.clone(),
                agent_version: "synthetic".into(),
                evidence: Evidence::Synthetic,
                native_session_id: Some("native-main".into()),
            },
        )
        .unwrap();
        let mut config: Config = serde_json::from_value(json!({"team":"policy-team","role":"builder","runtime":"tui","cwd":root,"store":root,"runtime_binary":"/bin/false","runtime_version":"synthetic","agent":agent,"agent_executable":"/bin/false","agent_version":"synthetic","feedback_registry":root,"feedback_instance":context.instance})).unwrap();
        let guard = tool_policy::prepare(&config, &policy(), root, &context).unwrap();
        let snapshot = guard.snapshot.clone();
        config.tool_guard = Some(guard);
        Self {
            directory,
            config,
            snapshot,
        }
    }
    fn input(&self, command: &str) -> Value {
        json!({"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":command},"session_id":"native-main","cwd":self.directory.path()})
    }
    fn run(&self, mode: &str, input: &Value) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
            .args(["tool-policy", mode, self.snapshot.to_str().unwrap()])
            .env(
                feedback::INSTANCE_ENV,
                self.config.feedback_instance.as_ref().unwrap(),
            )
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
        child.wait_with_output().unwrap()
    }
}
fn native_decision(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn native_cli_returns_deny_reason_and_checks_fixed_role_session_before_scanning() {
    for kind in [AgentKind::Codex, AgentKind::Claude] {
        let fixture = Fixture::new(kind);
        let mut role_claim = fixture.input("cargo test");
        role_claim["role"] = json!("submission");
        role_claim["agent_type"] = json!("submission");
        let denied = native_decision(fixture.run("hook", &role_claim));
        assert_eq!(denied["hookSpecificOutput"]["permissionDecision"], "deny");
        let reason = denied["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        assert!(reason.contains("builder"));
        assert!(reason.contains("full Cargo tests"));
        assert!(reason.contains("禁止换脚本"));
        let clear = native_decision(fixture.run("hook", &fixture.input("echo data")));
        assert!(clear.get("hookSpecificOutput").is_none());
        let mut input = fixture.input("echo data");
        input["session_id"] = json!("another-session");
        let denied = native_decision(fixture.run("hook", &input));
        assert!(
            denied["hookSpecificOutput"]["permissionDecisionReason"]
                .as_str()
                .unwrap()
                .contains("session mismatch")
        );
    }
}

#[test]
fn claude_explicit_settings_are_merged_into_a_new_file_without_editing_the_source() {
    let mut fixture = Fixture::new(AgentKind::Claude);
    let source = fixture.directory.path().join("user-settings.json");
    let original = json!({"env":{"EXAMPLE":"preserved"},"hooks":{"PreToolUse":[{"matcher":"Read","hooks":[{"type":"command","command":"existing-handler"}]}]}}).to_string();
    fs::write(&source, &original).unwrap();
    fixture.config.agent_args = vec![
        "--model".into(),
        "test-model".into(),
        "--settings".into(),
        source.display().to_string(),
    ];
    let root = fixture.directory.path().join("another-launch");
    fs::create_dir(&root).unwrap();
    let snapshot: Value = serde_json::from_slice(&fs::read(&fixture.snapshot).unwrap()).unwrap();
    let context: feedback::Context = serde_json::from_value(snapshot["context"].clone()).unwrap();
    let guard = tool_policy::prepare(&fixture.config, &policy(), &root, &context).unwrap();
    let document: Value =
        serde_json::from_slice(&fs::read(guard.settings.as_ref().unwrap()).unwrap()).unwrap();
    assert_eq!(document["env"]["EXAMPLE"], "preserved");
    assert_eq!(document["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read_to_string(source).unwrap(), original);
    fixture.config.tool_guard = Some(guard);
    let args = fixture.config.native_args();
    assert_eq!(
        args.iter()
            .filter(|arg| arg.as_str() == "--settings")
            .count(),
        1
    );
    assert_eq!(&args[..2], &["--model", "test-model"]);
}

#[test]
fn malformed_disabled_unknown_role_and_source_limits_are_reported() {
    let fixture = Fixture::new(AgentKind::Codex);
    let input = fixture.input("echo data");
    let mut snapshot: Value =
        serde_json::from_slice(&fs::read(&fixture.snapshot).unwrap()).unwrap();
    snapshot["context"]["registration"]["role"] = json!("foreign-role");
    fs::write(&fixture.snapshot, snapshot.to_string()).unwrap();
    let result = native_decision(fixture.run("hook", &input));
    assert!(
        result["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("identity changed")
    );
    assert!(tool_policy::inspect(&policy(), Path::new("/tmp"), &"x".repeat(1_048_577)).is_err());
    assert!(
        tool_policy::inspect(
            &policy(),
            Path::new("/tmp"),
            &format!("{}cargo test", "env ".repeat(30))
        )
        .is_err()
    );
    let mut invalid = policy();
    invalid.entries.clear();
    assert!(invalid.validate().is_err());
    invalid = policy();
    invalid.checker = fixture.directory.path().join("absent-checker");
    assert!(invalid.validate().is_err());
}
#[test]
fn opencode_bridge_protocol_checks_native_session_and_returns_rust_decisions() {
    let fixture = Fixture::new(AgentKind::OpenCode);
    let input = json!({"tool":"shell","sessionID":"native-main","instance":fixture.config.feedback_instance,"input":{"command":"cargo test","workdir":fixture.directory.path()}});
    let result = native_decision(fixture.run("bridge", &input));
    assert_eq!(result["decision"], "deny");
    let mut input = input;
    input["input"]["command"] = json!("echo data");
    assert_eq!(
        native_decision(fixture.run("bridge", &input))["decision"],
        "no_match"
    );
    input["instance"] = json!("wrong-instance");
    assert_eq!(
        native_decision(fixture.run("bridge", &input))["decision"],
        "deny"
    );
}
#[test]
fn checker_errors_produce_valid_denial_instead_of_a_fail_open_exit_code() {
    let fixture = Fixture::new(AgentKind::Codex);
    let output = fixture.run("hook", &json!({}));
    assert_eq!(
        native_decision(output)["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
    fs::write(&fixture.snapshot, "invalid snapshot").unwrap();
    let output = fixture.run("hook", &fixture.input("echo normal"));
    assert_eq!(
        native_decision(output)["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
    fs::remove_file(&fixture.snapshot).unwrap();
    assert_eq!(
        native_decision(fixture.run("hook", &fixture.input("echo normal")))["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
}

#[test]
fn bounded_checker_timeout_returns_a_denial_before_host_timeout() {
    let fixture = Fixture::new(AgentKind::Codex);
    let fifo = fixture.directory.path().join("waiting.py");
    let path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // This owned FIFO deliberately blocks source reading; no Python is executed.
    let created = unsafe { libc::mkfifo(path.as_ptr(), 0o600) };
    assert_eq!(created, 0);
    let mut snapshot: Value =
        serde_json::from_slice(&fs::read(&fixture.snapshot).unwrap()).unwrap();
    snapshot["policy"]["timeout_ms"] = json!(10);
    fs::write(&fixture.snapshot, snapshot.to_string()).unwrap();
    let result = native_decision(fixture.run("hook", &fixture.input("python waiting.py")));
    let reason = result["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap();
    assert!(reason.contains("timed out"));
    assert_eq!(result["hookSpecificOutput"]["permissionDecision"], "deny");
}
#[test]
fn startup_injects_only_the_restricted_role_and_preserves_native_scope() {
    let codex = Fixture::new(AgentKind::Codex);
    let args = codex.config.native_args();
    let inline = args.last().unwrap();
    let doc: toml::Value = toml::from_str(inline).unwrap();
    assert_eq!(
        doc["hooks"]["PreToolUse"][0]["matcher"].as_str(),
        Some("^Bash$")
    );
    assert!(!args.iter().any(|arg| arg.contains("bypass-hook-trust")));
    let mut release = codex.config.clone();
    release.tool_guard = None;
    assert_eq!(release.native_args(), release.agent_args);
    let claude = Fixture::new(AgentKind::Claude);
    let settings: Value = serde_json::from_slice(
        &fs::read(
            claude
                .config
                .tool_guard
                .as_ref()
                .unwrap()
                .settings
                .as_ref()
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        settings["hooks"]["PreToolUse"][0]["hooks"][0]["onFailure"],
        "block"
    );
    assert_eq!(claude.config.native_args()[0], "--settings");
    let opencode = Fixture::new(AgentKind::OpenCode);
    let plugin = opencode
        .config
        .tool_guard
        .as_ref()
        .unwrap()
        .plugin
        .as_ref()
        .unwrap();
    let source = fs::read_to_string(plugin.join("index.ts")).unwrap();
    assert!(source.contains("execute.before"));
    assert!(source.contains("Tool.Error"));
    assert!(!source.contains("cargo test"));
}

#[test]
fn incomplete_generated_guard_configuration_fails_validation() {
    let mut fixture = Fixture::new(AgentKind::Claude);
    fixture.config.tool_guard.as_mut().unwrap().settings = None;
    assert!(fixture.config.validate().is_err());
    let mut fixture = Fixture::new(AgentKind::OpenCode);
    fixture.config.tool_guard.as_mut().unwrap().plugin = None;
    assert!(fixture.config.validate().is_err());
    let mut fixture = Fixture::new(AgentKind::Codex);
    fixture.config.feedback_instance = None;
    assert!(fixture.config.validate().is_err());
}

#[test]
fn app_core_launch_argv_retains_each_agents_process_scoped_guard() {
    for kind in [AgentKind::Codex, AgentKind::Claude] {
        let mut fixture = Fixture::new(kind.clone());
        fixture.config.runtime = crewmoor::config::Runtime::Core;
        fixture.config.app = Some(crewmoor::app::Binding {
            session: "policy-core".into(),
            socket: fixture.directory.path().join("core.sock"),
            core_id: "owned-core".into(),
            machine: "local".into(),
            space_id: None,
            tab_id: None,
        });
        let record = crewmoor::record::Record::prepare(
            fixture.config,
            &fixture.directory.path().join("core-record"),
        )
        .unwrap();
        let argv = crewmoor::core::launch_argv(&record);
        match kind {
            AgentKind::Codex => {
                assert!(argv.iter().any(|arg| arg.starts_with("hooks.PreToolUse=")));
                assert!(argv.iter().any(|arg| arg == "--no-daemon"));
            }
            AgentKind::Claude => assert!(argv.iter().any(|arg| arg == "--settings")),
            AgentKind::OpenCode => unreachable!(),
        }
        // Core injects the role marker through terminal.create.env; argv must retain it.
        let removes_role = argv
            .windows(2)
            .any(|pair| pair[0] == "-u" && pair[1] == feedback::INSTANCE_ENV);
        assert!(!removes_role);
    }
}
