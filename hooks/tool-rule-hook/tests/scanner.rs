//! Static scanner regressions; analyzed commands are never executed.
#[path = "support/review.rs"]
mod review;
use serde_json::json;
use std::{fs, path::Path};
use tool_rule_hook::{self as tool_policy, Policy};
fn policy() -> Policy {
    serde_json::from_value(json!({
        "base_dir":"/tmp",
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
