//! Persistent ownership of App placement IDs, keyed by core identity and canonical cwd.
use crate::{
    Result,
    app::Binding,
    record::{Lock, Record},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: String,
    pub core_id: String,
    pub cwd: PathBuf,
    pub space_id: String,
    pub tab_id: String,
}
pub fn prepare(r: &mut Record, app: &Binding, doc: &Value) -> Result<()> {
    let project = resolve(&r.config.store, &app.core_id, &r.config.cwd, doc)?;
    let binding = r.config.app.as_mut().ok_or("missing App target")?;
    binding.space_id = Some(project.space_id.clone());
    binding.tab_id = Some(project.tab_id.clone());
    r.project = Some(project);
    r.save()
}
pub fn resolve(store: &Path, core_id: &str, cwd: &Path, doc: &Value) -> Result<Project> {
    let directory = store.join("projects");
    fs::create_dir_all(&directory)?;
    let _lock = Lock::take(&directory)?;
    let path = directory.join("index.json");
    choose(&path, core_id, cwd, doc)
}
fn choose(path: &Path, core_id: &str, cwd: &Path, doc: &Value) -> Result<Project> {
    layout_spaces(doc)?;
    let mut projects = read(path)?;
    let cwd = cwd.canonicalize()?;
    if let Some(project) = find(&projects, core_id, &cwd)? {
        validate(&project)?;
        return Ok(project);
    }
    let project = new_project(core_id, cwd);
    ensure_unused(&project, doc)?;
    projects.push(project.clone());
    save(path, &projects)?;
    Ok(project)
}
fn new_project(core_id: &str, cwd: PathBuf) -> Project {
    Project {
        id: uuid::Uuid::new_v4().simple().to_string(),
        core_id: core_id.into(),
        cwd,
        space_id: format!("sp{}", uuid::Uuid::new_v4().simple()),
        tab_id: format!("tb{}", uuid::Uuid::new_v4().simple()),
    }
}
pub fn read(path: &Path) -> Result<Vec<Project>> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}
fn find(projects: &[Project], core_id: &str, cwd: &Path) -> Result<Option<Project>> {
    let mut matching = projects
        .iter()
        .filter(|p| p.core_id == core_id && p.cwd == cwd);
    let first = matching.next().cloned();
    if matching.next().is_some() {
        return Err("duplicate project ownership records; request withheld".into());
    }
    Ok(first)
}
fn validate(project: &Project) -> Result<()> {
    crate::app::component(&project.id)?;
    crate::app::component(&project.space_id)?;
    crate::app::component(&project.tab_id)?;
    crate::config::absolute(&project.cwd)
}
pub fn ensure_unused(project: &Project, doc: &Value) -> Result<()> {
    for space in layout_spaces(doc)? {
        if space["id"].as_str() == Some(&project.space_id) {
            return Err("new project Space ID collision".into());
        }
        let tabs = array_or_empty(space.get("tabs"))?;
        if tabs
            .iter()
            .any(|t| t["id"].as_str() == Some(&project.tab_id))
        {
            return Err("new project Tab ID collision".into());
        }
    }
    Ok(())
}
fn layout_spaces(doc: &Value) -> Result<&[Value]> {
    match &doc["value"] {
        Value::Null if doc["version"] == 0 => Ok(&[]),
        Value::Object(value) => array_or_empty(value.get("spaces")),
        _ => Err("App document unavailable or invalid; request withheld".into()),
    }
}
fn array_or_empty(value: Option<&Value>) -> Result<&[Value]> {
    match value {
        None => Ok(&[]),
        Some(Value::Array(list)) => Ok(list),
        _ => Err("invalid App layout collection".into()),
    }
}
fn save(path: &Path, projects: &[Project]) -> Result<()> {
    let temporary = path.with_extension("new");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(projects)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}
