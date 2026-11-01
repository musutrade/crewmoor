use crate::{Result, config::AgentKind};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Evidence {
    Synthetic,
    Native,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub team: String,
    pub role: String,
    pub task: String,
    pub cwd: PathBuf,
    pub agent: AgentKind,
    pub agent_version: String,
    pub evidence: Evidence,
    pub native_session_id: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Context {
    pub instance: String,
    pub registration: Registration,
}
impl Registration {
    fn validate(&mut self) -> Result<()> {
        crate::config::name(&self.team)?;
        crate::config::name(&self.role)?;
        identifier(&self.task)?;
        identifier(&self.agent_version)?;
        crate::config::absolute(&self.cwd)?;
        self.cwd = self.cwd.canonicalize()?;
        if let Some(session) = &self.native_session_id {
            identifier(session)?;
        }
        Ok(())
    }
}
pub fn identifier(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err("invalid feedback identifier".into());
    }
    Ok(())
}
pub fn private_dir(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    Ok(())
}
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}
pub fn register(registry: &Path, mut registration: Registration) -> Result<Context> {
    crate::config::absolute(registry)?;
    registration.validate()?;
    let context = Context {
        instance: uuid::Uuid::new_v4().simple().to_string(),
        registration,
    };
    let directory = instance_dir(registry, &context.instance)?;
    private_dir(&directory)?;
    write_new(
        &directory.join("context.json"),
        &serde_json::to_vec_pretty(&context)?,
    )?;
    Ok(context)
}
pub fn instance_dir(registry: &Path, instance: &str) -> Result<PathBuf> {
    if uuid::Uuid::parse_str(instance)?.simple().to_string() != instance {
        return Err("invalid CrewMoor instance identifier".into());
    }
    Ok(registry.join("instances").join(instance))
}
pub fn load(registry: &Path, instance: &str) -> Result<Context> {
    let path = instance_dir(registry, instance)?.join("context.json");
    let context: Context = serde_json::from_slice(&fs::read(path)?)?;
    if context.instance != instance {
        return Err("feedback context changed".into());
    }
    Ok(context)
}
