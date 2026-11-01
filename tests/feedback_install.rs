use crewmoor::{
    config::AgentKind,
    feedback::{self, Evidence, Registration, install},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn existing() -> Value {
    json!({"env":{"SECRET":"do-not-print"},"permissions":{"allow":["Read"]},
        "hooks":{"SessionStart":[{"matcher":"startup|resume","hooks":[
            {"type":"command","command":"orca-unchanged","timeout":4},
            {"type":"command","command":"herdr-unchanged"}]}],
        "Stop":[{"hooks":[{"type":"command","command":"my-user-hook"}]}]}})
}
fn command(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_crewmoor"));
    cmd.arg("feedback")
        .arg("hooks")
        .env("HOME", home)
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR");
    cmd
}
fn prepare(home: &Path, kind: &str) -> std::path::PathBuf {
    let name = if kind == "codex" {
        ".codex/hooks.json"
    } else {
        ".claude/settings.json"
    };
    let path = home.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&existing()).unwrap()).unwrap();
    path
}
fn plan(home: &Path, kind: &str, action: &str, name: &str) -> Output {
    command(home)
        .args(["plan", kind, action])
        .arg(home.join("registry"))
        .arg(env!("CARGO_BIN_EXE_crewmoor"))
        .arg(home.join(name))
        .output()
        .unwrap()
}
#[test]
fn both_user_installers_preserve_herdr_orca_and_are_idempotent_and_target_scoped() {
    for kind in ["claude", "codex"] {
        let home = tempfile::tempdir().unwrap();
        let native = prepare(home.path(), kind);
        let original = fs::read(&native).unwrap();
        let preview = plan(home.path(), kind, "install", "first");
        assert!(
            preview.status.success(),
            "{}",
            String::from_utf8_lossy(&preview.stderr)
        );
        assert_eq!(fs::read(&native).unwrap(), original);
        let displayed = String::from_utf8(preview.stdout).unwrap();
        assert!(!displayed.contains("do-not-print"));
        assert_eq!(
            fs::read(home.path().join("first/before.json")).unwrap(),
            original
        );
        assert!(
            command(home.path())
                .arg("apply")
                .arg(home.path().join("first"))
                .status()
                .unwrap()
                .success()
        );
        let installed = fs::read(&native).unwrap();
        let doc: Value = serde_json::from_slice(&installed).unwrap();
        assert_eq!(doc["env"], existing()["env"]);
        assert_eq!(doc["permissions"], existing()["permissions"]);
        assert_eq!(
            doc["hooks"]["SessionStart"][0],
            existing()["hooks"]["SessionStart"][0]
        );
        assert!(
            plan(home.path(), kind, "install", "repeat")
                .status
                .success()
        );
        let repeat = command(home.path())
            .arg("apply")
            .arg(home.path().join("repeat"))
            .output()
            .unwrap();
        assert!(repeat.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&repeat.stdout).unwrap()["changed"],
            false
        );
        assert_eq!(fs::read(&native).unwrap(), installed);
        assert!(
            plan(home.path(), kind, "uninstall", "remove")
                .status
                .success()
        );
        assert!(
            command(home.path())
                .arg("apply")
                .arg(home.path().join("remove"))
                .status()
                .unwrap()
                .success()
        );
        let removed: Value = serde_json::from_slice(&fs::read(&native).unwrap()).unwrap();
        assert!(!removed.to_string().contains("crewmoor-feedback-v1"));
        assert_eq!(
            removed["hooks"]["SessionStart"][0],
            existing()["hooks"]["SessionStart"][0]
        );
        assert_eq!(removed["hooks"]["Stop"], existing()["hooks"]["Stop"]);
        assert!(
            command(home.path())
                .arg("rollback")
                .arg(home.path().join("remove"))
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(fs::read(&native).unwrap(), installed);
    }
}
#[test]
fn own_handler_in_shared_matcher_does_not_take_its_sibling_or_metadata_on_uninstall() {
    let own = install::handler(
        &AgentKind::Claude,
        Path::new("/registry"),
        Path::new("/bin/crewmoor"),
    );
    let doc = json!({"hooks":{"Stop":[{"matcher":"*","annotation":"keep","hooks":[own,
        {"type":"command","command":"user-keep"}]}]}});
    let removed = install::edit(&doc, &AgentKind::Claude, "uninstall", &json!(null)).unwrap();
    assert_eq!(removed["hooks"]["Stop"][0]["annotation"], "keep");
    assert_eq!(
        removed["hooks"]["Stop"][0]["hooks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let unrelated = json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo crewmoor-feedback-v1"}]}]}});
    assert_eq!(
        install::edit(&unrelated, &AgentKind::Claude, "uninstall", &json!(null)).unwrap(),
        unrelated
    );
}
#[test]
fn stale_preview_and_modified_proposal_refuse_without_replacing_user_configuration() {
    let home = tempfile::tempdir().unwrap();
    let native = prepare(home.path(), "claude");
    assert!(
        plan(home.path(), "claude", "install", "stale")
            .status
            .success()
    );
    fs::write(&native, b"{\"other\":true}").unwrap();
    assert!(
        !command(home.path())
            .arg("apply")
            .arg(home.path().join("stale"))
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(&native).unwrap(), b"{\"other\":true}");
    assert!(
        plan(home.path(), "claude", "install", "tamper")
            .status
            .success()
    );
    fs::write(home.path().join("tamper/after.json"), b"{}").unwrap();
    assert!(
        !command(home.path())
            .arg("apply")
            .arg(home.path().join("tamper"))
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(&native).unwrap(), b"{\"other\":true}");
}
#[test]
fn absent_user_config_can_be_created_and_safely_rolled_back_without_following_symlinks() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        plan(home.path(), "codex", "install", "new")
            .status
            .success()
    );
    assert!(
        command(home.path())
            .arg("apply")
            .arg(home.path().join("new"))
            .status()
            .unwrap()
            .success()
    );
    assert!(
        command(home.path())
            .arg("rollback")
            .arg(home.path().join("new"))
            .status()
            .unwrap()
            .success()
    );
    assert!(!home.path().join(".codex/hooks.json").exists());
    fs::write(home.path().join("protected"), b"{}").unwrap();
    std::os::unix::fs::symlink(
        home.path().join("protected"),
        home.path().join(".codex/hooks.json"),
    )
    .unwrap();
    assert!(
        !plan(home.path(), "codex", "install", "symlink")
            .status
            .success()
    );
    assert_eq!(fs::read(home.path().join("protected")).unwrap(), b"{}");
}
#[test]
fn installed_command_quotes_executable_and_registry_as_data_and_reaches_rust_receiver() {
    let dir = tempfile::tempdir().unwrap();
    let registry = dir.path().join("registry ' ; touch injected ; '");
    let binary = dir.path().join("callback ' binary");
    fs::copy(env!("CARGO_BIN_EXE_crewmoor"), &binary).unwrap();
    let context = feedback::register(
        &registry,
        Registration {
            team: "test".into(),
            role: "developer".into(),
            task: "issue-4".into(),
            cwd: dir.path().into(),
            agent: AgentKind::Claude,
            agent_version: "2.1.316 (Claude Code)".into(),
            native_session_id: None,
            evidence: Evidence::Synthetic,
        },
    )
    .unwrap();
    let handler = install::handler(&AgentKind::Claude, &registry, &binary);
    let mut child = Command::new("/bin/sh")
        .args(["-c", handler["command"].as_str().unwrap()])
        .current_dir(dir.path())
        .env(feedback::INSTANCE_ENV, &context.instance)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            &serde_json::to_vec(
                &json!({"session_id":"s","cwd":dir.path(),"hook_event_name":"SessionStart"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    assert!(!dir.path().join("injected").exists());
    assert_eq!(
        feedback::show(&registry, &context.instance).unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
