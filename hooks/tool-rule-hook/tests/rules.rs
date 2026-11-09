use serde_json::json;
use std::{fs, path::Path};
use tool_rule_hook::{Policy, inspect};
fn policy(base: &Path) -> Policy {
    serde_json::from_value(json!({"base_dir":base,"entries":[{"argv":["cargo","test"],"label":"Cargo full"},{"argv":["python3","tools/check.py","full"],"label":"Python full"},{"argv":["npm","test"],"label":"npm tests"},{"argv":["npm","run","check"],"label":"npm check"}],"feedback":{"reason":"Avoid duplicate broad checks.","responsibility":"The integration owner runs the full suite.","next":"Use a configured focused target."}})).unwrap()
}
#[test]
fn python_options_and_lexical_paths_match_only_the_configured_project_and_arguments() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path();
    fs::create_dir(base.join("tools")).unwrap();
    fs::write(base.join("tools/check.py"), "print('normal')").unwrap();
    let p = policy(base);
    let absolute = base.join("tools/check.py");
    for command in [
        "python3 tools/check.py full".into(),
        "python3 -B -u ./tools/check.py full".into(),
        "python3 -W ignore -X dev tools/../tools/check.py full".into(),
        format!("python3 -I {} full", absolute.display()),
        "python3 -- tools/check.py full".into(),
    ] {
        assert_eq!(
            inspect(&p, base, &command).unwrap().entry.as_deref(),
            Some("Python full"),
            "{command}"
        );
    }
    assert!(
        inspect(&p, base, "python3 -B tools/check.py fast")
            .unwrap()
            .entry
            .is_none()
    );
    let other = base.join("other");
    fs::create_dir_all(other.join("tools")).unwrap();
    fs::write(other.join("tools/check.py"), "print(1)").unwrap();
    assert!(
        inspect(&p, &other, "python3 tools/check.py full")
            .unwrap()
            .entry
            .is_none()
    );
    assert_eq!(
        inspect(&p, &other, &format!("python3 {} full", absolute.display()))
            .unwrap()
            .entry
            .as_deref(),
        Some("Python full")
    );
    let unknown = inspect(&p, base, "python3 --unknown tools/check.py full").unwrap();
    assert!(unknown.entry.is_none());
    assert!(unknown.unresolved);
    assert!(inspect(&p, base, "python3 -W").is_err());
    assert!(inspect(&p, base, "python3 missing.py fast").is_err());
}
#[test]
fn npm_task_aliases_prefix_and_task_arguments_keep_projects_separate() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path();
    let p = policy(base);
    for command in [
        "npm test".into(),
        "npm run test".into(),
        "npm --prefix . test".into(),
        "npm run --prefix=. test".into(),
        format!("npm --prefix={} run test", base.display()),
    ] {
        assert_eq!(
            inspect(&p, base, &command).unwrap().entry.as_deref(),
            Some("npm tests"),
            "{command}"
        );
    }
    assert_eq!(
        inspect(&p, base, "npm run check -- --full")
            .unwrap()
            .entry
            .as_deref(),
        Some("npm check")
    );
    assert!(
        inspect(&p, base, "npm --prefix sub run test")
            .unwrap()
            .entry
            .is_none()
    );
    let other = base.join("sub");
    fs::create_dir(&other).unwrap();
    assert!(inspect(&p, &other, "npm test").unwrap().entry.is_none());
    assert_eq!(
        inspect(&p, &other, &format!("npm --prefix={} test", base.display()))
            .unwrap()
            .entry
            .as_deref(),
        Some("npm tests")
    );
    for command in ["npm --prefix", "npm --prefix= test"] {
        assert!(inspect(&p, base, command).is_err());
    }
    assert!(inspect(&p, base, "npm --unknown test").unwrap().unresolved);
}
#[test]
fn explicit_fast_source_trust_is_exact_and_other_commands_still_scan() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path();
    fs::create_dir(base.join("tools")).unwrap();
    fs::write(
        base.join("tools/check.py"),
        "import os\nif mode == 'full':\n    os.system('cargo test')\n",
    )
    .unwrap();
    let mut p = policy(base);
    p.entries[1].argv.pop();
    let fast = vec!["python3".into(), "tools/check.py".into(), "fast".into()];
    p.allow.push(fast.clone());
    // An ordinary outer allow still scans all static branches.
    assert_eq!(
        inspect(&p, base, "python3 tools/check.py fast")
            .unwrap()
            .entry
            .as_deref(),
        Some("Cargo full")
    );
    p.skip_source.push(fast);
    assert!(
        inspect(&p, base, "python3 -B ./tools/check.py fast")
            .unwrap()
            .entry
            .is_none()
    );
    for command in [
        "python3 tools/check.py fast; cargo test",
        "python3 tools/check.py fast && python3 -c 'import os; os.system(\"cargo test\")'",
    ] {
        assert!(inspect(&p, base, command).unwrap().entry.is_some());
    }
    for command in [
        "python3 tools/check.py full",
        "python3 tools/check.py fast extra",
    ] {
        assert_eq!(
            inspect(&p, base, command).unwrap().entry.as_deref(),
            Some("Python full")
        );
    }
    p.skip_source
        .push(vec!["bash".into(), "-c".into(), "cargo test".into()]);
    assert!(p.validate().is_err());
}
#[test]
fn noncargo_exceptions_are_declared_exact_arguments_after_project_normalization() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path();
    let mut p = policy(base);
    p.allow = vec![vec![
        "npm".into(),
        "run".into(),
        "check".into(),
        "--".into(),
        "one-file.ts".into(),
    ]];
    p.validate().unwrap();
    assert!(
        inspect(&p, base, "npm --prefix=. run check -- one-file.ts")
            .unwrap()
            .entry
            .is_none()
    );
    for command in [
        "npm run check",
        "npm run check -- other-file.ts",
        "npm run check -- one-file.ts extra",
        "npm run check -- one-file.ts --all",
    ] {
        assert!(inspect(&p, base, command).unwrap().entry.is_some());
    }
    p.allow = vec![vec!["npm".into(), "run".into(), "check".into()]];
    assert!(p.validate().is_err());
    p.allow = vec![vec!["unconfigured".into(), "one".into()]];
    assert!(p.validate().is_err());
    p.feedback = serde_json::from_value(json!({"reason":" "})).unwrap();
    assert!(p.validate().is_err());
}

#[test]
fn python_option_separator_preserves_a_script_named_like_an_interpreter_option() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("-c"), "print(1)").unwrap();
    let mut p = policy(root.path());
    p.entries.push(
        serde_json::from_value(json!({"argv":["python3","--","-c","full"],"label":"dash script"}))
            .unwrap(),
    );
    assert_eq!(
        inspect(&p, root.path(), "python3 -- -c full")
            .unwrap()
            .entry
            .as_deref(),
        Some("dash script")
    );
    assert!(
        inspect(&p, root.path(), "python3 -- -c fast")
            .unwrap()
            .entry
            .is_none()
    );
    assert!(inspect(&p, root.path(), "npm --prefix '' test").is_err());
}

#[test]
fn npm_does_not_change_the_parent_cwd_for_later_relative_python_calls() {
    let root = tempfile::tempdir().unwrap();
    let mut p = policy(root.path());
    fs::write(root.path().join("ordinary.py"), "print('ordinary data')").unwrap();
    fs::create_dir(root.path().join("sub")).unwrap();
    fs::write(
        root.path().join("checks.py"),
        "import os; os.system('cargo test')",
    )
    .unwrap();
    p.entries
        .retain(|entry| entry.argv.first().is_none_or(|program| program != "npm"));
    for prefix in [
        "npm --version",
        "npm run build",
        "npm --prefix sub run build",
    ] {
        let command = format!("{prefix}; python3 ordinary.py");
        let result = inspect(&p, root.path(), &command).unwrap();
        assert!(result.entry.is_none(), "{command}");
        assert!(result.unresolved);
        let command = format!("{prefix} && python3 checks.py");
        assert_eq!(
            inspect(&p, root.path(), &command).unwrap().entry.as_deref(),
            Some("Cargo full")
        );
    }
    assert!(inspect(&p, root.path(), "cd sub; python3 ordinary.py").is_err());
}
