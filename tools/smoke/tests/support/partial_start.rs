//! Stand-in for `crewmoor team start CONFIG STATE` that fails after creating the team
//! directory, like a real start whose second role fails. Test use only.
#[derive(serde::Deserialize)]
struct TeamName {
    team: String,
}
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, team, start, config, state] = args.as_slice() else {
        return std::process::ExitCode::from(2);
    };
    if team != "team" || start != "start" {
        return std::process::ExitCode::from(2);
    }
    let text = std::fs::read_to_string(config).unwrap_or_default();
    let Ok(parsed) = toml::from_str::<TeamName>(&text) else {
        return std::process::ExitCode::from(3);
    };
    let dir = std::path::Path::new(state).join("teams").join(parsed.team);
    if std::fs::create_dir_all(&dir).is_err() {
        return std::process::ExitCode::from(4);
    }
    eprintln!("team {} role worker start failed: synthetic", dir.display());
    std::process::ExitCode::FAILURE
}
