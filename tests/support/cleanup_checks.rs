use super::common;
use crewmoor::{config::Runtime, engine, holders, process, record::Record};
use serde_json::json;

fn stop_unknown(r: &mut Record, fake: &common::Fake) {
    let result = engine::stop(r, fake);
    assert!(engine::preserve(r, result).is_err());
    assert_eq!(Record::load(&r.root).unwrap().phase, "unknown-outcome");
    assert!(r.binding().is_ok());
}
#[test]
fn replaced_shell_in_the_same_owned_pane_is_never_adopted_at_stop() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    let original = r.original_shell.as_ref().unwrap().clone();
    process::same(&original).unwrap();
    fake.discard_agent().unwrap();
    fake.replace_shell().unwrap();
    stop_unknown(&mut r, &fake);
    assert!(process::exited(&original).unwrap());
    assert!(
        !fake
            .calls
            .borrow()
            .iter()
            .any(|args| args.get(1).map(String::as_str) == Some("close"))
    );
}
#[test]
fn a_surviving_holder_prevents_stopped_and_binding_release() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    let holder = r.core_holders.as_ref().unwrap()[0].process.clone();
    *fake.leave_holder.borrow_mut() = true;
    stop_unknown(&mut r, &fake);
    assert!(!process::exited(&holder).unwrap());
    assert!(process::exited(r.server.as_ref().unwrap()).unwrap());
    assert!(process::exited(r.agent_process.as_ref().unwrap()).unwrap());
}
#[test]
fn shell_fingerprint_is_checked_even_when_the_recorded_agent_is_foreground() {
    let (_dir, mut r, fake) = common::launch(Runtime::Tui);
    r.original_shell.as_mut().unwrap().start_ticks = "replaced".into();
    stop_unknown(&mut r, &fake);
    assert!(
        !fake
            .calls
            .borrow()
            .iter()
            .any(|args| args.get(1).map(String::as_str) == Some("close"))
    );
}
#[test]
fn changed_multiple_or_unknown_holders_refuse_before_ending_an_agent() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    *fake.multiple_holders.borrow_mut() = true;
    stop_unknown(&mut r, &fake);
    *fake.multiple_holders.borrow_mut() = false;
    *fake.changed_holder.borrow_mut() = true;
    stop_unknown(&mut r, &fake);
    *fake.changed_holder.borrow_mut() = false;
    fake.replace_holder().unwrap();
    stop_unknown(&mut r, &fake);
    r.core_holders = None;
    stop_unknown(&mut r, &fake);
    assert!(!*fake.killed.borrow());
}
#[test]
fn launch_records_a_new_holder_even_when_terminal_creation_fails() {
    let (dir, config, fake) = common::setup(Runtime::Core);
    *fake.fail_after_holder.borrow_mut() = true;
    let root = dir.path().join("instance");
    assert!(engine::launch(config, &root, &fake).is_err());
    let mut r = Record::load(&root).unwrap();
    assert!(r.handle.is_none());
    let holder = r.core_holders.as_ref().unwrap()[0].process.clone();
    engine::stop(&mut r, &fake).unwrap();
    assert!(process::exited(&holder).unwrap());
    assert!(r.binding().is_err());
}
#[test]
fn no_target_failure_can_end_only_its_verified_empty_instance() {
    for runtime in [Runtime::Core, Runtime::Tui] {
        let (_dir, mut r, fake) = failed_creation(runtime);
        assert!(r.handle.is_none());
        engine::stop(&mut r, &fake).unwrap();
        assert_eq!(r.phase, "stopped");
        assert!(r.binding().is_err());
        assert!(!r.socket().exists());
        assert!(process::exited(r.server.as_ref().unwrap()).unwrap());
    }
}
fn failed_creation(runtime: Runtime) -> (tempfile::TempDir, Record, common::Fake) {
    let (dir, config, fake) = common::setup(runtime);
    *fake.fail_creation.borrow_mut() = true;
    let root = dir.path().join("instance");
    assert!(engine::launch(config, &root, &fake).is_err());
    let r = Record::load(&root).unwrap();
    assert_eq!(r.phase, "unknown-outcome");
    r.guard().unwrap();
    (dir, r, fake)
}
#[test]
fn server_only_cleanup_refuses_nonempty_inventory_and_unknown_instance() {
    for runtime in [Runtime::Core, Runtime::Tui] {
        let (_dir, mut r, fake) = failed_creation(runtime);
        *fake.extra.borrow_mut() = true;
        stop_unknown(&mut r, &fake);
        process::same(r.server.as_ref().unwrap()).unwrap();
        *fake.extra.borrow_mut() = false;
        r.container_id = None;
        stop_unknown(&mut r, &fake);
        process::same(r.server.as_ref().unwrap()).unwrap();
    }
}
#[test]
fn a_remaining_socket_prevents_binding_release_after_processes_end() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    *fake.leave_socket.borrow_mut() = true;
    stop_unknown(&mut r, &fake);
    assert!(r.socket().exists());
    assert!(process::exited(r.server.as_ref().unwrap()).unwrap());
    assert!(process::exited(&r.core_holders.as_ref().unwrap()[0].process).unwrap());
}
#[test]
fn holder_schema_and_partial_target_records_fail_closed() {
    let value = json!({"pid":std::process::id(),"instance":"probe","current":true});
    assert_eq!(
        holders::read(&json!({"holders":[value],"holder":value}))
            .unwrap()
            .len(),
        1
    );
    assert!(holders::read(&json!({})).is_err());
    assert!(holders::read(&json!({"holders":[],"holder":value})).is_err());
    let mut changed = value.clone();
    changed["current"] = json!(false);
    assert!(holders::read(&json!({"holders":[changed],"holder":changed})).is_err());
    changed["current"] = json!(true);
    changed["instance"] = json!("");
    assert!(holders::read(&json!({"holders":[changed],"holder":changed})).is_err());
    let (_dir, mut r, _fake) = common::launch(Runtime::Tui);
    assert!(r.require_uncreated().is_err());
    r.handle = None;
    r.terminal_id = None;
    r.workspace_id = None;
    assert!(r.require_uncreated().is_err());
}
