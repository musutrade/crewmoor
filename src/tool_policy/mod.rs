//! Role selection and trusted identity adapter for the independent hook.
pub(crate) mod codex;
mod hook;
mod launch;
use crate::Result;
pub use hook::cli;
pub use launch::{Guard, prepare};
use serde::{Deserialize, Serialize};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub checker: PathBuf,
    pub rules: PathBuf,
    #[serde(default)]
    pub opencode_plugin: Option<PathBuf>,
    pub session_store: Option<PathBuf>,
    pub session_scope: Option<String>,
}
impl Policy {
    pub fn resolve(&self, cwd: &std::path::Path) -> Self {
        Self {
            checker: cwd.join(&self.checker),
            rules: cwd.join(&self.rules),
            opencode_plugin: self.opencode_plugin.as_ref().map(|path| cwd.join(path)),
            session_store: self.session_store.as_ref().map(|path| cwd.join(path)),
            session_scope: self.session_scope.clone(),
        }
    }
    pub(crate) fn validate_role(
        &self,
        kind: &crate::config::AgentKind,
        args: &[String],
        cwd: &std::path::Path,
    ) -> Result<()> {
        self.resolve(cwd).validate()?;
        crate::config::validate_args(kind, args)?;
        match kind {
            crate::config::AgentKind::Codex => codex::validate(args),
            crate::config::AgentKind::Claude => launch::validate_claude(args, cwd),
            crate::config::AgentKind::OpenCode => {
                if self.opencode_plugin.is_none()
                    || self.session_store.is_none()
                    || self.session_scope.is_none()
                {
                    return Err(
                        "OpenCode role needs opencode_plugin, session_store and session_scope"
                            .into(),
                    );
                }
                Ok(())
            }
        }
    }
    pub fn validate(&self) -> Result<()> {
        crate::config::absolute(&self.checker)?;
        crate::config::absolute(&self.rules)?;
        self.validate_selection()?;
        self.validate_checker()?;
        self.validate_rules()
    }
    fn validate_selection(&self) -> Result<()> {
        if let Some(plugin) = &self.opencode_plugin {
            crate::config::absolute(plugin)?;
            if !plugin.join("index.ts").is_file() {
                return Err("export the independent OpenCode bridge before selecting it".into());
            }
        }
        if let Some(store) = &self.session_store {
            crate::config::absolute(store)?;
        }
        if let Some(scope) = &self.session_scope {
            crate::feedback::model::identifier(scope)?;
        }
        Ok(())
    }
    fn validate_checker(&self) -> Result<()> {
        let metadata = std::fs::metadata(&self.checker)?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err("tool-rule-hook checker must be an executable file".into());
        }
        Ok(())
    }
    fn validate_rules(&self) -> Result<()> {
        let mut command = std::process::Command::new(&self.checker);
        command.args(["validate"]).arg(&self.rules);
        let output = crate::transport::output(&mut command, std::time::Duration::from_secs(6))?;
        if !output.status.success() {
            return Err(format!(
                "tool-rule-hook rules validation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(())
    }
}
