use crewmoor::{
    Result,
    config::AgentKind,
    feedback::{self, Evidence, Registration},
    issue::{self, ASSIGNMENT_FILE, Assignment, Gh},
};
use serde_json::{Value, json};
use std::{cell::RefCell, path::Path, process::Command};

/// Answers canned `gh` output and records every argument list it was given.
struct Fake {
    answer: Result<Value>,
    calls: RefCell<Vec<Vec<String>>>,
}
impl Fake {
    fn new(answer: Value) -> Self {
        Self {
            answer: Ok(answer),
            calls: RefCell::default(),
        }
    }
    fn failing() -> Self {
        Self {
            answer: Err("gh unavailable".into()),
            calls: RefCell::default(),
        }
    }
    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.borrow().clone()
    }
}
impl Gh for Fake {
    fn call(&self, args: &[String]) -> Result<Value> {
        self.calls.borrow_mut().push(args.to_vec());
        match &self.answer {
            Ok(value) => Ok(value.clone()),
            Err(error) => Err(error.to_string().into()),
        }
    }
}
fn instance(dir: &Path) -> String {
    let registration = Registration {
        team: "issue-test".into(),
        role: "dispatcher".into(),
        task: "issue-8".into(),
        cwd: dir.to_path_buf(),
        agent: AgentKind::Claude,
        agent_version: "2.1.316 (Claude Code)".into(),
        evidence: Evidence::Synthetic,
        native_session_id: None,
    };
    feedback::register(dir, registration).unwrap().instance
}
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}
fn record(dir: &Path, instance: &str) -> std::path::PathBuf {
    dir.join("instances").join(instance).join(ASSIGNMENT_FILE)
}

#[test]
fn assign_writes_the_record_and_current_reads_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let id = instance(dir.path());
    let reg = dir.path().to_str().unwrap();
    let gh = Fake::new(Value::Null);
    let written = issue::run(&args(&["assign", reg, "musutrade/crewmoor", "8", &id]), &gh).unwrap();
    assert_eq!(written, json!({"repo":"musutrade/crewmoor","number":8}));
    let stored: Value =
        serde_json::from_slice(&std::fs::read(record(dir.path(), &id)).unwrap()).unwrap();
    assert_eq!(stored, json!({"repo":"musutrade/crewmoor","number":8}));
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(record(dir.path(), &id))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let current = issue::run(&args(&["current", reg, &id]), &gh).unwrap();
    assert_eq!(current, written);
    assert!(gh.calls().is_empty(), "assign and current never call gh");
}
#[test]
fn assigning_again_overwrites_without_leaving_staged_files() {
    let dir = tempfile::tempdir().unwrap();
    let id = instance(dir.path());
    let first = Assignment {
        repo: "musutrade/crewmoor".into(),
        number: 8,
    };
    let second = Assignment {
        repo: "other-org/repo.rs".into(),
        number: 15,
    };
    issue::assign(dir.path(), &id, &first).unwrap();
    issue::assign(dir.path(), &id, &first).unwrap();
    assert_eq!(issue::current(dir.path(), &id).unwrap(), first);
    issue::assign(dir.path(), &id, &second).unwrap();
    assert_eq!(issue::current(dir.path(), &id).unwrap(), second);
    let names: Vec<String> = std::fs::read_dir(dir.path().join("instances").join(&id))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(sorted, ["context.json", ASSIGNMENT_FILE]);
}
#[test]
fn current_and_done_fail_without_an_assignment() {
    let dir = tempfile::tempdir().unwrap();
    let id = instance(dir.path());
    let gh = Fake::new(json!({"state":"CLOSED"}));
    let error = issue::current(dir.path(), &id).unwrap_err();
    assert_eq!(error.to_string(), "no issue assigned to this instance");
    let error = issue::done(dir.path(), &id, &gh).unwrap_err();
    assert_eq!(error.to_string(), "no issue assigned to this instance");
    assert!(gh.calls().is_empty());
}
#[test]
fn assign_rejects_bad_input_and_unregistered_instances() {
    let dir = tempfile::tempdir().unwrap();
    let id = instance(dir.path());
    let reg = dir.path().to_str().unwrap();
    let gh = Fake::new(Value::Null);
    for repo in [
        "crewmoor", "a/b/c", "-R/x", "owner/", "/name", "own er/x", "o/--json",
    ] {
        let result = issue::run(&args(&["assign", reg, repo, "8", &id]), &gh);
        assert!(result.is_err(), "{repo} accepted");
    }
    for number in ["0", "-1", "eight", ""] {
        let result = issue::run(&args(&["assign", reg, "a/b", number, &id]), &gh);
        assert!(result.is_err(), "{number} accepted");
    }
    let unregistered = uuid::Uuid::new_v4().simple().to_string();
    assert!(issue::run(&args(&["assign", reg, "a/b", "1", &unregistered]), &gh).is_err());
    assert!(issue::run(&args(&["assign", reg, "a/b", "1", "../escape"]), &gh).is_err());
    assert!(!record(dir.path(), &id).exists());
    assert!(!dir.path().join("instances").join(&unregistered).exists());
}
#[test]
fn current_rejects_a_damaged_record() {
    let dir = tempfile::tempdir().unwrap();
    let id = instance(dir.path());
    for text in [
        "not json",
        r#"{"repo":"a/b","number":0}"#,
        r#"{"repo":"bad","number":1}"#,
        r#"{"repo":"a/b","number":1,"extra":true}"#,
    ] {
        std::fs::write(record(dir.path(), &id), text).unwrap();
        assert!(issue::current(dir.path(), &id).is_err(), "{text} accepted");
    }
}
#[test]
fn done_reports_closed_or_open_from_gh() {
    let dir = tempfile::tempdir().unwrap();
    let id = instance(dir.path());
    let reg = dir.path().to_str().unwrap();
    let assignment = Assignment {
        repo: "musutrade/crewmoor".into(),
        number: 8,
    };
    issue::assign(dir.path(), &id, &assignment).unwrap();
    for (state, closed) in [("CLOSED", true), ("OPEN", false)] {
        let gh = Fake::new(json!({"state": state}));
        let answer = issue::run(&args(&["done", reg, &id]), &gh).unwrap();
        assert_eq!(answer, json!({"closed": closed}));
        assert_eq!(
            gh.calls(),
            [args(&[
                "issue",
                "view",
                "8",
                "--repo=musutrade/crewmoor",
                "--json=state"
            ])]
        );
    }
    for answer in [json!({"state":"MERGED"}), json!({}), json!([])] {
        assert!(issue::done(dir.path(), &id, &Fake::new(answer)).is_err());
    }
    assert!(issue::done(dir.path(), &id, &Fake::failing()).is_err());
}
fn listing() -> Value {
    json!([{"number":3,"title":"Oldest ready","body":"Do it","url":"https://github.com/o/r/issues/3"}])
}
#[test]
fn next_lists_the_oldest_open_issue_with_the_default_label() {
    let gh = Fake::new(listing());
    let answer = issue::run(&args(&["next", "o/r"]), &gh).unwrap();
    assert_eq!(
        answer,
        json!({"number":3,"title":"Oldest ready","body":"Do it","url":"https://github.com/o/r/issues/3"})
    );
    let expected = |label: &str| {
        args(&[
            "issue",
            "list",
            "--repo=o/r",
            "--state=open",
            label,
            "--search=sort:created-asc",
            "--limit=1",
            "--json=number,title,body,url",
        ])
    };
    assert_eq!(gh.calls(), [expected("--label=ready")]);
    let gh = Fake::new(listing());
    issue::run(&args(&["next", "o/r", "--label", "good first issue"]), &gh).unwrap();
    assert_eq!(gh.calls(), [expected("--label=good first issue")]);
}
#[test]
fn next_fails_when_nothing_matches_or_input_is_bad() {
    let error = issue::next("o/r", "ready", &Fake::new(json!([]))).unwrap_err();
    assert_eq!(error.to_string(), "no open issue labelled ready");
    let gh = Fake::new(listing());
    for bad in [
        args(&["next", "nope"]),
        args(&["next", "o/r", "--label", ""]),
        args(&["next", "o/r", "--label", "a,b"]),
        args(&["next", "o/r", "--label", "a\nb"]),
        args(&["next", "o/r", "--tag", "ready"]),
        args(&["next", "o/r", "--label"]),
        args(&["next"]),
    ] {
        assert!(issue::run(&bad, &gh).is_err(), "{bad:?} accepted");
    }
    assert!(gh.calls().is_empty(), "invalid input never reaches gh");
    assert!(issue::next("o/r", "ready", &Fake::new(json!({"number":1}))).is_err());
    assert!(issue::next("o/r", "ready", &Fake::failing()).is_err());
}
#[test]
fn run_rejects_unknown_operations_and_wrong_arity() {
    let gh = Fake::new(Value::Null);
    for bad in [
        args(&[]),
        args(&["close", "a"]),
        args(&["assign", "reg", "a/b", "1"]),
        args(&["current", "reg"]),
        args(&["done", "reg", "x", "y"]),
    ] {
        let error = issue::run(&bad, &gh).unwrap_err();
        assert!(
            error.to_string().starts_with("usage: crewmoor issue"),
            "{bad:?}"
        );
    }
}
/// A stand-in `gh` on PATH: logs its arguments, prints `answer`, exits with `status`.
fn fake_gh(dir: &Path, answer: &str, status: i32) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(dir.join("answer"), answer).unwrap();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{log}'\ncat '{answer}'\necho 'fake gh stderr' >&2\nexit {status}\n",
        log = dir.join("gh-args").display(),
        answer = dir.join("answer").display(),
    );
    let path = bin.join("gh");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}
fn crewmoor(bin: &Path, values: &[&str]) -> std::process::Output {
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .arg("issue")
        .args(values)
        .env("PATH", path)
        .output()
        .unwrap()
}
#[test]
fn binary_runs_every_operation_through_the_installed_gh() {
    let dir = tempfile::tempdir().unwrap();
    let id = instance(dir.path());
    let reg = dir.path().to_str().unwrap();
    let bin = fake_gh(dir.path(), &listing().to_string(), 0);
    let out = crewmoor(&bin, &["next", "o/r", "--label", "ready"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let printed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(printed["number"], 3);
    let logged = std::fs::read_to_string(dir.path().join("gh-args")).unwrap();
    assert!(logged.starts_with("issue\nlist\n--repo=o/r\n"), "{logged}");
    let out = crewmoor(&bin, &["assign", reg, "o/r", "3", &id]);
    assert!(out.status.success());
    let out = crewmoor(&bin, &["current", reg, &id]);
    let printed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(printed, json!({"repo":"o/r","number":3}));
    let bin = fake_gh(dir.path(), r#"{"state":"CLOSED"}"#, 0);
    let out = crewmoor(&bin, &["done", reg, &id]);
    let printed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(printed, json!({"closed": true}));
}
#[test]
fn binary_exits_nonzero_when_gh_fails_or_nothing_is_assigned() {
    let dir = tempfile::tempdir().unwrap();
    let id = instance(dir.path());
    let reg = dir.path().to_str().unwrap();
    let bin = fake_gh(dir.path(), "", 1);
    let out = crewmoor(&bin, &["next", "o/r"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("fake gh stderr"));
    assert!(out.stdout.is_empty());
    let bin = fake_gh(dir.path(), "not json", 0);
    assert!(!crewmoor(&bin, &["next", "o/r"]).status.success());
    let out = crewmoor(&bin, &["current", reg, &id]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no issue assigned"));
    assert!(!crewmoor(&bin, &[]).status.success());
}
