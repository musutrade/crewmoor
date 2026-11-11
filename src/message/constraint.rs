use super::Binding;
use crate::{Result, team::Team};
use std::{fs, path::Path};

pub(crate) fn validate(content: Option<&str>) -> Result<()> {
    if content.is_some_and(|text| !text.trim().is_empty() && text.len() > 16_384) {
        return Err("message constraint must have at most 16384 UTF-8 bytes when nonblank".into());
    }
    Ok(())
}

/// Read exactly one whole running configuration per submission; never use a legacy snapshot.
pub(super) fn recipient(binding: &Binding, role: &str) -> Result<String> {
    let path = binding.config.team_config.as_deref().ok_or(
        "binding has no running team configuration source; use crewmoor message source BINDING TEAM_DIR once (no rebind)",
    )?;
    let team = load(binding, path)?;
    let content = team.role(role)?.message_constraint.as_deref();
    validate(content)?;
    Ok(content
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("")
        .into())
}

/// Check the source against the fixed pair. Configuration never supplies new routing identities.
pub(super) fn load(binding: &Binding, path: &Path) -> Result<Team> {
    crate::config::absolute(path)?;
    let team: Team = serde_json::from_slice(&fs::read(path).map_err(|e| {
        format!(
            "cannot read running team configuration {}: {e}",
            path.display()
        )
    })?)?;
    if team.team != binding.team
        || team.dispatcher != binding.dispatcher.context.registration.role
        || team.runtime != binding.config.runtime
        || team.feedback_registry != binding.config.feedback_registry
    {
        return Err("running team configuration does not match binding".into());
    }
    for member in [&binding.dispatcher, &binding.worker] {
        let role = &member.context.registration.role;
        if team.roles.iter().filter(|r| &r.role == role).count() != 1 {
            return Err(
                format!("running team configuration must contain bound role {role} once").into(),
            );
        }
    }
    Ok(team)
}
