//! Edits only marked CrewMoor handlers in the two native user-level JSON files.
use super::{event, model};
use crate::{Result, config::AgentKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

const MARKER: &str = ": crewmoor-feedback-v1; exec ";
const LABEL: &str = "CrewMoor feedback (v1)";

#[derive(Debug, Deserialize, Serialize)]
pub struct Plan {
    pub agent: AgentKind,
    pub action: String,
    pub path: PathBuf,
    pub registry: PathBuf,
    pub binary: PathBuf,
    pub before_exists: bool,
    pub original_mode: u32,
    pub changed: bool,
    pub removed: Value,
    pub added: Value,
}
pub fn user_path(agent: &AgentKind) -> Result<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME unavailable")?);
    let (variable, fallback, file) = match agent {
        AgentKind::Codex => ("CODEX_HOME", ".codex", "hooks.json"),
        AgentKind::Claude => ("CLAUDE_CONFIG_DIR", ".claude", "settings.json"),
    };
    let directory = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(fallback));
    crate::config::absolute(&directory)?;
    Ok(directory.join(file))
}
pub fn handler(agent: &AgentKind, registry: &Path, binary: &Path) -> Value {
    let command = format!(
        "{MARKER}{} feedback hook {} {}",
        quote(binary),
        agent.name(),
        quote(registry)
    );
    json!({"type":"command","command":command,"timeout":2,"statusMessage":LABEL})
}
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}
fn owned(value: &Value) -> bool {
    value["type"] == "command"
        && value["statusMessage"] == LABEL
        && value["command"]
            .as_str()
            .is_some_and(|cmd| cmd.starts_with(MARKER))
}
pub fn edit(document: &Value, agent: &AgentKind, action: &str, entry: &Value) -> Result<Value> {
    let mut result = document.clone();
    let object = result
        .as_object_mut()
        .ok_or("native settings must be an object")?;
    if action == "uninstall" && !object.contains_key("hooks") {
        return Ok(result);
    }
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("hooks must be an object")?;
    for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
        remove_owned(groups);
    }
    match action {
        "install" => append_handlers(hooks, agent, entry)?,
        "uninstall" => {}
        _ => return Err("choose install or uninstall".into()),
    }
    Ok(result)
}
fn append_handlers(
    hooks: &mut serde_json::Map<String, Value>,
    agent: &AgentKind,
    entry: &Value,
) -> Result<()> {
    for event in event::events(agent) {
        let groups = hooks
            .entry(*event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("hook event must be an array")?;
        groups.push(json!({"hooks":[entry]}));
    }
    Ok(())
}
fn remove_owned(groups: &mut Vec<Value>) {
    groups.retain_mut(|group| {
        let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = handlers.len();
        handlers.retain(|entry| !owned(entry));
        let removed = handlers.len() != before;
        !(removed && handlers.is_empty() && group.as_object().is_some_and(|g| g.len() == 1))
    });
}
fn own_entries(document: &Value) -> Value {
    let mut entries = Vec::new();
    if let Some(hooks) = document["hooks"].as_object() {
        for (event, groups) in hooks {
            for group in groups.as_array().into_iter().flatten() {
                for entry in group["hooks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|v| owned(v))
                {
                    entries.push(json!({"event":event,"handler":entry}));
                }
            }
        }
    }
    json!(entries)
}
pub fn preview(
    agent: AgentKind,
    action: &str,
    registry: &Path,
    binary: &Path,
    directory: &Path,
) -> Result<Plan> {
    validate_locations(&[registry, binary, directory])?;
    let path = user_path(&agent)?;
    let before = read_native(&path)?;
    let original = original_document(before.as_deref())?;
    let proposed = edit(
        &original,
        &agent,
        action,
        &handler(&agent, registry, binary),
    )?;
    let plan = Plan {
        agent,
        action: action.into(),
        path: path.clone(),
        registry: registry.into(),
        binary: binary.into(),
        before_exists: before.is_some(),
        original_mode: mode(&path)?,
        changed: original != proposed,
        removed: own_entries(&original),
        added: own_entries(&proposed),
    };
    save_preview(directory, before.as_deref(), &proposed, &plan)?;
    Ok(plan)
}
fn validate_locations(paths: &[&Path]) -> Result<()> {
    for path in paths {
        crate::config::absolute(path)?;
    }
    Ok(())
}
fn original_document(before: Option<&[u8]>) -> Result<Value> {
    match before {
        Some(bytes) => Ok(serde_json::from_slice(bytes)?),
        None => Ok(json!({})),
    }
}
fn save_preview(
    directory: &Path,
    before: Option<&[u8]>,
    proposed: &Value,
    plan: &Plan,
) -> Result<()> {
    // Review and raw backups are private: native settings may contain credentials.
    fs::DirBuilder::new().mode(0o700).create(directory)?;
    model::write_new(&directory.join("before.json"), before.unwrap_or_default())?;
    write_json(&directory.join("after.json"), proposed)?;
    write_json(&directory.join("plan.json"), plan)
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    model::write_new(path, &serde_json::to_vec_pretty(value)?)
}
use std::os::unix::fs::DirBuilderExt;
fn mode(path: &Path) -> Result<u32> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.mode() & 0o777),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0o600),
        Err(e) => Err(e.into()),
    }
}
fn read_native(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err("native config must be a regular file; left untouched".into());
            }
            Ok(Some(fs::read(path)?))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub fn apply(directory: &Path, rollback: bool) -> Result<Value> {
    let (plan, before, after) = read_plan(directory)?;
    let old = plan.before_exists.then_some(before.as_slice());
    let (expected, desired) = if rollback {
        (Some(after.as_slice()), old)
    } else {
        (old, Some(after.as_slice()))
    };
    let current = read_native(&plan.path)?;
    if !plan.changed || current.as_deref() == desired {
        return Ok(json!({"changed":false,"path":plan.path}));
    }
    if current.as_deref() != expected {
        return Err("native config changed since preview; no write".into());
    }
    replace(&plan, desired)?;
    Ok(
        json!({"changed":true,"path":plan.path,"rollback":rollback,"backup":directory.join("before.json")}),
    )
}
fn read_plan(directory: &Path) -> Result<(Plan, Vec<u8>, Vec<u8>)> {
    let plan: Plan = serde_json::from_slice(&fs::read(directory.join("plan.json"))?)?;
    if user_path(&plan.agent)? != plan.path {
        return Err("user config location changed".into());
    }
    let before = fs::read(directory.join("before.json"))?;
    let after = fs::read(directory.join("after.json"))?;
    verify_proposal(&plan, &before, &after)?;
    Ok((plan, before, after))
}
fn verify_proposal(plan: &Plan, before: &[u8], after: &[u8]) -> Result<()> {
    let original = if plan.before_exists {
        serde_json::from_slice(before)?
    } else {
        json!({})
    };
    let expected = edit(
        &original,
        &plan.agent,
        &plan.action,
        &handler(&plan.agent, &plan.registry, &plan.binary),
    )?;
    if serde_json::from_slice::<Value>(after)? != expected {
        return Err("proposal differs from owned-handler edit; no write".into());
    }
    Ok(())
}
fn replace(plan: &Plan, desired: Option<&[u8]>) -> Result<()> {
    let Some(bytes) = desired else {
        fs::remove_file(&plan.path)?;
        return Ok(());
    };
    let parent = plan.path.parent().ok_or("native config has no parent")?;
    model::private_dir(parent)?;
    let temporary = parent.join(format!(".crewmoor-hooks-{}", uuid::Uuid::new_v4().simple()));
    model::write_new(&temporary, bytes)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(plan.original_mode))?;
    fs::rename(temporary, &plan.path)?;
    Ok(())
}
