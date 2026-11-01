use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Tui,
    Core,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Codex,
    Claude,
}
impl AgentKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
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
    pub agent: AgentKind,
    pub agent_executable: PathBuf,
    pub agent_version: String,
    #[serde(default)]
    pub agent_args: Vec<String>,
    pub app: Option<crate::app::Binding>,
    pub app_target: Option<PathBuf>,
    pub host: Option<Host>,
    pub feedback_instance: Option<String>,
    pub initial_prompt: Option<String>,
}
/// An existing TUI server that CrewMoor connects to but never owns or stops.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Host {
    pub socket: PathBuf,
    pub server_version: String,
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
    pub fn resolve_target(&mut self) -> Result<()> {
        let Some(path) = self.app_target.take() else {
            return Ok(());
        };
        if self.app.is_some() {
            return Err("choose app or app_target, not both".into());
        }
        absolute(&path)?;
        self.app = Some(toml::from_str(&std::fs::read_to_string(path)?)?);
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        name(&self.team)?;
        name(&self.role)?;
        self.validate_paths()?;
        if let Some(app) = &self.app {
            app.validate(&self.runtime)?;
        }
        if self.runtime == Runtime::Tui {
            self.validate_tui()?;
        }
        self.validate_options()
    }
    fn validate_options(&self) -> Result<()> {
        self.validate_host()?;
        self.validate_marker()?;
        validate_args(&self.agent, &self.agent_args)?;
        initial_prompt(&self.agent, self.initial_prompt.as_deref())
    }
    fn validate_host(&self) -> Result<()> {
        let Some(host) = &self.host else {
            return Ok(());
        };
        if self.runtime != Runtime::Tui {
            return Err("external host mode requires tui".into());
        }
        absolute(&host.socket)?;
        crate::feedback::model::identifier(&host.server_version)
    }
    fn validate_marker(&self) -> Result<()> {
        let Some(instance) = &self.feedback_instance else {
            return Ok(());
        };
        if self.runtime == Runtime::Core && self.app.is_none() {
            return Err("core feedback marker needs an existing App/core target".into());
        }
        crate::feedback::model::instance_dir(std::path::Path::new("/"), instance).map(|_| ())
    }
    fn validate_paths(&self) -> Result<()> {
        absolute(&self.cwd)?;
        absolute(&self.store)?;
        validate_endpoint(self.endpoint_dir.as_deref())?;
        absolute(&self.runtime_binary)?;
        absolute(&self.agent_executable)?;
        if !self.cwd.is_dir() {
            return Err("cwd is not a directory".into());
        }
        Ok(())
    }
    pub fn argv(&self) -> Vec<String> {
        let mut args = vec![self.agent_executable.display().to_string()];
        if self.runtime == Runtime::Core
            && self.agent == AgentKind::Codex
            && !self.agent_args.iter().any(|a| a == "--no-daemon")
        {
            args.push("--no-daemon".into());
        }
        args.extend(self.agent_args.clone());
        args.extend(self.initial_prompt.clone());
        args
    }
    fn validate_tui(&self) -> Result<()> {
        if !self.role.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Err("official agent name must start with a lowercase letter".into());
        }
        if self
            .agent_args
            .iter()
            .any(|arg| arg.chars().any(char::is_control))
        {
            return Err("official agent start rejects control bytes in arguments".into());
        }
        Ok(())
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
pub fn validate_args(kind: &AgentKind, args: &[String]) -> Result<()> {
    let mut values = args.iter();
    while let Some(arg) = values.next() {
        agent_arg(kind, arg, &mut values)?;
    }
    Ok(())
}
fn agent_arg<'a>(
    kind: &AgentKind,
    arg: &str,
    values: &mut impl Iterator<Item = &'a String>,
) -> Result<()> {
    match arg {
        "--no-alt-screen" | "--no-daemon" if *kind == AgentKind::Codex => Ok(()),
        "--model" => model(values.next()),
        "-m" if *kind == AgentKind::Codex => model(values.next()),
        _ => Err(
            "unsupported argument for this Agent kind; prompts/subcommands are forbidden".into(),
        ),
    }
}
fn model(value: Option<&String>) -> Result<()> {
    let value = value.ok_or("missing model")?;
    if value.is_empty() || value.starts_with('-') || value.contains('\0') {
        return Err("invalid model".into());
    }
    Ok(())
}
/// Codex's native `codex [PROMPT]`: exactly one startup task; Claude takes none.
pub fn initial_prompt(kind: &AgentKind, prompt: Option<&str>) -> Result<()> {
    let Some(prompt) = prompt else {
        return Ok(());
    };
    if *kind != AgentKind::Codex {
        return Err("initial prompt is only accepted for Codex".into());
    }
    if prompt.trim().is_empty()
        || prompt.starts_with('-')
        || prompt.len() > 4096
        || prompt.chars().any(char::is_control)
    {
        return Err("initial prompt must be 1..4096 bytes and not an option".into());
    }
    Ok(())
}
