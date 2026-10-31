use super::common;
use crewmoor::{
    config::{AgentKind, Runtime},
    core, engine, holders, launch,
    native::{self, Native, Runner, strings},
    record::Record,
    transport,
};
use serde_json::{Value, json};
use std::{
    io::Read,
    process::Command,
    time::{Duration, Instant},
};
#[test]
fn transport_exec_handshake_and_input_eof_complete_without_killing_the_fixture() {
    for handshake in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_transport-fixture"));
        command
            .arg("hold")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        command.env_remove("CREWMOOR_TRANSPORT_HANDSHAKE");
        if handshake {
            command.env("CREWMOOR_TRANSPORT_HANDSHAKE", "1");
        }
        let mut child = command.spawn().unwrap();
        if handshake {
            let mut byte = [0];
            child
                .stdout
                .as_mut()
                .unwrap()
                .read_exact(&mut byte)
                .unwrap();
            assert_eq!(byte, *b"R");
        }
        child.stdin.take();
        assert!(child.wait().unwrap().success());
    }
}

#[test]
fn both_agents_start_locate_and_end_in_both_runtime_paths_without_semantic_state() {
    for runtime in [Runtime::Core, Runtime::Tui] {
        for agent in [AgentKind::Codex, AgentKind::Claude] {
            let (dir, mut config, fake) = common::setup(runtime.clone());
            config.agent = agent.clone();
            let mut r = engine::launch(config, &dir.path().join("i"), &fake).unwrap();
            assert_eq!(r.accepted, Some(true));
            assert!(r.request_issued);
            assert!(r.process_verified);
            engine::inspect(&mut r, &fake).unwrap();
            let value = serde_json::to_value(&r).unwrap();
            assert!(value.get("ready").is_none());
            assert_eq!(
                r.config
                    .argv()
                    .iter()
                    .filter(|arg| *arg == "--no-daemon")
                    .count(),
                usize::from(agent == AgentKind::Codex)
            );
            engine::stop(&mut r, &fake).unwrap();
            assert_eq!(r.phase, "stopped");
            assert!(
                !fake
                    .calls
                    .borrow()
                    .iter()
                    .any(|args| args.first().map(String::as_str) == Some("agent"))
            );
        }
    }
}
#[test]
fn kind_parameters_versions_and_unsupported_agents_are_explicit() {
    let dir = common::directory();
    let mut c = common::config(dir.path(), Runtime::Core);
    native::versions_config(&c).unwrap();
    c.agent_version = "unexpected".into();
    assert!(native::versions_config(&c).is_err());
    c.agent = AgentKind::Claude;
    c.agent_args = strings(&["--no-alt-screen"]);
    assert!(c.validate().is_err());
    c.agent_args = strings(&["-m", "model"]);
    assert!(c.validate().is_err());
    c.agent_args = strings(&["--model", "model with spaces"]);
    c.validate().unwrap();
    assert!(!c.argv().iter().any(|arg| arg == "--no-daemon"));
    let mut value = serde_json::to_value(&c).unwrap();
    for other in ["hermes", "other"] {
        value["agent"] = json!(other);
        assert!(serde_json::from_value::<crewmoor::config::Config>(value.clone()).is_err());
    }
    assert!(launch::word("bad\0argument").is_err());
    assert!(launch::command(&[]).is_err());
}
#[test]
fn missing_or_null_core_name_and_missing_holder_alias_do_not_block_owned_cleanup() {
    for null in [false, true] {
        let (dir, config, fake) = common::setup(Runtime::Core);
        *fake.omit_holder.borrow_mut() = true;
        *fake.omit_name.borrow_mut() = !null;
        *fake.null_name.borrow_mut() = null;
        let mut r = engine::launch(config, &dir.path().join("i"), &fake).unwrap();
        assert!(r.native_name.is_none());
        engine::inspect(&mut r, &fake).unwrap();
        let mut target = fake.terminal(&r);
        target["name"] = json!("conflict");
        assert!(core::verify_target(&r, &target).is_err());
        let mut status = core::status(&r, &fake).unwrap();
        status["holder"] = Value::Null;
        assert!(holders::read(&status).is_err());
        engine::stop(&mut r, &fake).unwrap();
    }
}
#[test]
fn required_only_process_info_withholds_input_and_cleanup_without_adopting_a_shell() {
    let (dir, config, fake) = common::setup(Runtime::Tui);
    *fake.missing_process.borrow_mut() = true;
    let mut r = engine::launch(config, &dir.path().join("i"), &fake).unwrap();
    assert!(r.original_shell.is_none());
    assert!(!r.request_issued);
    assert!(!r.process_verified);
    assert!(engine::stop(&mut r, &fake).is_err());
    assert!(r.binding().is_ok());
    assert!(
        !fake
            .calls
            .borrow()
            .iter()
            .any(|args| args.get(1).map(String::as_str) == Some("run"))
    );
}
#[test]
fn missing_process_evidence_remains_unknown_and_never_permits_stop() {
    for null in [false, true] {
        let (_dir, mut r, fake) = common::launch(Runtime::Tui);
        *fake.missing_process.borrow_mut() = !null;
        *fake.null_process.borrow_mut() = null;
        engine::inspect(&mut r, &fake).unwrap();
        assert!(!r.process_verified);
        assert!(engine::stop(&mut r, &fake).is_err());
        assert!(r.binding().is_ok());
    }
}
#[test]
fn input_timeout_is_not_retried_and_process_proof_is_independent_of_admission() {
    let (dir, config, fake) = common::setup(Runtime::Tui);
    *fake.ambiguous_start.borrow_mut() = true;
    let mut r = engine::launch(config, &dir.path().join("i"), &fake).unwrap();
    assert_eq!(r.accepted, None);
    assert!(r.request_error.is_some());
    assert!(r.process_verified);
    assert_eq!(
        fake.calls
            .borrow()
            .iter()
            .filter(|args| args.get(1).map(String::as_str) == Some("run"))
            .count(),
        1
    );
    engine::stop(&mut r, &fake).unwrap();
}
#[test]
fn missing_core_pid_does_not_become_a_verified_process_or_permit_stop() {
    let (dir, config, fake) = common::setup(Runtime::Core);
    *fake.minimal_terminal.borrow_mut() = true;
    let mut r = engine::launch(config, &dir.path().join("i"), &fake).unwrap();
    assert!(!r.process_verified);
    assert!(r.agent_process.is_none());
    assert!(r.native_name.is_none());
    assert!(engine::stop(&mut r, &fake).is_err());
    assert!(!*fake.killed.borrow());
}
#[test]
fn executable_mismatch_is_not_adopted_by_inspect_or_cleanup() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    r.config.agent_executable = "/bin/false".into();
    assert!(engine::inspect(&mut r, &fake).is_err());
    assert!(engine::stop(&mut r, &fake).is_err());
    assert!(r.binding().is_ok());
}
#[test]
fn bounded_native_cli_and_blank_pane_admission_do_not_require_json_or_ready() {
    let dir = common::directory();
    let r = Record::prepare(
        common::config(dir.path(), Runtime::Tui),
        &dir.path().join("i"),
    )
    .unwrap();
    assert_eq!(
        Native
            .call(&r, &strings(&["pane", "run", "w1:p1", "blank-ack"]))
            .unwrap(),
        Value::Null
    );
    let start = Instant::now();
    let mut command = Command::new(env!("CARGO_BIN_EXE_transport-fixture"));
    command.arg("stall");
    let error = transport::output(&mut command, Duration::from_millis(40)).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<crewmoor::failure::NativeFailure>()
            .unwrap()
            .code,
        "timeout"
    );
    assert!(start.elapsed() < Duration::from_secs(5));
}
#[test]
fn shell_quoting_preserves_both_agent_argv_including_control_bytes_and_metacharacters() {
    for kind in [AgentKind::Codex, AgentKind::Claude] {
        let dir = common::directory();
        let output = dir.path().join("argv.json");
        let mut config = common::config(dir.path(), Runtime::Tui);
        config.agent = kind;
        config.agent_args = vec![
            "--model".into(),
            "space '\";$(false) `false`\n\t\u{3}trailing\n".into(),
        ];
        config.validate().unwrap();
        let line = launch::command(&config.argv()).unwrap();
        assert!(!line.contains('\n'));
        assert!(!line.contains('\t'));
        assert!(!line.contains('\u{3}'));
        let script = format!(
            "alias codex=false; alias claude=false; function command() {{ return 97; }};\n{line}"
        );
        let status = Command::new("/bin/bash")
            .args([
                "--noprofile",
                "--norc",
                "-O",
                "expand_aliases",
                "-c",
                &script,
            ])
            .current_dir(dir.path())
            .env("CREWMOOR_ARGV_EVIDENCE", &output)
            .status()
            .unwrap();
        assert!(status.success());
        let value: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(value["argv"], json!(config.argv()));
        assert_eq!(value["kernel_argv"], json!(config.argv()));
    }
}
