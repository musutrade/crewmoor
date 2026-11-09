use super::Policy;
use crate::{
    Result,
    config::{AgentKind, Config},
    feedback::{Context, model},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
const OPENCODE_BRIDGE: &[u8] = include_bytes!("../../assets/opencode-tool-policy/index.ts");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub snapshot: PathBuf,
    pub checker: PathBuf,
    pub settings: Option<PathBuf>,
    pub plugin: Option<PathBuf>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub registry: PathBuf,
    pub context: Context,
    pub policy: Policy,
}
pub fn prepare(config: &Config, policy: &Policy, root: &Path, context: &Context) -> Result<Guard> {
    policy.validate()?;
    let registry = config
        .feedback_registry
        .as_ref()
        .ok_or("role tool policy needs feedback_registry")?;
    let snapshot = root.join("tool-policy.json");
    let state = Snapshot {
        registry: registry.clone(),
        context: context.clone(),
        policy: policy.clone(),
    };
    model::write_new(&snapshot, &serde_json::to_vec_pretty(&state)?)?;
    let mut guard = Guard {
        snapshot,
        checker: policy.checker.clone(),
        settings: None,
        plugin: None,
    };
    match config.agent {
        AgentKind::Claude => guard.claude(config, root)?,
        AgentKind::OpenCode => guard.opencode(root)?,
        AgentKind::Codex => {}
    }
    Ok(guard)
}
impl Guard {
    pub(crate) fn validate(&self, config: &Config) -> Result<()> {
        for path in [&self.snapshot, &self.checker]
            .into_iter()
            .chain(self.settings.as_ref())
            .chain(self.plugin.as_ref())
        {
            crate::config::absolute(path)?;
        }
        if config.feedback_instance.is_none() || config.feedback_registry.is_none() {
            return Err("tool policy requires an explicitly registered role instance".into());
        }
        if config.agent == AgentKind::Claude && self.settings.is_none() {
            return Err("Claude role guard needs local settings".into());
        }
        if config.agent == AgentKind::OpenCode && self.plugin.is_none() {
            return Err("OpenCode role guard needs its authorized local bridge".into());
        }
        Ok(())
    }
    fn command(&self) -> String {
        format!(
            "{} tool-policy hook {}",
            quote(&self.checker),
            quote(&self.snapshot)
        )
    }
    fn handler(&self, claude: bool) -> Value {
        let mut handler = json!({"type":"command","command":self.command(),"timeout":10});
        if claude {
            handler["onFailure"] = json!("block");
        }
        handler
    }
    fn claude(&mut self, config: &Config, root: &Path) -> Result<()> {
        let mut document = claude_settings(&config.agent_args)?;
        let object = document
            .as_object_mut()
            .ok_or("Claude --settings must be an object")?;
        let hooks = object
            .entry("hooks")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("Claude hooks must be an object")?;
        let entries = hooks
            .entry("PreToolUse")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Claude PreToolUse must be an array")?;
        entries.push(json!({"matcher":"^Bash$","hooks":[self.handler(true)]}));
        let path = root.join("tool-policy-claude-settings.json");
        model::write_new(&path, &serde_json::to_vec_pretty(&document)?)?;
        self.settings = Some(path);
        Ok(())
    }
    fn opencode(&mut self, root: &Path) -> Result<()> {
        let directory = root.join("tool-policy-opencode");
        model::private_dir(&directory)?;
        model::write_new(&directory.join("index.ts"), OPENCODE_BRIDGE)?;
        self.plugin = Some(directory);
        Ok(())
    }
    pub(crate) fn apply(&self, agent: &AgentKind, args: &mut Vec<String>) {
        match agent {
            AgentKind::Codex => {
                let command = json!(self.command());
                args.extend(["-c".into(), "features.hooks=true".into(), "-c".into(), format!("hooks.PreToolUse=[{{matcher=\"^Bash$\",hooks=[{{type=\"command\",command={command},timeout=10}}]}}]")]);
            }
            AgentKind::Claude => {
                remove_settings(args);
                if let Some(path) = &self.settings {
                    args.extend(["--settings".into(), path.display().to_string()]);
                }
            }
            AgentKind::OpenCode => {}
        }
    }
    pub(crate) fn opencode_content(&self, existing: Option<&str>) -> Result<String> {
        let mut content: Value = serde_json::from_str(existing.unwrap_or("{}"))?;
        let object = content
            .as_object_mut()
            .ok_or("OPENCODE_CONFIG_CONTENT must be an object")?;
        let plugins = object
            .entry("plugins")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("OpenCode plugins must be an array")?;
        let plugin = self.plugin.as_ref().ok_or("OpenCode bridge missing")?;
        plugins.push(
            json!({"package":plugin,"options":{"checker":self.checker,"snapshot":self.snapshot}}),
        );
        Ok(content.to_string())
    }
}
fn claude_settings(args: &[String]) -> Result<Value> {
    let Some(index) = args.iter().rposition(|arg| arg == "--settings") else {
        return Ok(json!({}));
    };
    let input = args
        .get(index + 1)
        .ok_or("Claude --settings value missing")?;
    let text = if input.trim_start().starts_with('{') {
        input.clone()
    } else {
        fs::read_to_string(input)?
    };
    Ok(serde_json::from_str(&text)?)
}
fn remove_settings(args: &mut Vec<String>) {
    while let Some(index) = args.iter().position(|arg| arg == "--settings") {
        args.drain(index..index + 2);
    }
}
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opencode_overlay_preserves_existing_configuration_and_requires_a_plugin_list() {
        let guard = Guard {
            snapshot: "/owned/policy.json".into(),
            checker: "/owned/checker".into(),
            settings: None,
            plugin: Some("/owned/plugin".into()),
        };
        let merged: Value = serde_json::from_str(
            &guard
                .opencode_content(Some(
                    "{\"plugins\":[\"existing\"],\"example\":\"preserved\"}",
                ))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(merged["plugins"][0], "existing");
        assert_eq!(merged["plugins"][1]["package"], "/owned/plugin");
        assert_eq!(merged["example"], "preserved");
        assert!(guard.opencode_content(Some("{\"plugins\":false}")).is_err());
        assert!(guard.opencode_content(Some("invalid")).is_err());
    }
}
