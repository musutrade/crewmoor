use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Tui,
    Core,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub team: String,
    pub role: String,
    pub runtime: Runtime,
    pub cwd: PathBuf,
    pub store: PathBuf,
    pub endpoint_dir: Option<PathBuf>,
    pub runtime_binary: PathBuf,
    pub runtime_version: String,
    pub agent_version: String,
    #[serde(default)]
    pub agent_args: Vec<String>,
}
pub fn name(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 24 {
        return Err("name must have 1..24 characters".into());
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("name must contain lowercase letters, digits or -".into());
    }
    Ok(())
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        name(&self.team)?;
        name(&self.role)?;
        absolute(&self.cwd)?;
        absolute(&self.store)?;
        validate_endpoint(self.endpoint_dir.as_deref())?;
        absolute(&self.runtime_binary)?;
        if !self.cwd.is_dir() {
            return Err("cwd is not a directory".into());
        }
        validate_args(&self.agent_args)
    }
    pub fn argv(&self) -> Vec<String> {
        let mut args = vec!["codex".into(), "--no-daemon".into()];
        args.extend(self.agent_args.clone());
        args
    }
}
fn validate_endpoint(path: Option<&std::path::Path>) -> Result<()> {
    if let Some(path) = path {
        absolute(path)?;
    }
    Ok(())
}
pub fn absolute(path: &std::path::Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("path must be absolute".into());
    }
    Ok(())
}
pub fn validate_args(args: &[String]) -> Result<()> {
    let mut values = args.iter();
    while let Some(arg) = values.next() {
        agent_arg(arg, &mut values)?;
    }
    Ok(())
}
fn agent_arg<'a>(arg: &str, values: &mut impl Iterator<Item = &'a String>) -> Result<()> {
    match arg {
        "--no-alt-screen" => Ok(()),
        "--model" | "-m" => model(values.next()),
        _ => Err("only interactive --no-alt-screen and --model are supported; prompts/subcommands are forbidden".into()),
    }
}
fn model(value: Option<&String>) -> Result<()> {
    let value = value.ok_or("missing model")?;
    if value.is_empty() || value.starts_with('-') {
        return Err("invalid model".into());
    }
    Ok(())
}
