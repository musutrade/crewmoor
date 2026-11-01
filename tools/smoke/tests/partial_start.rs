use crewmoor_smoke::{flow, run::Run};
use serde_json::json;
use std::{fs, path::PathBuf};

const FIXTURE: &str = env!("CARGO_BIN_EXE_partial-start-fixture");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("crewmoor-smoke-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn run(dir: &std::path::Path, team_line: &str) -> Run {
    fs::write(
        dir.join("team.toml"),
        format!("{team_line}\nruntime = \"tui\"\n"),
    )
    .unwrap();
    let plan = json!({"evidence":dir.join("evidence"),"crewmoor":FIXTURE,
        "team_toml":dir.join("team.toml"),"state":dir,"host_socket":dir.join("synthetic.sock")});
    Run::create(plan, "synthetic").unwrap()
}
#[test]
fn partial_start_keeps_the_owned_team_path_for_plain_and_escaped_names() {
    for (name, line) in [
        ("plain", r#"team = "partial""#),
        ("escaped", r#"team = "par\u0074ial""#),
        ("literal", "team = 'partial' # comment"),
    ] {
        let dir = scratch(name);
        let run = run(&dir, line);
        assert_eq!(flow::expected_dir(&run).unwrap(), dir.join("teams/partial"));
        assert!(flow::start(&run).is_err(), "{name}");
        assert!(dir.join("teams/partial").is_dir(), "{name}");
        assert_eq!(
            run.team_dir.borrow().clone(),
            Some(dir.join("teams/partial")),
            "{name}"
        );
        assert!(run.evidence.join("01-team-start.json").exists(), "{name}");
        fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn an_existing_team_directory_is_never_claimed() {
    let dir = scratch("existing");
    fs::create_dir_all(dir.join("teams/partial")).unwrap();
    let run = run(&dir, r#"team = "partial""#);
    assert!(flow::start(&run).is_err());
    assert_eq!(run.team_dir.borrow().clone(), None);
    assert!(!run.evidence.join("01-team-start.json").exists());
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn configs_without_one_decodable_team_name_are_refused_before_start() {
    for (name, line) in [
        ("missing", "task = \"x\""),
        ("malformed", "team = \"unterminated"),
        ("nested", r#"team = "../other""#),
        ("absolute", r#"team = "/tmp/other""#),
        ("empty", r#"team = """#),
    ] {
        let dir = scratch(name);
        let run = run(&dir, line);
        assert!(flow::start(&run).is_err(), "{name}");
        assert_eq!(run.team_dir.borrow().clone(), None, "{name}");
        assert!(!run.evidence.join("01-team-start.json").exists(), "{name}");
        assert!(!dir.join("teams").exists(), "{name}");
        fs::remove_dir_all(dir).unwrap();
    }
}
