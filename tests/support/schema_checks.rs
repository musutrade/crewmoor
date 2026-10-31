use super::common;
use crewmoor::{
    config::Runtime,
    core, engine,
    native::{Runner, strings},
    process,
};
use serde_json::{Value, json};

fn contracts(version: &str) -> Value {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(format!("{version}-fields.json"));
    let data: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    data["contracts"].clone()
}
fn field_contract(value: &Value, schema: &Value) -> bool {
    let required = schema["required"].as_array().unwrap();
    let allowed = schema["properties"].as_array().unwrap();
    required
        .iter()
        .all(|name| value.get(name.as_str().unwrap()).is_some())
        && value
            .as_object()
            .unwrap()
            .keys()
            .all(|key| allowed.contains(&json!(key)))
}
#[test]
fn real_terminal_shape_without_holder_supports_inspect_and_complete_stop() {
    let (_dir, mut r, fake) = common::launch(Runtime::Core);
    let fields = contracts("core-0.1.15");
    let terminal = core::target(&r, &fake).unwrap();
    assert!(terminal.get("holder").is_none());
    assert!(field_contract(&terminal, &fields["Terminal"]));
    let status = core::status(&r, &fake).unwrap();
    assert!(field_contract(&status, &fields["StatusResult"]));
    assert!(field_contract(&status["core"], &fields["CoreInfo"]));
    assert!(field_contract(&status["holder"], &fields["HolderInfo"]));
    engine::inspect(&mut r, &fake).unwrap();
    let holder = r.core_holders.as_ref().unwrap()[0].process.clone();
    engine::stop(&mut r, &fake).unwrap();
    assert!(process::exited(&holder).unwrap());
    assert!(r.binding().is_err());
}
#[test]
fn pinned_terminal_contract_rejects_an_invented_holder_field() {
    let (_dir, r, fake) = common::launch(Runtime::Core);
    let fields = contracts("core-0.1.15");
    let mut terminal = fake.terminal(&r);
    terminal["holder"] = json!("invented-holder");
    assert!(!field_contract(&terminal, &fields["Terminal"]));
    terminal.as_object_mut().unwrap().remove("holder");
    terminal.as_object_mut().unwrap().remove("screen");
    assert!(!field_contract(&terminal, &fields["Terminal"]));
}
#[test]
fn consumed_tui_objects_match_bundled_schema_field_contracts() {
    let (_dir, r, fake) = common::launch(Runtime::Tui);
    let fields = contracts("tui-0.9.5");
    let pane = fake.call(&r, &strings(&["pane", "get", "w1:p1"])).unwrap();
    assert!(field_contract(&pane["pane"], &fields["PaneInfo"]));
    let info = fake
        .call(&r, &strings(&["pane", "process-info", "--pane", "w1:p1"]))
        .unwrap();
    assert!(field_contract(
        &info["process_info"],
        &fields["PaneProcessInfo"]
    ));
    assert!(field_contract(
        &info["process_info"]["foreground_processes"][0],
        &fields["PaneProcessInfoProcess"]
    ));
    let workspaces = fake.call(&r, &strings(&["workspace", "list"])).unwrap();
    assert!(field_contract(
        &workspaces["workspaces"][0],
        &fields["WorkspaceInfo"]
    ));
}
