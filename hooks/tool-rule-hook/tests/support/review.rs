//! Static regression cases from the independent #65 review. No analyzed code runs.
use super::{Policy, fs, policy, tool_policy};
use std::path::Path;

#[test]
fn ansi_c_literal_arguments_and_concatenations_match_their_actual_words() {
    for command in [
        r"cargo $'test'",
        r"$'cargo' $'test'",
        r"c$'arg'o te$'st'",
        r"c$''argo test",
        r"cargo $'\x74\145\u0073\U00000074'",
    ] {
        let scan = tool_policy::inspect(&policy(), Path::new("/tmp"), command).unwrap();
        assert_eq!(scan.entry.as_deref(), Some("full Cargo tests"), "{command}");
        assert!(!scan.unresolved);
    }
    for command in [
        r"echo $'cargo test'",
        r"echo 汉字$'data'",
        r"cargo $'test\n'",
        r"echo $'\cA'",
        r"echo $'\c?'",
    ] {
        assert!(
            tool_policy::inspect(&policy(), Path::new("/tmp"), command)
                .unwrap()
                .entry
                .is_none()
        );
    }
    for command in [
        r"cargo $'\qtest'",
        r"cargo $'test\0suffix'",
        r"echo $'\xff'",
    ] {
        assert!(tool_policy::inspect(&policy(), Path::new("/tmp"), command).is_err());
    }
}
fn python_command(source: &str) -> String {
    format!("python -c {}", shlex::try_quote(source).unwrap())
}

#[test]
fn child_process_cwd_resolves_nested_sources_without_leaking_to_sibling_calls() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("sub")).unwrap();
    fs::write(root.path().join("checks.py"), "print('ordinary root file')").unwrap();
    fs::write(
        root.path().join("sub/checks.py"),
        "import subprocess; subprocess.run(['cargo', 'test'])",
    )
    .unwrap();
    for source in [
        "import subprocess; subprocess.run(['python', 'checks.py'], cwd='sub')",
        "import subprocess; subprocess.run(('python', 'checks.py'), cwd='sub')",
        "import subprocess; subprocess.run('python checks.py', shell=True, cwd='sub')",
        "import subprocess; subprocess.run('python - < checks.py', shell=True, cwd='sub')",
    ] {
        let scan = tool_policy::inspect(&policy(), root.path(), &python_command(source)).unwrap();
        assert_eq!(scan.entry.as_deref(), Some("full Cargo tests"));
    }
    let absolute = serde_json::to_string(&root.path().join("sub")).unwrap();
    let source =
        format!("import subprocess; subprocess.run(['python','checks.py'], cwd={absolute})");
    assert!(
        tool_policy::inspect(&policy(), root.path(), &python_command(&source))
            .unwrap()
            .entry
            .is_some()
    );
    let source = "import subprocess; subprocess.run(['echo','data'], cwd='sub'); subprocess.run(['python','checks.py'], cwd=None)";
    assert!(
        tool_policy::inspect(&policy(), root.path(), &python_command(source))
            .unwrap()
            .entry
            .is_none()
    );
    for source in [
        "import subprocess; subprocess.run(['python','checks.py'], cwd=directory)",
        "import subprocess; subprocess.run(['python','checks.py'], **options)",
        "import subprocess; subprocess.run('python - < checks.py', shell=True, cwd=directory)",
    ] {
        assert!(tool_policy::inspect(&policy(), root.path(), &python_command(source)).is_err());
    }
    let source = "import subprocess; subprocess.run(['cargo','test'], cwd=directory)";
    assert!(
        tool_policy::inspect(&policy(), root.path(), &python_command(source))
            .unwrap()
            .entry
            .is_some()
    );
}

#[test]
fn shell_interpreter_options_stop_at_script_arguments_and_option_separator() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("print.sh"), "printf '%s\\n' \"$@\"\n").unwrap();
    for command in [
        "bash print.sh -c 'cargo test'",
        "bash -e print.sh -c 'cargo test'",
        "bash -- print.sh -c 'cargo test'",
        "bash -- -c 'cargo test'",
        "bash print.sh -lc 'cargo test'",
    ] {
        let scan = tool_policy::inspect(&policy(), root.path(), command).unwrap();
        assert!(scan.entry.is_none(), "{command}");
        assert!(scan.unresolved);
    }
    for command in [
        "bash -c 'cargo test'",
        "bash -e -c 'cargo test'",
        "bash -c -e 'cargo test'",
        "bash -c -- 'cargo test'",
        "bash -c - 'cargo test'",
        "bash -e -s <<'SH'\ncargo test\nSH\n",
    ] {
        assert!(
            tool_policy::inspect(&policy(), root.path(), command)
                .unwrap()
                .entry
                .is_some(),
            "{command}"
        );
    }
}
fn with_exception(args: &[&str]) -> Policy {
    let mut policy = policy();
    policy.allow = vec![args.iter().map(|arg| (*arg).into()).collect()];
    policy
}
#[test]
fn every_cargo_target_selector_is_concrete_before_the_test_argument_separator() {
    for args in [
        vec!["cargo", "test", "--test", "message", "--test", "*"],
        vec!["cargo", "test", "--test", "message", "--test=*"],
        vec!["cargo", "test", "filter", "--test=message", "--test", "?"],
        vec!["cargo", "test", "--test", "message", "--test"],
        vec!["cargo", "test", "--test=message", "--test="],
    ] {
        assert!(with_exception(&args).validate().is_err(), "{args:?}");
    }
    for args in [
        vec![
            "cargo",
            "test",
            "--test",
            "message",
            "--test",
            "core_message",
        ],
        vec!["cargo", "test", "--test=message"],
        vec!["cargo", "test", "--test", "message", "--", "--test", "*"],
        vec!["cargo", "test", "--test", "message", "--", "--tests"],
    ] {
        with_exception(&args).validate().unwrap();
    }
}
