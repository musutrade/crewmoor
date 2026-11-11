use crewmoor::{config::Runtime, message::Config};
use serde_json::json;

/// A schema-valid running configuration, independent of synthetic container state.
pub fn configure(config: &mut Config, team: &str, dispatcher: &str, worker: &str) {
    let path = config.feedback_registry.join("team.json");
    let roles: Vec<_> = [dispatcher, worker]
        .iter()
        .map(|role| {
            json!({
                "role":role,"cwd":config.feedback_registry,"agent":"codex",
                "agent_executable":"/bin/false","agent_version":"synthetic"
            })
        })
        .collect();
    let mut document = json!({
        "team":team,"task":"synthetic","runtime":config.runtime,
        "runtime_binary":config.runtime_binary,"runtime_version":"synthetic",
        "feedback_registry":config.feedback_registry,"evidence":"synthetic",
        "dispatcher":dispatcher,"roles":roles
    });
    if config.runtime == Runtime::Tui {
        document["host"] = json!({"socket":config.socket,"server_version":"synthetic"});
    } else {
        document["app_target"] = json!("/unused/app.toml");
    }
    // Ensure the fixture uses the production schema, without invoking native launch validation.
    let _: crewmoor::team::Team = serde_json::from_value(document.clone()).unwrap();
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    config.team_config = Some(path);
}
