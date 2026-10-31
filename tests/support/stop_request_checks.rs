use super::common;
use crewmoor::{config::Runtime, engine, launch, process, record::Record};
use std::process::Command;

fn refused(r: &mut Record, fake: &common::Fake) {
    let result = engine::stop(r, fake);
    assert!(engine::preserve(r, result).is_err());
    assert_eq!(r.phase, "unknown-outcome");
    assert!(!r.process_verified);
    assert!(r.binding().is_ok());
    let saved = Record::load(&r.root).unwrap();
    assert_eq!(saved.agent_process, r.agent_process);
    assert_eq!(saved.request_issued, r.request_issued);
    assert_eq!(saved.phase, "unknown-outcome");
    assert!(!saved.process_verified);
    assert!(saved.binding().is_ok());
    assert!(!*fake.closed.borrow());
    assert!(
        !fake
            .calls
            .borrow()
            .iter()
            .any(|args| args.get(1).map(String::as_str) == Some("stop"))
    );
}
#[test]
fn admitted_request_without_any_observed_agent_cannot_clear_an_empty_shell() {
    let (dir, config, fake) = common::setup(Runtime::Tui);
    *fake.failed_start.borrow_mut() = true;
    let mut r = engine::launch(config, &dir.path().join("i"), &fake).unwrap();
    assert!(r.request_issued);
    assert_eq!(r.accepted, Some(true));
    assert!(r.agent_process.is_none());
    refused(&mut r, &fake);
}
#[test]
fn exited_recorded_agent_is_retained_and_does_not_imply_container_cleanup() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    let birth = r.agent_process.clone();
    fake.discard_agent().unwrap();
    assert!(process::exited(birth.as_ref().unwrap()).unwrap());
    refused(&mut r, &fake);
    assert_eq!(r.agent_process, birth);
}
#[test]
fn an_unverified_observation_cannot_be_repaired_implicitly_during_stop() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    let birth = r.agent_process.clone();
    *fake.omit_foreground.borrow_mut() = true;
    engine::inspect(&mut r, &fake).unwrap();
    *fake.omit_foreground.borrow_mut() = false;
    refused(&mut r, &fake);
    assert_eq!(r.agent_process, birth);
}
#[test]
fn inconsistent_no_request_flag_does_not_erase_a_recorded_agent() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    fake.discard_agent().unwrap();
    r.request_issued = false;
    refused(&mut r, &fake);
    assert!(r.agent_process.is_some());
}
#[test]
fn argv_build_failure_is_known_unsent_and_keeps_original_shell_evidence() {
    let (dir, mut config, fake) = common::setup(Runtime::Tui);
    config.agent_executable = "/invalid\0executable".into();
    let root = dir.path().join("i");
    assert!(engine::launch(config, &root, &fake).is_err());
    let mut r = Record::load(&root).unwrap();
    assert!(!r.request_issued);
    assert!(r.original_shell.is_some());
    assert!(
        !fake
            .calls
            .borrow()
            .iter()
            .any(|args| args.get(1).map(String::as_str) == Some("run"))
    );
    engine::stop(&mut r, &fake).unwrap();
    assert_eq!(r.phase, "stopped");
}
#[test]
fn absolute_executable_ignores_a_shadowed_command_function() {
    let prefix = "function command() { return 97; };";
    let old = format!("{prefix} command /bin/true");
    let old_status = Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-c", &old])
        .status()
        .unwrap();
    assert_eq!(old_status.code(), Some(97));
    let line = launch::command(&["/bin/true".into()]).unwrap();
    let fixed = format!("{prefix} {line}");
    assert!(
        Command::new("/bin/bash")
            .args(["--noprofile", "--norc", "-c", &fixed])
            .status()
            .unwrap()
            .success()
    );
    assert!(launch::command(&["relative".into()]).is_err());
}
