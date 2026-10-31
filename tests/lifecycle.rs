#[path = "support/app_checks.rs"]
mod app_checks;
#[path = "support/common.rs"]
mod common;
#[path = "support/environment_checks.rs"]
mod environment_checks;
#[path = "support/schema_checks.rs"]
mod schema_checks;
use crewmoor::{
    config::{self, AgentKind, Runtime},
    core, engine,
    native::{self, Native, Runner, strings},
    process,
    record::Record,
    tui,
};
use serde_json::{Value, json};
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn four_combinations_use_native_handles_and_keep_hook_status_unknown() {
    for runtime in [Runtime::Tui, Runtime::Core] {
        for agent in [AgentKind::Codex, AgentKind::Claude] {
            let (dir, mut cfg, fake) = common::setup(runtime.clone());
            cfg.agent = agent.clone();
            cfg.agent_args = match agent {
                AgentKind::Codex => strings(&["--no-alt-screen"]),
                AgentKind::Claude => strings(&["--model", "claude-test-model"]),
            };
            *fake.minimal_agent.borrow_mut() = agent == AgentKind::Claude;
            let mut r = engine::launch(cfg, &dir.path().join("i"), &fake).unwrap();
            assert!(r.process_verified);
            assert_eq!(r.accepted, Some(true));
            assert_eq!(r.agent_status, "unknown");
            engine::verified_process(&r).unwrap();
            let args = match runtime {
                Runtime::Tui => tui::launch_args(&r).unwrap(),
                Runtime::Core => core::launch_args(&r),
            };
            assert!(fake.calls.borrow().contains(&args));
            assert_native_contract(&r, &fake);
            engine::inspect(&mut r, &fake).unwrap();
            assert_eq!(r.agent_status, "unknown");
            engine::stop(&mut r, &fake).unwrap();
            assert_eq!(r.phase, "stopped");
            assert!(r.binding().is_err());
            assert!(*fake.stopped.borrow());
            assert!(process::exited(r.agent_process.as_ref().unwrap()).unwrap());
        }
    }
}
fn assert_native_contract(r: &Record, fake: &common::Fake) {
    let calls = fake.calls.borrow();
    let prefix = match r.config.runtime {
        Runtime::Tui => "agent",
        Runtime::Core => "run",
    };
    let call = calls
        .iter()
        .find(|a| a.first().map(String::as_str) == Some(prefix))
        .unwrap();
    let boundary = call.iter().position(|a| a == "--").unwrap();
    let mut expected = Vec::new();
    if r.config.runtime == Runtime::Core {
        expected.push(env!("CARGO_BIN_EXE_transport-fixture").to_owned());
        if r.config.agent == AgentKind::Codex {
            expected.push("--no-daemon".into());
        }
    } else {
        assert_eq!(
            &call[3..7],
            strings(&["--kind", r.config.agent.name(), "--pane", "w1:p1"])
        );
    }
    expected.extend(r.config.agent_args.clone());
    assert_eq!(&call[boundary + 1..], expected);
    if r.config.agent == AgentKind::Claude {
        let invalid = call
            .iter()
            .any(|a| ["--no-daemon", "--no-alt-screen", "-m"].contains(&a.as_str()));
        assert!(!invalid);
    }
}
#[test]
fn official_tui_arguments_do_not_repeat_host_alias_flags_or_poll_state() {
    let (_dir, r, fake) = common::launch(Runtime::Tui);
    assert_eq!(
        tui::launch_args(&r).unwrap(),
        strings(&[
            "agent",
            "start",
            "probe",
            "--kind",
            "codex",
            "--pane",
            "w1:p1",
            "--timeout",
            "5000",
            "--"
        ])
    );
    let starts = fake
        .calls
        .borrow()
        .iter()
        .filter(|a| a.first().map(String::as_str) == Some("agent"))
        .count();
    assert_eq!(starts, 1);
    let semantic_call = fake
        .calls
        .borrow()
        .iter()
        .flatten()
        .any(|a| a == "wait" || a == "prompt");
    assert!(!semantic_call);
}
#[test]
fn ambiguous_start_does_not_resend_and_process_location_is_independent() {
    let (dir, cfg, fake) = common::setup(Runtime::Tui);
    *fake.ambiguous.borrow_mut() = true;
    let mut r = engine::launch(cfg, &dir.path().join("i"), &fake).unwrap();
    assert!(r.request_issued);
    assert_eq!(r.accepted, None);
    assert!(r.process_verified);
    assert_eq!(r.request_error.as_deref(), Some("timeout"));
    assert_eq!(
        fake.calls
            .borrow()
            .iter()
            .filter(|a| a.get(1).map(String::as_str) == Some("start"))
            .count(),
        1
    );
    engine::stop(&mut r, &fake).unwrap();
}
#[test]
fn missing_agent_process_is_not_launch_success_but_owned_native_pane_can_end() {
    for runtime in [Runtime::Tui, Runtime::Core] {
        let (dir, cfg, fake) = common::setup(runtime);
        *fake.no_agent.borrow_mut() = true;
        let mut r = engine::launch(cfg, &dir.path().join("i"), &fake).unwrap();
        assert!(!r.process_verified);
        assert!(r.agent_process.is_none());
        assert_eq!(r.agent_status, "unknown");
        engine::stop(&mut r, &fake).unwrap();
        assert_eq!(r.phase, "stopped");
    }
}
#[test]
fn extra_user_resources_survive_and_prevent_instance_stop_and_binding_release() {
    for runtime in [Runtime::Tui, Runtime::Core] {
        let (_dir, mut r, fake) = common::launch(runtime);
        *fake.extra.borrow_mut() = true;
        engine::stop(&mut r, &fake).unwrap();
        assert_eq!(r.phase, "partial-stop");
        assert!(r.binding().is_ok());
        assert!(!*fake.stopped.borrow());
        assert!(process::exited(r.agent_process.as_ref().unwrap()).unwrap());
        process::same(r.server.as_ref().unwrap()).unwrap();
    }
}
#[test]
fn native_target_changes_refuse_before_any_close_or_kill() {
    for runtime in [Runtime::Tui, Runtime::Core] {
        let (_dir, mut r, fake) = common::launch(runtime);
        *fake.changed.borrow_mut() = true;
        assert!(engine::stop(&mut r, &fake).is_err());
        assert!(!*fake.closed.borrow());
        assert!(!*fake.finished.borrow());
        assert!(r.binding().is_ok());
    }
}
#[test]
fn core_owner_generation_argv_and_cwd_must_match_returned_target() {
    let (_dir, r, fake) = common::launch(Runtime::Core);
    for key in ["address", "created_ms", "argv", "cwd", "labels"] {
        let mut value = fake.terminal(&r);
        value[key] = Value::Null;
        assert!(core::verify_target(&r, &value).is_err());
    }
}
#[test]
fn missing_optional_display_and_process_fields_do_not_become_semantic_state() {
    let (dir, cfg, fake) = common::setup(Runtime::Core);
    *fake.omit_name.borrow_mut() = true;
    let r = engine::launch(cfg, &dir.path().join("i"), &fake).unwrap();
    assert!(r.native_name.is_none());
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    *fake.missing_process.borrow_mut() = true;
    engine::inspect(&mut r, &fake).unwrap();
    assert!(!r.process_verified);
    assert_eq!(r.agent_status, "unknown");
    engine::stop(&mut r, &fake).unwrap();
}
#[test]
fn observations_do_not_persist_unrelated_foreground_argv_or_container_status() {
    let (_dir, r, _fake) = common::launch(Runtime::Tui);
    let text = fs::read_to_string(r.root.join("record.json")).unwrap();
    assert!(!text.contains("synthetic-sensitive-unrelated-value"));
    assert!(!text.contains("interactive_ready"));
    assert!(!text.contains("agent_status\": \"working"));
}
#[test]
fn actual_exit_and_socket_guards_preserve_binding_on_failed_native_cleanup() {
    for failure in ["actor", "holder", "socket"] {
        let (_dir, mut r, fake) = common::launch(Runtime::Core);
        match failure {
            "actor" => *fake.leave_actor.borrow_mut() = true,
            "holder" => *fake.leave_holder.borrow_mut() = true,
            _ => *fake.leave_socket.borrow_mut() = true,
        }
        let result = engine::stop(&mut r, &fake);
        assert!(engine::preserve(&mut r, result).is_err());
        assert!(r.binding().is_ok());
        assert_eq!(Record::load(&r.root).unwrap().phase, "unknown-outcome");
    }
}
#[test]
fn record_binding_server_socket_and_operation_lock_protect_instance_identity() {
    let (dir, mut r, _fake) = common::launch(Runtime::Core);
    let lock = r.lock().unwrap();
    assert!(r.lock().is_err());
    drop(lock);
    assert!(Record::prepare(r.config.clone(), &dir.path().join("other")).is_err());
    r.socket_inode = Some(0);
    assert!(r.guard().is_err());
    r.server.as_mut().unwrap().start_ticks = "changed".into();
    assert!(r.guard().is_err());
    fs::write(r.config.store.join("test-team.json"), "other").unwrap();
    assert!(r.binding().is_err());
}
#[test]
fn preserved_unknown_errors_never_relaunch_or_erase_process_facts() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    let birth = r.agent_process.clone();
    *fake.fail.borrow_mut() = true;
    let result = engine::inspect(&mut r, &fake);
    assert!(engine::preserve(&mut r, result).is_err());
    assert_eq!(r.agent_process, birth);
    assert!(!r.process_verified);
}
#[test]
fn kind_configurations_reject_tasks_unknown_agents_and_native_control_bytes() {
    let dir = common::directory();
    let mut cfg = common::config(dir.path(), Runtime::Tui);
    cfg.validate().unwrap();
    for args in [
        strings(&["prompt"]),
        strings(&["--model"]),
        strings(&["--model", "x\ny"]),
    ] {
        cfg.agent_args = args;
        assert!(cfg.validate().is_err());
    }
    cfg.agent = AgentKind::Claude;
    cfg.agent_args = strings(&["--no-alt-screen"]);
    assert!(cfg.validate().is_err());
    cfg.agent_args = strings(&["--model", "model ' ; $(false)"]);
    cfg.validate().unwrap();
    let mut value = serde_json::to_value(cfg).unwrap();
    value["agent"] = json!("hermes");
    assert!(serde_json::from_value::<config::Config>(value).is_err());
}
#[test]
fn bounded_cli_transport_and_native_protocol_errors_remain_explicit() {
    let start = Instant::now();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_transport-fixture"));
    cmd.arg("stall");
    assert!(crewmoor::transport::output(&mut cmd, Duration::from_millis(30)).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(native::payload(json!({"error":{"kind":"bad"}}), &Runtime::Core).is_err());
    assert!(engine::cli(&[]).is_err());
    let dir = common::directory();
    let cfg = common::config(dir.path(), Runtime::Core);
    native::versions_config(&cfg).unwrap();
    let r = Record::prepare(cfg, &dir.path().join("i")).unwrap();
    assert!(Native.call(&r, &strings(&["reply", "not json"])).is_err());
}
#[test]
fn native_absence_requires_not_found_and_pid_reuse_is_not_a_live_recorded_process() {
    let error = crewmoor::failure::NativeFailure {
        code: "not_found".into(),
        message: "absent".into(),
    };
    assert!(crewmoor::failure::require_absent(Err(Box::new(error))).is_ok());
    assert!(crewmoor::failure::require_absent(Ok(json!({}))).is_err());
    assert!(process::start_ticks("invalid").is_err());
    let mut identity = process::identity(std::process::id()).unwrap();
    identity.start_ticks = "old".into();
    assert!(process::exited(&identity).unwrap());
}

#[test]
fn existing_codex_alias_supplies_one_no_daemon_without_running_authentication() {
    let dir = common::directory();
    let path = dir.path().join("argv.json");
    let executable = env!("CARGO_BIN_EXE_transport-fixture");
    let script = format!("alias codex='{executable} --no-daemon';\ncodex --model 'two words'");
    let status = Command::new("/bin/bash")
        .args([
            "--noprofile",
            "--norc",
            "-O",
            "expand_aliases",
            "-c",
            &script,
        ])
        .env("CREWMOOR_ARGV_EVIDENCE", &path)
        .status()
        .unwrap();
    assert!(status.success());
    let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        value["argv"],
        json!([executable, "--no-daemon", "--model", "two words"])
    );
    assert_eq!(value["argv"], value["kernel_argv"]);
}

#[test]
fn configuration_cli_versions_and_fake_native_boot_failures_do_not_start_models() {
    let output = Command::new(env!("CARGO_BIN_EXE_crewmoor"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("crewmoor 0.1.0")
    );
    let dir = common::directory();
    let mut cfg = common::config(dir.path(), Runtime::Core);
    cfg.agent_args = strings(&["--no-daemon"]);
    cfg.validate().unwrap();
    assert_eq!(cfg.argv().iter().filter(|a| *a == "--no-daemon").count(), 1);
    let path = dir.path().join("config.toml");
    fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    assert_eq!(engine::read_config(&path).unwrap().team, "test-team");
    for runtime in [Runtime::Core, Runtime::Tui] {
        let dir = common::directory();
        let cfg = common::config(dir.path(), runtime);
        let record = Record::prepare(cfg, &dir.path().join("i")).unwrap();
        assert!(Native.boot(&record).is_err());
    }
}

#[test]
fn public_cli_rejects_version_and_transport_failures_without_relaunch() {
    let (dir, cfg, _fake) = common::setup(Runtime::Core);
    let path = dir.path().join("bad-version.toml");
    fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    let fresh = dir.path().join("new-state");
    let args = vec![
        "launch".into(),
        path.display().to_string(),
        fresh.display().to_string(),
    ];
    assert!(engine::cli(&args).is_err());
    assert!(!fresh.exists());
    let (_dir, r, _fake) = common::launch(Runtime::Core);
    for command in ["inspect", "stop", "unsupported"] {
        assert!(engine::cli(&[command.into(), r.root.display().to_string()]).is_err());
    }
    let saved = Record::load(&r.root).unwrap();
    assert_eq!(saved.phase, "unknown-outcome");
    assert!(saved.binding().is_ok());
}

#[test]
fn owned_transport_exec_handshake_and_input_eof_flush_a_real_successful_exit() {
    use std::io::Read;
    let mut child = Command::new(env!("CARGO_BIN_EXE_transport-fixture"))
        .arg("hold")
        .env("CREWMOOR_TRANSPORT_HANDSHAKE", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut marker = [0; 2];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut marker)
        .unwrap();
    assert_eq!(&marker, b"R\n");
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
}

#[test]
fn each_agent_rejects_wrong_version_and_kernel_executable_without_claiming_launch() {
    for agent in [AgentKind::Codex, AgentKind::Claude] {
        let dir = common::directory();
        let mut cfg = common::config(dir.path(), Runtime::Tui);
        cfg.agent = agent.clone();
        cfg.agent_version = "different agent version".into();
        assert!(native::versions_config(&cfg).is_err());
        let (dir, mut cfg, fake) = common::setup(Runtime::Tui);
        cfg.agent = agent;
        cfg.agent_executable = "/bin/false".into();
        let root = dir.path().join("i");
        assert!(engine::launch(cfg, &root, &fake).is_err());
        let saved = Record::load(&root).unwrap();
        assert!(!saved.process_verified);
        assert_eq!(saved.agent_status, "unknown");
        assert!(saved.binding().is_ok());
    }
}

#[test]
fn tui_closes_its_retained_empty_workspace_before_stopping_the_private_session() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    engine::stop(&mut r, &fake).unwrap();
    assert!(*fake.workspace_closed.borrow());
    assert!(*fake.stopped.borrow());
    assert_eq!(r.phase, "stopped");
    let calls = fake.calls.borrow();
    let pane = calls
        .iter()
        .position(|a| a == &strings(&["pane", "close", "w1:p1"]))
        .unwrap();
    let workspace = calls
        .iter()
        .position(|a| a == &strings(&["workspace", "close", "w1"]))
        .unwrap();
    let session = calls
        .iter()
        .position(|a| a.first().map(String::as_str) == Some("session"))
        .unwrap();
    assert!(pane < workspace);
    assert!(workspace < session);
}

#[test]
fn tui_keeps_user_panes_tabs_and_unconfirmed_workspace_removal_partial() {
    for kind in ["pane", "tab", "retained"] {
        let (_dir, mut r, fake) = common::launch(Runtime::Tui);
        match kind {
            "pane" => *fake.extra_pane.borrow_mut() = true,
            "tab" => *fake.extra_tab.borrow_mut() = true,
            _ => *fake.leave_workspace.borrow_mut() = true,
        }
        engine::stop(&mut r, &fake).unwrap();
        assert!(*fake.closed.borrow());
        assert!(!*fake.workspace_closed.borrow());
        assert!(!*fake.stopped.borrow());
        assert!(r.binding().is_ok());
        assert_eq!(r.phase, "partial-stop");
    }
}
