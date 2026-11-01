use super::{app_checks, common};
use crewmoor::{
    app,
    config::{AgentKind, Runtime},
    engine,
    native::strings,
    project::{self, Project},
    record::Record,
};
use serde_json::json;
use std::{fs, path::Path};

fn auto_target(base: &Path) -> app::Binding {
    let mut target = app_checks::binding(base);
    target.space_id = None;
    target.tab_id = None;
    target
}
fn empty_doc() -> serde_json::Value {
    json!({"version":0,"value":null})
}
fn entry(cwd: &Path) -> Project {
    Project {
        id: "project-owned".into(),
        core_id: "own-core".into(),
        cwd: cwd.into(),
        space_id: "sp-owned".into(),
        tab_id: "tb-owned".into(),
    }
}
#[test]
fn both_agents_use_target_profile_and_persist_project_before_one_native_request() {
    for kind in [AgentKind::Codex, AgentKind::Claude] {
        let (dir, mut cfg, fake) = common::setup(Runtime::Core);
        cfg.agent = kind.clone();
        cfg.agent_args = strings(&["--model", "synthetic project model"]);
        let profile = dir.path().join("vps.toml");
        fs::write(&profile, toml::to_string(&auto_target(dir.path())).unwrap()).unwrap();
        cfg.app_target = Some(profile.clone());
        *fake.app_doc.borrow_mut() = Some(empty_doc());
        let mut first = engine::launch(cfg.clone(), &dir.path().join("first"), &fake).unwrap();
        let project = first.project.clone().unwrap();
        assert_eq!(project.cwd, dir.path().canonicalize().unwrap());
        assert!(first.process_verified);
        assert_eq!(first.accepted, Some(true));
        let binding = first.config.app.as_ref().unwrap();
        assert_eq!(
            binding.local_ids().unwrap(),
            (project.space_id.as_str(), project.tab_id.as_str())
        );
        let (_, params) =
            app::request(&first, &crewmoor::core::launch_args(&first).unwrap()).unwrap();
        assert_eq!(params["labels"]["crewmoor-project"], project.id);
        assert_eq!(params["labels"]["herdr.space"], project.space_id);
        assert_eq!(params["keep"], true);
        assert_eq!(params["argv"], json!(first.config.argv()));
        if kind == AgentKind::Claude {
            assert!(!first.config.argv().contains(&"--no-daemon".into()));
        }
        let saved = Record::load(&first.root).unwrap();
        assert_eq!(saved.project, Some(project.clone()));
        engine::stop(&mut first, &fake).unwrap();
        assert_eq!(first.phase, "target-stopped-host-retained");
        let mut target = auto_target(dir.path());
        target.socket = dir.path().join("second.sock");
        target.machine = "same-core-new-alias".into();
        fs::write(&profile, toml::to_string(&target).unwrap()).unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
        cfg.cwd = alias;
        cfg.team = "second-team".into();
        let second_fake = common::Fake::default();
        *second_fake.app_doc.borrow_mut() = Some(empty_doc());
        let mut second = engine::launch(cfg, &dir.path().join("second"), &second_fake).unwrap();
        assert_eq!(second.project, Some(project));
        let index = second.config.store.join("projects/index.json");
        assert_eq!(project::read(&index).unwrap().len(), 1);
        engine::stop(&mut second, &second_fake).unwrap();
        assert!(!*second_fake.stopped.borrow());
    }
}
#[test]
fn native_rpc_auto_space_runs_both_agent_branches_with_unknown_ids() {
    for agent in [AgentKind::Codex, AgentKind::Claude] {
        app_checks::native_transport(agent, true);
    }
}
#[test]
fn project_identity_uses_core_and_canonical_path_never_user_space_display_name() {
    let dir = common::directory();
    let one = dir.path().join("one/crewmoor");
    let two = dir.path().join("two/crewmoor");
    fs::create_dir_all(&one).unwrap();
    fs::create_dir_all(&two).unwrap();
    let doc = json!({"version":1,"value":{"spaces":[{"id":"user-space","name":"crewmoor","folder":one,
        "tabs":[{"id":"user-tab","name":"1"}]}]}});
    let before = doc.clone();
    let store = dir.path().join("store");
    let p1 = project::resolve(&store, "core-a", &one, &doc).unwrap();
    let p2 = project::resolve(&store, "core-a", &two, &doc).unwrap();
    let p3 = project::resolve(&store, "core-b", &one, &doc).unwrap();
    assert_ne!(p1.id, p2.id);
    assert_ne!(p1.id, p3.id);
    assert_ne!(p1.space_id, "user-space");
    assert_ne!(p1.tab_id, "user-tab");
    assert_eq!(project::resolve(&store, "core-a", &one, &doc).unwrap(), p1);
    assert_eq!(
        project::read(&store.join("projects/index.json"))
            .unwrap()
            .len(),
        3
    );
    assert_eq!(doc, before);
}
#[test]
fn invalid_or_duplicate_project_records_and_busy_store_withhold_request() {
    for case in ["invalid", "duplicate", "ids", "slash-id", "lock"] {
        let (dir, mut cfg, fake) = common::setup(Runtime::Core);
        cfg.app = Some(auto_target(dir.path()));
        let registry = cfg.store.join("projects");
        fs::create_dir_all(&registry).unwrap();
        let p = entry(&dir.path().canonicalize().unwrap());
        match case {
            "invalid" => fs::write(registry.join("index.json"), "not-json").unwrap(),
            "duplicate" => fs::write(
                registry.join("index.json"),
                serde_json::to_vec(&vec![p.clone(), p]).unwrap(),
            )
            .unwrap(),
            "ids" => {
                let mut p = p;
                p.space_id.clear();
                fs::write(
                    registry.join("index.json"),
                    serde_json::to_vec(&vec![p]).unwrap(),
                )
                .unwrap();
            }
            "slash-id" => {
                let mut p = p;
                p.id = "/not-local".into();
                fs::write(
                    registry.join("index.json"),
                    serde_json::to_vec(&vec![p]).unwrap(),
                )
                .unwrap();
            }
            _ => fs::write(registry.join("operation.lock"), "synthetic lock").unwrap(),
        }
        let root = dir.path().join("i");
        assert!(engine::launch(cfg, &root, &fake).is_err());
        let record = Record::load(&root).unwrap();
        assert!(!record.request_issued);
        assert!(record.binding().is_ok());
        assert!(!*fake.created.borrow());
    }
}
#[test]
fn auto_ids_accept_official_empty_document_but_reject_collision_or_invalid_data() {
    let dir = common::directory();
    let p = entry(dir.path());
    for valid in [
        empty_doc(),
        json!({"value":{}}),
        json!({"value":{"spaces":[{}]}}),
    ] {
        project::ensure_unused(&p, &valid).unwrap();
    }
    for invalid in [
        json!({}),
        json!({"version":1,"value":null}),
        json!({"value":3}),
        json!({"value":{"spaces":null}}),
        json!({"value":{"spaces":[{"tabs":null}]}}),
        json!({"value":{"spaces":[{"id":"sp-owned"}]}}),
        json!({"value":{"spaces":[{"id":"user-space","tabs":[{"id":"tb-owned"}]}]}}),
    ] {
        assert!(project::ensure_unused(&p, &invalid).is_err());
    }
}
#[test]
fn explicit_layout_is_preserved_and_partial_id_or_conflicting_profile_is_rejected() {
    let dir = common::directory();
    let mut target = auto_target(dir.path());
    target.validate(&Runtime::Core).unwrap();
    assert!(target.local_ids().is_err());
    target.space_id = Some("sp-only".into());
    assert!(target.validate(&Runtime::Core).is_err());
    let mut cfg = common::config(dir.path(), Runtime::Core);
    cfg.app = Some(target);
    cfg.app_target = Some(dir.path().join("profile.toml"));
    assert!(cfg.resolve_target().is_err());
    cfg.app = None;
    cfg.app_target = Some("relative".into());
    assert!(cfg.resolve_target().is_err());
    cfg.app_target = Some(dir.path().join("missing.toml"));
    assert!(cfg.resolve_target().is_err());
    let (dir, mut cfg, fake) = common::setup(Runtime::Core);
    cfg.app = Some(app_checks::binding(dir.path()));
    let mut r = engine::launch(cfg, &dir.path().join("i"), &fake).unwrap();
    assert!(r.project.is_none());
    assert!(!r.config.store.join("projects/index.json").exists());
    engine::stop(&mut r, &fake).unwrap();
}
#[test]
fn unknown_project_launch_keeps_ids_and_binding_without_resend() {
    let (dir, mut cfg, fake) = common::setup(Runtime::Core);
    cfg.app = Some(auto_target(dir.path()));
    *fake.ambiguous.borrow_mut() = true;
    let root = dir.path().join("i");
    assert!(engine::launch(cfg, &root, &fake).is_err());
    let record = Record::load(&root).unwrap();
    assert!(record.request_issued);
    assert_eq!(record.accepted, None);
    assert!(record.project.is_some());
    assert!(record.binding().is_ok());
    assert_eq!(
        project::read(&record.config.store.join("projects/index.json")).unwrap(),
        vec![record.project.unwrap()]
    );
    let runs = fake
        .calls
        .borrow()
        .iter()
        .filter(|call| call.first().map(String::as_str) == Some("run"))
        .count();
    assert_eq!(runs, 1);
}
