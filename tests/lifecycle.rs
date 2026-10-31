#[path = "support/cleanup_checks.rs"]
mod cleanup_checks;
#[path = "support/common.rs"]
mod common;
#[path = "support/environment_checks.rs"]
mod environment_checks;
use crewmoor::{
    config::{self, Runtime},
    core, engine,
    native::{self, Native, Runner, strings},
    process,
    record::Record,
    tui,
};
use serde_json::json;
use std::{fs, os::unix::net::UnixListener};

#[test]
fn isolated_core_launch_inspect_and_controlled_terminal_stop() {
    let (_dir, mut record, fake) = common::launch(Runtime::Core);
    assert_eq!(record.accepted, Some(true));
    assert_eq!(record.ready, "unknown");
    assert!(engine::verified_process(&record).is_ok());
    engine::inspect(&mut record, &fake).unwrap();
    engine::stop(&mut record, &fake).unwrap();
    assert!(*fake.killed.borrow());
    assert!(
        fake.calls
            .borrow()
            .iter()
            .any(|args| args == &strings(&["rm", "t7"]))
    );
    assert!(
        fake.calls
            .borrow()
            .iter()
            .all(|args| !args.iter().any(|arg| arg == "send"))
    );
}
#[test]
fn isolated_tui_launch_and_stop_preserve_native_identity() {
    let (_dir, mut record, fake) = common::launch(Runtime::Tui);
    assert_eq!(record.ready, "idle");
    assert_eq!(record.handle.as_deref(), Some("w1:p1"));
    engine::stop(&mut record, &fake).unwrap();
    assert!(
        fake.calls
            .borrow()
            .iter()
            .any(|args| args == &strings(&["workspace", "close", "w1"]))
    );
    assert!(
        fake.calls
            .borrow()
            .iter()
            .all(|args| !args.iter().any(|arg| arg == "prompt"))
    );
}
#[test]
fn stop_refuses_additional_unowned_resources() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    *fake.extra.borrow_mut() = true;
    assert!(core::stop(&mut r, &fake).is_err());
    assert!(!*fake.killed.borrow());
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    *fake.extra.borrow_mut() = true;
    assert!(tui::stop(&mut r, &fake).is_err());
}
#[test]
fn receipt_binding_generation_and_target_mismatch_fail_closed() {
    let (dir, record, fake) = common::launch(Runtime::Core);
    let mut value = fake.terminal(&record);
    value["labels"]["crewmoor-owner"] = json!("other");
    assert!(core::verify_target(&record, &value).is_err());
    value = fake.terminal(&record);
    value["created_ms"] = json!(43);
    assert!(core::verify_target(&record, &value).is_err());
    value = fake.terminal(&record);
    value["argv"] = json!(["bash"]);
    assert!(core::verify_target(&record, &value).is_err());
    assert!(Record::prepare(record.config.clone(), &dir.path().join("other-instance")).is_err());
    fs::write(record.config.store.join("test-team.json"), "other").unwrap();
    assert!(record.guard().is_err());
}
#[test]
fn unknowns_are_not_ready_and_tui_mismatch_is_refused() {
    assert_eq!(core::readiness(&json!({"state":"live"})), "unknown");
    assert_eq!(
        core::readiness(&json!({"reported":true,"status":{"app":"bash","state":"idle"}})),
        "unknown"
    );
    assert_eq!(
        core::readiness(
            &json!({"state":"live","reported":true,"status":{"app":"codex","state":"idle"}})
        ),
        "idle"
    );
    assert_eq!(tui::readiness(&json!({"agent_status":"idle"})), "unknown");
    let (_dir, r, fake) = common::launch(Runtime::Tui);
    let mut value = fake.agent(&r)["agent"].clone();
    value["terminal_id"] = json!("replaced");
    assert!(tui::verify_target(&r, &value).is_err());
}
#[test]
fn state_root_lock_process_and_socket_mismatches_are_refused() {
    let (dir, mut r, _fake) = common::launch(Runtime::Core);
    let lock = r.lock().unwrap();
    assert!(r.lock().is_err());
    drop(lock);
    let loaded = Record::load(&r.root).unwrap();
    assert_eq!(loaded.owner, r.owner);
    let wrong = dir.path().join("copy");
    fs::create_dir(&wrong).unwrap();
    fs::copy(r.root.join("record.json"), wrong.join("record.json")).unwrap();
    assert!(Record::load(&wrong).is_err());
    r.server.as_mut().unwrap().start_ticks = "wrong".into();
    assert!(r.guard().is_err());
    assert!(process::start_ticks("invalid").is_err());
    assert!(process::start_ticks("1 (weird) name) X").is_err());
    r.server = Some(process::identity(std::process::id()).unwrap());
    r.socket_inode = Some(0);
    assert!(r.guard().is_err());
}
#[test]
fn failure_is_persisted_without_relaunch_or_assumed_success() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    *fake.fail.borrow_mut() = true;
    let error = engine::inspect(&mut r, &fake);
    assert!(engine::preserve(&mut r, error).is_err());
    assert_eq!(Record::load(&r.root).unwrap().phase, "unknown-outcome");
    assert_eq!(r.ready, "unknown");
    assert!(engine::stop(&mut r, &fake).is_err());
}
#[test]
fn configuration_rejects_tasks_and_only_allows_interactive_launch_options() {
    let dir = common::directory();
    let mut c = common::config(dir.path(), Runtime::Core);
    c.validate().unwrap();
    for args in [
        vec!["do work"],
        vec!["exec"],
        vec!["--model"],
        vec!["--model", "-bad"],
    ] {
        c.agent_args = args.into_iter().map(Into::into).collect();
        assert!(c.validate().is_err());
    }
    c.agent_args = strings(&["--no-alt-screen", "--model", "model-name"]);
    c.validate().unwrap();
    assert_eq!(c.argv()[1], "--no-daemon");
    assert!(config::name("../another-team").is_err());
    assert!(config::name("").is_err());
    assert!(config::absolute("relative".as_ref()).is_err());
}
#[test]
fn native_transport_payload_environment_and_version_errors_are_checked() {
    let dir = common::directory();
    let c = common::config(dir.path(), Runtime::Core);
    let r = Record::prepare(c, &dir.path().join("instance")).unwrap();
    assert_eq!(
        Native
            .call(&r, &strings(&["reply", "{\"ok\":true}"]))
            .unwrap(),
        json!({"ok":true})
    );
    assert!(Native.call(&r, &strings(&["fail"])).is_err());
    assert!(Native.call(&r, &strings(&["reply", "not-json"])).is_err());
    assert!(native::payload(json!({"error":"bad"}), &Runtime::Core).is_err());
    assert_eq!(
        native::payload(json!({"result":{"ok":1}}), &Runtime::Tui).unwrap(),
        json!({"ok":1})
    );
    assert_eq!(
        native::version(&r.config.runtime_binary).unwrap(),
        "fixture 1"
    );
    assert!(native::check_runtime_version(&r.config, "wrong").is_err());
    assert!(native::check_runtime_version(&r.config, "fixture 1").is_ok());
    let cmd = native::command(&r);
    let scoped_socket = cmd
        .get_envs()
        .any(|(key, value)| key == "HC_SOCKET" && value == Some(r.socket().as_os_str()));
    assert!(scoped_socket);
    assert!(native::text(&json!({}), "/missing").is_err());
    assert!(native::pid(&json!({"pid":-1}), "/pid").is_err());
}
#[test]
fn readiness_socket_and_release_are_checked() {
    let dir = common::directory();
    let socket = dir.path().join("ready.sock");
    let _listener = UnixListener::bind(&socket).unwrap();
    native::wait_socket(&socket).unwrap();
    let (_dir, r, _fake) = common::launch(Runtime::Core);
    assert!(native::identity_from(&json!({"pid":std::process::id()})).is_ok());
    r.release().unwrap();
    assert!(r.binding().is_err());
}
#[test]
fn cli_invalid_requests_do_not_launch_anything() {
    assert!(engine::cli(&[]).is_err());
    assert!(engine::operate("invalid", "/does-not-exist".as_ref()).is_err());
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn configuration_io_cli_and_version_checks_do_not_start_unverified_programs() {
    let dir = common::directory();
    let mut config = common::config(dir.path(), Runtime::Core);
    config.runtime_version = "wrong-runtime".into();
    let path = dir.path().join("launch.toml");
    fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(engine::read_config(&path).unwrap().team, "test-team");
    assert!(
        engine::cli(&strings(&[
            "launch",
            path.to_str().unwrap(),
            "/tmp/cm-never-start"
        ]))
        .is_err()
    );
    assert!(!std::path::Path::new("/tmp/cm-never-start").exists());
    config.runtime_version = "fixture 1".into();
    assert!(native::versions_config(&config).is_err());
    assert!(native::version("/bin/false".as_ref()).is_err());
    assert!(native::version("/nonexistent/codex".as_ref()).is_err());
    assert!(engine::cli(&strings(&["--version"])).is_ok());
    assert!(engine::cli(&strings(&["nonsense"])).is_err());
    let (_dir, r, _fake) = common::launch(Runtime::Core);
    assert!(engine::operate("invalid", &r.root).is_err());
    assert!(engine::operate("inspect", &r.root).is_err());
    assert!(engine::operate("stop", &r.root).is_err());
}
#[test]
fn native_boot_failure_is_explicit_and_existing_socket_is_never_reused() {
    let dir = common::directory();
    let config = common::config(dir.path(), Runtime::Core);
    let record = Record::prepare(config, &dir.path().join("instance")).unwrap();
    assert!(Native.boot(&record).is_err());
    assert_eq!(
        fs::read_to_string(record.root.join("config/herdr-core/config.toml")).unwrap(),
        "notes = false\n"
    );
    let _socket = UnixListener::bind(record.socket()).unwrap();
    assert!(Native.boot(&record).is_err());
}

#[test]
fn tui_transport_startup_failure_retains_an_unknown_receipt() {
    let dir = common::directory();
    let config = common::config(dir.path(), Runtime::Tui);
    let state = dir.path().join("instance");
    assert!(engine::launch(config, &state, &Native).is_err());
    let receipt = Record::load(&state).unwrap();
    assert_eq!(receipt.phase, "unknown-outcome");
    assert_eq!(receipt.accepted, None);
    assert_eq!(receipt.ready, "unknown");
    assert!(!receipt.socket().exists());
}

#[test]
fn compact_core_launch_reply_can_be_completed_only_from_matching_owned_terminal() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    assert_eq!(r.created_ms, Some(42));
    r.created_ms = None;
    assert!(core::target(&r, &fake).is_err());
    r.accepted = None;
    assert!(core::inspect_target(&mut r, &fake).is_err());
    r.accepted = Some(true);
    core::inspect_target(&mut r, &fake).unwrap();
    assert_eq!(r.created_ms, Some(42));
    assert_eq!(
        fake.calls
            .borrow()
            .iter()
            .filter(|args| args.first().map(String::as_str) == Some("run"))
            .count(),
        1
    );
}

#[test]
fn tui_server_transport_is_not_in_the_callers_process_group() {
    let dir = common::directory();
    let record = Record::prepare(
        common::config(dir.path(), Runtime::Tui),
        &dir.path().join("instance"),
    )
    .unwrap();
    let mut command = native::server_command(&record).unwrap();
    let mut child = command
        .arg("hold")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stat = fs::read_to_string(format!("/proc/{}/stat", child.id())).unwrap();
    let (_, rest) = stat.rsplit_once(") ").unwrap();
    assert_eq!(
        rest.split_whitespace().nth(2).unwrap(),
        child.id().to_string()
    );
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn owned_transport_server_survives_its_launch_client_exit() {
    use std::io::Read;
    let dir = common::directory();
    let r = Record::prepare(
        common::config(dir.path(), Runtime::Tui),
        &dir.path().join("instance"),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_transport-fixture"))
        .arg("detach-child")
        .arg(&r.root)
        .output()
        .unwrap();
    assert!(output.status.success());
    let pid: u32 = String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let identity = process::identity(pid).unwrap();
    assert_eq!(
        identity.executable,
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_transport-fixture"))
    );
    let mut client =
        std::os::unix::net::UnixStream::connect(r.root.join("transport.sock")).unwrap();
    client.read_to_end(&mut Vec::new()).unwrap();
}

#[test]
fn failed_tui_start_can_close_only_its_original_empty_shell() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    r.accepted = None;
    r.agent_process = None;
    fake.discard_agent().unwrap();
    engine::stop(&mut r, &fake).unwrap();
    assert_eq!(r.phase, "stopped");
    assert_eq!(r.ready, "not-running");
    assert!(r.binding().is_err());
}

#[test]
fn tui_stop_refuses_an_added_or_unconfirmed_foreground_process() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    *fake.extra_process.borrow_mut() = true;
    assert!(engine::stop(&mut r, &fake).is_err());
    *fake.extra_process.borrow_mut() = false;
    r.agent_process = None;
    assert!(engine::stop(&mut r, &fake).is_err());
    let mutating = fake
        .calls
        .borrow()
        .iter()
        .any(|args| args == &strings(&["workspace", "close", "w1"]));
    assert!(!mutating);
}

#[test]
fn runtime_specific_daemon_arguments_do_not_duplicate_tui_injection() {
    let (_dir, r, _fake) = common::launch(Runtime::Tui);
    let tui_args = tui::launch_args(&r).unwrap();
    assert!(!tui_args.iter().any(|arg| arg == "--no-daemon"));
    let (_dir, r, _fake) = common::launch(Runtime::Core);
    assert_eq!(
        core::launch_args(&r)
            .iter()
            .filter(|arg| *arg == "--no-daemon")
            .count(),
        1
    );
}
#[test]
fn absence_requires_a_native_not_found_not_a_timeout_or_live_response() {
    use crewmoor::failure::{NativeFailure, require_absent};
    let missing =
        NativeFailure::decode(&json!({"error":{"code":"pane_not_found","message":"gone"}}));
    assert!(missing.to_string().contains("pane_not_found"));
    assert!(require_absent(Err(Box::new(missing))).is_ok());
    let timeout = NativeFailure::decode(&json!({"error":{"kind":"timeout"}}));
    assert!(require_absent(Err(Box::new(timeout))).is_err());
    assert!(require_absent(Ok(json!({"state":"live"}))).is_err());
}
#[test]
fn claimed_native_removal_with_a_live_process_keeps_binding_unknown() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    *fake.leave_actor.borrow_mut() = true;
    let result = engine::stop(&mut r, &fake);
    assert!(engine::preserve(&mut r, result).is_err());
    assert_eq!(r.phase, "unknown-outcome");
    assert!(r.binding().is_ok());
    assert!(!process::exited(r.agent_process.as_ref().unwrap()).unwrap());
}
