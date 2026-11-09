use serde_json::json;
use std::{fs, path::Path};
/// Adapter simulation: the Rust transport fixture acknowledges validation; no scanner/host is run.
pub fn policy(root: &Path) -> crewmoor::tool_policy::Policy {
    let rules = root.join("external-rules.json");
    fs::write(&rules, "{}").unwrap();
    let plugin = root.join("shared-rule-plugin");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(
        plugin.join("index.ts"),
        include_bytes!("../../hooks/tool-rule-hook/assets/opencode/index.ts"),
    )
    .unwrap();
    serde_json::from_value(json!({"checker":env!("CARGO_BIN_EXE_transport-fixture"),"rules":rules,"opencode_plugin":plugin,"session_store":root.join("owned-rule-state"),"session_scope":"test-realm"}))
        .unwrap()
}
