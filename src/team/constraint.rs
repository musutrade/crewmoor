use super::Team;
use crate::Result;
use serde_json::{Value, json};
use std::{fs, path::Path};

/// Change only one role's constraint. Readers see a complete old or new configuration.
pub fn update_constraint(dir: &Path, role: &str, content: &str) -> Result<()> {
    crate::config::absolute(dir)?;
    let mut document: Value = serde_json::from_slice(&fs::read(dir.join("team.json"))?)?;
    let team: Team = serde_json::from_value(document.clone())?;
    let index = team
        .roles
        .iter()
        .position(|r| r.role == role)
        .ok_or("role is not in this team")?;
    if team.roles.iter().filter(|r| r.role == role).count() != 1 {
        return Err("constraint recipient role is ambiguous".into());
    }
    crate::message::validate_constraint(Some(content))?;
    document["roles"][index]["message_constraint"] = json!(content);
    crate::issue::replace(dir, "team.json", &serde_json::to_vec_pretty(&document)?)
}

pub(super) fn cli(args: &[String]) -> Result<Value> {
    let [dir, role, text] = args else {
        return Err(super::USAGE.into());
    };
    update_constraint(Path::new(dir), role, &fs::read_to_string(text)?)?;
    Ok(json!({"team_config":Path::new(dir).join("team.json"),"role":role,"saved":true}))
}
