use crewmoor::app_target;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
};
use tempfile::TempDir;

fn fake_server(dir: &TempDir, responses: &[serde_json::Value]) -> std::thread::JoinHandle<()> {
    let sock = dir.path().join("hc.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let responses: Vec<_> = responses.to_vec();
    std::thread::spawn(move || {
        for resp in &responses {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .ok();
                let reply = serde_json::json!({"jsonrpc":"2.0","id":1,"result":resp});
                stream.write_all(format!("{reply}\n").as_bytes()).ok();
            }
        }
    })
}

fn status_result() -> serde_json::Value {
    serde_json::json!({
        "core": {"id": "abc123", "name": "testmachine"},
        "version": "0.1.16",
        "pid": 1234
    })
}

fn doc_result_with_spaces() -> serde_json::Value {
    serde_json::json!({
        "value": {
            "spaces": [{
                "id": "testmachine/sp-abc",
                "name": "myspace",
                "tabs": [
                    {"id": "testmachine/tb-def", "name": "mytab"},
                    {"id": "testmachine/tb-xyz", "name": ""}
                ]
            }]
        }
    })
}

fn doc_result_empty() -> serde_json::Value {
    serde_json::json!({"value": {"spaces": []}})
}

#[test]
fn generate_writes_all_fields_and_comments_spaces() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("hc.sock");
    let handle = fake_server(&dir, &[status_result(), doc_result_with_spaces()]);

    let out = dir.path().join("out.toml");
    app_target::cli(&[
        "generate".into(),
        "--socket".into(),
        sock.to_str().unwrap().into(),
        "--output".into(),
        out.to_str().unwrap().into(),
    ])
    .unwrap();
    handle.join().unwrap();

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("core_id   = \"abc123\""));
    assert!(content.contains("machine   = \"testmachine\""));
    // header comment references the version
    assert!(content.contains("0.1.16"));
    assert!(content.contains("socket    ="));
    // Spaces are commented out, not active TOML fields
    let active_lines: Vec<_> = content.lines().filter(|l| !l.starts_with('#')).collect();
    let active = active_lines.join("\n");
    assert!(!active.contains("space_id"));
    assert!(!active.contains("tab_id"));
    assert!(content.contains("sp-abc"));
    assert!(content.contains("tb-def"));
    assert!(content.contains("myspace"));
    assert!(content.contains("mytab"));
}

#[test]
fn generate_without_spaces_omits_space_comments() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("hc.sock");
    let handle = fake_server(&dir, &[status_result(), doc_result_empty()]);

    let out = dir.path().join("out.toml");
    app_target::cli(&[
        "generate".into(),
        "--socket".into(),
        sock.to_str().unwrap().into(),
        "--output".into(),
        out.to_str().unwrap().into(),
    ])
    .unwrap();
    handle.join().unwrap();

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("core_id   = \"abc123\""));
    let active_lines: Vec<_> = content.lines().filter(|l| !l.starts_with('#')).collect();
    let active = active_lines.join("\n");
    assert!(!active.contains("space_id"));
    assert!(!active.contains("tab_id"));
    // No space comment block either
    assert!(!content.contains("# space:"));
}

#[test]
fn generate_refuses_to_overwrite_existing_output() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("hc.sock");
    let handle = fake_server(&dir, &[status_result(), doc_result_empty()]);

    let out = dir.path().join("existing.toml");
    std::fs::write(&out, "old content").unwrap();

    let result = app_target::cli(&[
        "generate".into(),
        "--socket".into(),
        sock.to_str().unwrap().into(),
        "--output".into(),
        out.to_str().unwrap().into(),
    ]);
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "old content");
    handle.join().unwrap();
}

#[test]
fn generate_prints_to_stdout_when_no_output_flag() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("hc.sock");
    let handle = fake_server(&dir, &[status_result(), doc_result_empty()]);

    // No --output: cli writes to stdout and returns Ok
    app_target::cli(&[
        "generate".into(),
        "--socket".into(),
        sock.to_str().unwrap().into(),
    ])
    .unwrap();
    handle.join().unwrap();
}

#[test]
fn unknown_subcommand_and_missing_socket_value_are_rejected() {
    assert!(app_target::cli(&["unknown".into()]).is_err());
    assert!(app_target::cli(&["generate".into(), "--socket".into()]).is_err());
    assert!(app_target::cli(&["generate".into(), "--output".into()]).is_err());
    assert!(app_target::cli(&["generate".into(), "--bogus".into()]).is_err());
}
