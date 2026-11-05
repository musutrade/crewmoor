//! Rust-driven shell/path regression; synthetic Herdr responses are not acceptance.
use crewmoor::{Result, native::Runner, opencode::prepare, process, record::Record};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::Path,
    process::{Command, Stdio},
};

struct Startup {
    shell: u32,
    calls: RefCell<Vec<Vec<String>>>,
}
impl Runner for Startup {
    fn boot(&self, _: &Record) -> Result<process::Identity> {
        Err("existing host must not boot".into())
    }
    fn call(&self, record: &Record, args: &[String]) -> Result<Value> {
        self.calls.borrow_mut().push(args.to_vec());
        match (args[0].as_str(), args[1].as_str()) {
            ("status", "server") => Ok(json!({"version":"0.9.5","socket":record.socket()})),
            ("workspace", "create") => Ok(
                json!({"workspace":{"workspace_id":"w"},"root_pane":{"pane_id":"w:p","terminal_id":"t"}}),
            ),
            ("pane", "process-info") => {
                Ok(json!({"process_info":{"pane_id":"w:p","shell_pid":self.shell}}))
            }
            ("pane", "run") => {
                let output = Command::new("/bin/bash")
                    .args([
                        "--noprofile",
                        "--norc",
                        "-c",
                        &format!("PATH=/usr/bin:$PATH\n{}\ncommand -v opencode", args[3]),
                    ])
                    .env("PATH", record.root.join("opencode-bin"))
                    .output()?;
                if !output.status.success() {
                    return Err("startup export failed".into());
                }
                let selected = String::from_utf8(output.stdout)?;
                assert_eq!(selected.trim(), prepare::alias(record).to_str().unwrap());
                Ok(json!({"type":"pane_input_sent"}))
            }
            ("agent", "start") => Ok(
                json!({"agent":{"pane_id":"w:p","terminal_id":"t","workspace_id":"w","name":"probe","agent_session":{"kind":"id","value":"reported-subtree-B","source":"herdr:opencode"},"agent_status":"working"}}),
            ),
            _ => Err("unexpected native operation".into()),
        }
    }
}
fn record(root: &Path) -> Record {
    let config = serde_json::from_value(json!({"team":"entry-test","role":"probe","runtime":"tui","cwd":root,"store":root.join("bindings"),"runtime_binary":"/unused/herdr","runtime_version":"herdr 0.9.5","agent":"opencode","agent_executable":env!("CARGO_BIN_EXE_opencode-fixture"),"agent_version":"opencode v2.2.5","host":{"socket":"/unused/host.sock","server_version":"0.9.5"}})).unwrap();
    Record::prepare(config, &root.join("space ' quote")).unwrap()
}
#[test]
fn owned_posix_shell_export_precedes_exactly_one_official_start() {
    let root = tempfile::tempdir().unwrap();
    let mut record = record(root.path());
    record.config.host.as_mut().unwrap().socket = root.path().join("host.sock");
    let _listener = std::os::unix::net::UnixListener::bind(record.socket()).unwrap();
    record.capture_socket().unwrap();
    std::fs::create_dir(record.root.join("opencode-bin")).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_crewmoor"), prepare::alias(&record)).unwrap();
    let mut shell = Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-c", "read -r value"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let runner = Startup {
        shell: shell.id(),
        calls: RefCell::default(),
    };
    let result = crewmoor::tui::create(&mut record, &runner);
    drop(shell.stdin.take());
    shell.wait().unwrap();
    result.unwrap();
    let report = record.opencode_native.as_ref().unwrap();
    assert_eq!(report["reported_session"]["value"], "reported-subtree-B");
    assert_eq!(report["reported_session"]["source"], "herdr:opencode");
    assert_eq!(report["bound_session_state"], "unknown");
    assert_eq!(record.agent_status, "unknown");
    let calls = runner.calls.borrow();
    let configure = calls
        .iter()
        .position(|args| args[0..2] == ["pane", "run"])
        .unwrap();
    let starts: Vec<_> = calls
        .iter()
        .enumerate()
        .filter(|(_, args)| args[0..2] == ["agent", "start"])
        .collect();
    assert_eq!(starts.len(), 1);
    assert!(configure < starts[0].0);
    assert_eq!(&starts[0].1[3..5], ["--kind", "opencode"]);
    assert!(prepare::tui_path(&record, Path::new("/usr/bin/fish")).is_err());
    record.root.push("control\n");
    assert!(prepare::tui_path(&record, Path::new("/bin/bash")).is_err());
}
