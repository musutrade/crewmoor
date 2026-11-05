//! Rust subprocess regression for the caller DB source; native protocols are fixtures.
use crewmoor::{
    config::{AgentKind, Config},
    feedback::{self, Evidence, Registration},
    opencode::prepare,
    record::Record,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

fn snapshot(root: &Path, db: Option<&str>) -> Record {
    let context = feedback::register(
        &root.join("registry"),
        Registration {
            team: "db-source".into(),
            role: "probe".into(),
            task: "synthetic-db-env".into(),
            cwd: root.into(),
            agent: AgentKind::OpenCode,
            agent_version: "opencode v2.2.5".into(),
            evidence: Evidence::Synthetic,
            native_session_id: None,
        },
    )
    .unwrap();
    let config: Config = serde_json::from_value(json!({"team":"db-source","role":"probe","runtime":"core","cwd":root,"store":root.join("bindings"),"runtime_binary":env!("CARGO_BIN_EXE_transport-fixture"),"runtime_version":"fixture 1","agent":"opencode","agent_executable":env!("CARGO_BIN_EXE_opencode-fixture"),"agent_version":"opencode v2.2.5","feedback_registry":root.join("registry"),"feedback_instance":context.instance})).unwrap();
    let file = root.join("config.toml");
    fs::write(&file, toml::to_string(&config).unwrap()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_crewmoor"));
    command.arg("launch").arg(file).arg(root.join("launch"));
    match db {
        Some(value) => {
            command.env("OPENCODE_DB", value);
        }
        None => {
            command.env_remove("OPENCODE_DB");
        }
    }
    let output = command.output().unwrap();
    // The transport fixture refuses server start: only preparation ran, no real core.
    assert!(!output.status.success());
    let record = Record::load(&root.join("launch")).unwrap();
    let launch: Value =
        serde_json::from_slice(&fs::read(prepare::launch_file(&record)).unwrap()).unwrap();
    assert_eq!(launch["environment"]["OPENCODE_DB"], json!(db));
    record
}
fn run(record: &Record, trace: &Path) {
    let output = Command::new(prepare::alias(record))
        .args(prepare::tui_arguments(&record.config))
        .env(prepare::LAUNCH_ENV, prepare::launch_file(record))
        .env("OPENCODE_DB", record.root.join("fallback-only.sqlite"))
        .env("CM_OPENCODE_TEST_TRACE", trace)
        .current_dir(&record.config.cwd)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn native_server_and_tui_restore_unset_or_explicit_caller_db_after_fallback() {
    for db in [None, Some("/caller/configured-native.sqlite")] {
        let directory = tempfile::Builder::new()
            .prefix("cdb")
            .tempdir_in("/tmp")
            .unwrap();
        let record = snapshot(directory.path(), db);
        let trace = directory.path().join("trace.jsonl");
        run(&record, &trace);
        let rows: Vec<Value> = fs::read_to_string(trace)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(rows.iter().any(|row| row["path"] == "/api/session"));
        assert!(rows.iter().any(|row| row["method"] == "TUI"));
        for row in rows {
            assert_eq!(row["native_db"], json!(db));
        }
    }
}
