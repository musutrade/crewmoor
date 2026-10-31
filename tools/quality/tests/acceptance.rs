use crewmoor_quality::{config, coverage, driver, source};
use std::{fs, process::Command};

#[test]
fn crap_boundaries() {
    assert_eq!(coverage::enforce(10, 1.0).unwrap(), 10.0);
    assert!(coverage::enforce(10, 0.999).is_err());
    assert!(coverage::enforce(11, 1.0).is_err());
    assert_eq!(coverage::score(2, 0.0).unwrap(), 6.0);
    for cov in [-1.0, 1.1, f64::NAN, f64::INFINITY] {
        assert!(coverage::score(2, cov).is_err());
    }
    assert!(coverage::score(0, 1.0).is_err());
}
#[test]
fn unwrap_context() {
    let code = "fn f(x: Option<u8>) -> u8 { x.unwrap() }";
    assert!(source::analyze(code, false).is_err());
    assert!(source::analyze(code, true).is_ok());
    assert!(source::analyze(&format!("{code} #[cfg(test)] mod tests {{}}"), false).is_err());
    assert!(source::analyze(&format!("#[cfg(test)] mod tests {{ {code} }}"), false).is_ok());
    assert!(source::analyze("fn f(x: Option<u8>) -> u8 { Option::unwrap(x) }", false).is_err());
    assert!(source::analyze("fn f(x: Option<u8>) -> u8 { x.expect(\"value\") }", false).is_ok());
}
#[test]
fn suppression_in_tests_and_macros() {
    for attr in [
        "#[allow(clippy::all)]",
        "#[expect(clippy::all)]",
        "#[cfg_attr(test, allow(warnings))]",
    ] {
        assert!(source::analyze(&format!("{attr} fn f() {{}}"), true).is_err());
    }
    assert!(
        source::analyze(
            "macro_rules! m { () => { #[allow(clippy::all)] fn f() {} }; }",
            false
        )
        .is_err()
    );
    assert!(source::analyze("fn f() { println!(\"{}\", x.unwrap()); }", false).is_err());
    assert!(source::analyze("#[test] fn f() { let n = x.unwrap(); }", false).is_ok());
}
#[test]
fn complexity_models() {
    let rows = source::analyze(
        "fn f(x: bool) { if x && x {} while x {} let c = || { if x {} }; }",
        false,
    )
    .unwrap();
    assert_eq!(rows[0].cc, 5);
    let rows = source::analyze("fn f() { fn g() { if true {} } }", false).unwrap();
    assert_eq!((rows[0].cc, rows[1].cc), (1, 2));
    assert!(source::analyze("fn f() { custom!(); }", false).unwrap()[0].unsupported);
    assert!(
        source::analyze("fn f() { assert!(if x {true} else {false}); }", false).unwrap()[0]
            .unsupported
    );
}
#[test]
fn configuration_negatives() {
    for text in [
        "[lints.clippy]\nall='allow'",
        "[build]\nrustflags=['-A', 'warnings']",
        "[build]\nrustflags=['--cap-lints=allow']",
        "[build]\nrustc-wrapper='unknown'",
    ] {
        assert!(config::document(text).is_err());
    }
    assert!(
        config::document(
            "[build]\nrustc-wrapper='sccache'\nrustflags=['-C', 'link-arg=-fuse-ld=mold']"
        )
        .is_ok()
    );
    assert!(config::environment([("RUSTFLAGS".into(), "-A warnings".into())].into_iter()).is_err());
    for text in [
        "cargo clippy -- -A warnings",
        "continue-on-error: true",
        "cargo clippy || true",
        "exit 0",
    ] {
        assert!(config::workflow(text).is_err());
    }
}
#[test]
fn coverage_validation() {
    for text in ["{}", r#"{"type":"llvm.coverage.json.export","data":[]}"#] {
        assert!(coverage::parse(text).is_err());
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lib.rs");
    fs::write(&path, "fn f() {}\n").unwrap();
    let json = serde_json::json!({"type":"llvm.coverage.json.export", "data":[{"files":[{"filename":path,"segments":[[1,1,1,true,true,false],[1,5,0,true,false,false]]}]}]});
    let report = coverage::parse(&json.to_string()).unwrap();
    let functions = source::analyze("fn f() {}", false).unwrap();
    assert_eq!(coverage::check_file(&path, &functions, &report).unwrap(), 1);
    assert!(coverage::fraction(&Default::default(), &functions[0]).is_err());
}
#[test]
fn orchestration_propagates_failure() {
    let plan = driver::plan("Cargo.toml".as_ref(), "coverage.json".as_ref());
    assert!(plan[1].ends_with(&["--".into(), "-D".into(), "warnings".into()]));
    assert!(driver::execute(&plan, |_| Ok(String::new())).is_ok());
    assert!(driver::execute(&plan, |_| Err("failure".into())).is_err());
    assert!(driver::command(&["--version".into()]).is_ok());
    assert!(driver::command(&["crewmoor-nonexistent-command".into()]).is_err());
}
#[test]
fn cli_and_missing_project() {
    let binary = env!("CARGO_BIN_EXE_crewmoor-quality");
    assert!(
        Command::new(binary)
            .arg("--help")
            .status()
            .unwrap()
            .success()
    );
    assert!(
        !Command::new(binary)
            .arg("invalid")
            .status()
            .unwrap()
            .success()
    );
    let temp = tempfile::tempdir().unwrap();
    assert!(driver::check(&temp.path().join("Cargo.toml")).is_err());
    assert!(driver::business(temp.path()).is_ok());
}
#[test]
fn fixture_policy_and_inventory() {
    let manifest = driver::root().join("fixtures/quality/Cargo.toml");
    assert!(!driver::policy(&manifest).unwrap().is_empty());
    assert!(driver::cli(vec!["lint-policy".into(), manifest.display().to_string()]).is_ok());
    assert!(
        driver::cli(vec![
            "crap".into(),
            manifest.display().to_string(),
            "/missing/report".into()
        ])
        .is_err()
    );
    assert!(driver::cli(vec!["check".into(), "/missing/manifest".into()]).is_err());
}

#[test]
fn raw_identifiers_cannot_bypass_policy() {
    for text in [
        "fn f(x: Option<u8>) -> u8 { x.r#unwrap() }",
        "fn f(x: Option<u8>) -> u8 { Option::r#unwrap(x) }",
        "#![r#allow(clippy::all)] fn f() {}",
        "#[r#expect(clippy::all)] fn f() {}",
        "#[cfg_attr(test, r#allow(clippy::all))] fn f() {}",
        "fn f() { println!(\"{}\", x.r#unwrap()); }",
    ] {
        assert!(source::analyze(text, false).is_err());
    }
    assert!(source::analyze("#[test] fn f() { x.r#unwrap(); }", false).is_ok());
}
#[test]
fn external_test_module_requires_explicit_context() {
    let helper = "pub fn helper() -> u8 { \"1\".parse::<u8>().unwrap() }";
    assert!(source::analyze(helper, false).is_err());
    assert!(source::analyze(&format!("#![cfg(test)]\n{helper}"), false).is_ok());
}

#[test]
fn crap_command_consumes_report_and_rejects_failure() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let manifest = dir.path().join("Cargo.toml");
    fs::write(
        &manifest,
        "[package]\nname='metric-case'\nversion='0.0.0'\nedition='2024'\n",
    )
    .unwrap();
    let path = dir.path().join("src/lib.rs");
    fs::write(&path, "pub fn f(x: bool) { if x {} if x {} }\n").unwrap();
    driver::command(&[
        "generate-lockfile".into(),
        "--manifest-path".into(),
        manifest.display().to_string(),
    ])
    .unwrap();
    let report = dir.path().join("coverage.json");
    let json = serde_json::json!({"type":"llvm.coverage.json.export","data":[{"files":[{"filename":path,"segments":[[1,1,1,true,true,false]]}]}]});
    fs::write(&report, json.to_string()).unwrap();
    assert!(driver::crap(&manifest, &report).is_ok());
    let json = serde_json::json!({"type":"llvm.coverage.json.export","data":[{"files":[{"filename":path,"segments":[[1,1,0,true,true,false]]}]}]});
    fs::write(&report, json.to_string()).unwrap();
    assert!(driver::crap(&manifest, &report).is_err());
}

#[test]
fn verify_checks_tool_before_fixture_and_propagates_errors() {
    let paths = std::cell::RefCell::new(Vec::new());
    driver::verify_with(|path| {
        paths.borrow_mut().push(path.to_path_buf());
        Ok(())
    })
    .unwrap();
    let count = if driver::root().join("Cargo.toml").is_file() {
        3
    } else {
        2
    };
    assert_eq!(paths.borrow().len(), count);
    assert!(paths.borrow()[0].ends_with("tools/quality/Cargo.toml"));
    assert!(paths.borrow()[1].ends_with("fixtures/quality/Cargo.toml"));
    assert!(driver::verify_with(|_| Err("check failed".into())).is_err());
}

#[test]
fn custom_cargo_home_rejects_lint_override() {
    let temp = tempfile::tempdir().unwrap();
    assert!(config::custom_home(temp.path()).is_ok());
    fs::write(
        temp.path().join("config.toml"),
        "[build]\nrustflags=['--cap-lints=allow']",
    )
    .unwrap();
    assert!(config::custom_home(temp.path()).is_err());
}

#[test]
fn invalid_coverage_segments_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lib.rs");
    fs::write(&path, "fn f() {}\n").unwrap();
    for segment in [
        serde_json::json!([0, 1, 1, true, true, false]),
        serde_json::json!([1, 1, -1, true, true, false]),
        serde_json::json!([1, 1, 1]),
    ] {
        let report = serde_json::json!({"type":"llvm.coverage.json.export","data":[{"files":[{"filename":path,"segments":[segment]}]}]});
        assert!(coverage::parse(&report.to_string()).is_err());
    }
}

#[test]
fn macro_inner_attributes_and_cargo_expect_are_rejected() {
    assert!(
        source::analyze(
            "macro_rules! m { () => { mod inner { #![r#allow(clippy::all)] } }; }",
            false
        )
        .is_err()
    );
    assert!(config::document("[lints.clippy]\nall='expect'").is_err());
}
