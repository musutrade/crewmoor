use crate::{
    Result,
    config::{AgentKind, Config},
    feedback::model,
    record::Record,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};
pub const LAUNCH_ENV: &str = "CREWMOOR_OPENCODE_LAUNCH";

#[derive(Serialize, Deserialize)]
pub struct Launch {
    pub config: Config,
    pub root: PathBuf,
    environment: BTreeMap<String, Option<String>>,
}
impl Launch {
    fn capture(record: &Record) -> Result<Self> {
        let mut environment = BTreeMap::new();
        for key in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            "OPENCODE_DB",
        ] {
            let value = std::env::var_os(key)
                .map(|v| v.into_string().map_err(|_| "non-UTF8 child environment"))
                .transpose()?;
            environment.insert(key.into(), value);
        }
        Ok(Self {
            config: record.config.clone(),
            root: record.root.clone(),
            environment,
        })
    }
    pub fn command(&self) -> Result<Command> {
        let mut command = Command::new(&self.config.agent_executable);
        for (key, value) in &self.environment {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
        if let Some(path) = native_path(&self.root)? {
            command.env("PATH", path);
        }
        if let Some(guard) = &self.config.tool_guard {
            // Merge native configuration in memory; do not persist a copy of credentials.
            let existing = std::env::var_os("OPENCODE_CONFIG_CONTENT")
                .map(|value| {
                    value
                        .into_string()
                        .map_err(|_| "non-UTF8 OpenCode config content")
                })
                .transpose()?;
            command.env(
                "OPENCODE_CONFIG_CONTENT",
                guard.opencode_content(existing.as_deref())?,
            );
        }
        Ok(command)
    }
}
fn native_path(root: &Path) -> Result<Option<std::ffi::OsString>> {
    let entry = root.join("opencode-bin");
    Ok(std::env::var_os("PATH")
        .map(|path| std::env::join_paths(std::env::split_paths(&path).filter(|p| p != &entry)))
        .transpose()?)
}

pub fn prepare(record: &Record) -> Result<()> {
    if record.config.agent != AgentKind::OpenCode {
        return Ok(());
    }
    let config = &record.config;
    if !config
        .agent_version
        .trim_start_matches("opencode ")
        .trim_start_matches('v')
        .starts_with("2.")
    {
        return Err("only OpenCode v2 is supported".into());
    }
    let registry = config
        .feedback_registry
        .as_ref()
        .ok_or("OpenCode needs feedback_registry")?;
    let instance = config
        .feedback_instance
        .as_deref()
        .ok_or("OpenCode needs an explicitly registered instance")?;
    model::load_for_agent(registry, instance, &AgentKind::OpenCode)?;
    prepare_files(record)
}
fn prepare_files(record: &Record) -> Result<()> {
    model::private_dir(&record.root.join("opencode-bin"))?;
    symlink(std::env::current_exe()?, alias(record))?;
    model::write_new(
        &launch_file(record),
        &serde_json::to_vec(&Launch::capture(record)?)?,
    )
}
pub fn alias(record: &Record) -> PathBuf {
    record.root.join("opencode-bin/opencode")
}
pub fn launch_file(record: &Record) -> PathBuf {
    record.root.join("opencode-launch.json")
}

pub fn arguments(config: &Config) -> Vec<String> {
    let mut args = config.agent_args.clone();
    if let Some(prompt) = &config.initial_prompt {
        args.extend(["--prompt".into(), prompt.clone()]);
    }
    args
}
pub fn tui_arguments(config: &Config) -> Vec<String> {
    let mut args = vec!["--standalone".into()];
    args.extend(config.agent_args.clone());
    args
}
pub fn core_argv(record: &Record) -> Vec<String> {
    let mut argv = crate::app::child_prefix(true);
    argv.push(format!("{LAUNCH_ENV}={}", launch_file(record).display()));
    if let Some(instance) = &record.config.feedback_instance {
        argv.push(format!("{}={instance}", crate::feedback::INSTANCE_ENV));
    }
    argv.push(alias(record).display().to_string());
    argv.extend(arguments(&record.config));
    argv
}
pub fn workspace_env(record: &Record, args: &mut Vec<String>) {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let value = format!(
        "PATH={}:{}",
        record.root.join("opencode-bin").display(),
        path.to_string_lossy()
    );
    args.extend([
        "--env".into(),
        value,
        "--env".into(),
        format!("{LAUNCH_ENV}={}", launch_file(record).display()),
        "--env".into(),
        format!(
            "OPENCODE_DB={}",
            record.root.join("opencode.sqlite").display()
        ),
    ]);
}
/// One owned-shell startup setting after user rc files, before native agent start.
pub fn tui_path(record: &Record, shell: &Path) -> Result<String> {
    match shell.file_name().and_then(|value| value.to_str()) {
        Some("bash" | "dash" | "sh" | "zsh" | "ksh") => {}
        _ => return Err("OpenCode TUI entry requires a known POSIX startup shell".into()),
    }
    let directory = record.root.join("opencode-bin");
    let directory = directory.to_str().ok_or("non-UTF8 OpenCode entry")?;
    if directory.chars().any(char::is_control) {
        return Err("control character in OpenCode entry path".into());
    }
    let quoted = directory.replace('\'', "'\\''");
    Ok(format!("export PATH='{quoted}':\"$PATH\""))
}
pub fn read() -> Result<Launch> {
    let path = std::env::var_os(LAUNCH_ENV).ok_or("missing owned OpenCode launch file")?;
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
