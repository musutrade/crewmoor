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

const RULES_ENV: &str = "TOOL_RULE_HOOK_RULES";
const CONTEXT_ENV: &str = "TOOL_RULE_HOOK_CONTEXT";
const VERIFY_ENV: &str = "TOOL_RULE_HOOK_VERIFY";

/// Only the exact ordered injection emitted by Guard::environment is recognized.
/// This is called after the fixed /usr/bin/env removal prefix has matched.
pub(crate) fn agent_arguments(argv: &[String]) -> Result<&[String]> {
    if !argv
        .first()
        .is_some_and(|value| value.starts_with(&format!("{RULES_ENV}=")))
    {
        return Ok(argv);
    }
    path_assignment(argv, 0, RULES_ENV)?;
    path_assignment(argv, 1, CONTEXT_ENV)?;
    let verifier = assignment(argv, 2, VERIFY_ENV)?;
    verify_arguments(verifier)?;
    Ok(&argv[3..])
}
fn assignment<'a>(argv: &'a [String], index: usize, name: &str) -> Result<&'a str> {
    argv.get(index)
        .and_then(|value| value.strip_prefix(&format!("{name}=")))
        .ok_or_else(|| "incomplete or reordered role tool-rule environment prefix".into())
}
fn path_assignment(argv: &[String], index: usize, name: &str) -> Result<()> {
    crate::config::absolute(Path::new(assignment(argv, index, name)?))
}
fn verify_arguments(value: &str) -> Result<()> {
    let argv: Vec<String> = serde_json::from_str(value)?;
    let [program, command, mode] = argv.as_slice() else {
        return Err("role tool-rule verifier argv differs".into());
    };
    if command != "tool-policy" || mode != "verify" {
        return Err("role tool-rule verifier argv differs".into());
    }
    crate::config::absolute(Path::new(program))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub snapshot: PathBuf,
    pub checker: PathBuf,
    #[serde(default)]
    pub rules: Option<PathBuf>,
    #[serde(default)]
    pub verifier: Option<PathBuf>,
    #[serde(default)]
    pub codex_args: Option<Vec<String>>,
    #[serde(default)]
    pub session_store: Option<PathBuf>,
    #[serde(default)]
    pub session_scope: Option<String>,
    pub settings: Option<PathBuf>,
    pub plugin: Option<PathBuf>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub registry: PathBuf,
    pub context: Context,
    #[serde(default)]
    pub policy: Option<Value>,
}
pub fn prepare(config: &Config, policy: &Policy, root: &Path, context: &Context) -> Result<Guard> {
    let policy = policy.resolve(&config.cwd);
    policy.validate_role(&config.agent, &config.agent_args, &config.cwd)?;
    let snapshot = write_context(config, root, context)?;
    let mut guard = Guard {
        snapshot,
        checker: policy.checker.clone(),
        rules: Some(policy.rules.clone()),
        verifier: Some(std::env::current_exe()?),
        codex_args: None,
        session_store: policy.session_store.clone(),
        session_scope: policy.session_scope.clone(),
        settings: None,
        plugin: None,
    };
    match config.agent {
        AgentKind::Claude => guard.claude(config, root)?,
        AgentKind::OpenCode => guard.prepare_opencode(&policy)?,
        AgentKind::Codex => {
            guard.codex_args = Some(super::codex::inject(&config.agent_args, &guard.command())?)
        }
    }
    Ok(guard)
}
fn write_context(config: &Config, root: &Path, context: &Context) -> Result<PathBuf> {
    let registry = config
        .feedback_registry
        .as_ref()
        .ok_or("role tool policy needs feedback_registry")?;
    let snapshot = root.join("tool-policy.json");
    let state = Snapshot {
        registry: registry.clone(),
        context: context.clone(),
        policy: None,
    };
    model::write_new(&snapshot, &serde_json::to_vec_pretty(&state)?)?;
    Ok(snapshot)
}
impl Guard {
    fn prepare_opencode(&mut self, policy: &Policy) -> Result<()> {
        self.plugin = Some(
            policy
                .opencode_plugin
                .clone()
                .ok_or("OpenCode role requires shared exported opencode_plugin")?,
        );
        self.session_store
            .as_ref()
            .ok_or("OpenCode role requires a trusted session_store")?;
        self.session_scope
            .as_ref()
            .ok_or("OpenCode role requires a trusted session_scope realm")?;
        Ok(())
    }
    pub(crate) fn bind_opencode(&self, config: &Config, info: &Value) -> Result<()> {
        let store = self.session_store.as_ref().ok_or("session store missing")?;
        let scope = self
            .session_scope
            .as_deref()
            .ok_or("session scope missing")?;
        let session = crate::native::text(info, "/id")?;
        let directory = crate::native::text(info, "/location/directory")?;
        if Path::new(&directory) != config.cwd {
            return Err("native session location differs from launch context".into());
        }
        let verifier = self.verifier.as_ref().ok_or("context verifier missing")?;
        let mut command = std::process::Command::new(&self.checker);
        command.arg("session-bind").arg(store);
        let result = super::hook::forward(
            command,
            &json!({"scope":scope,"session":session,"directory":directory,"rules":self.rules,"verifier":{"argv":[verifier,"tool-policy","verify"],"context":self.snapshot,"env":{"CREWMOOR_FEEDBACK_INSTANCE":config.feedback_instance}}}),
        )?;
        if result["revision"] != 1 {
            return Err("independent hook binding was not confirmed; do not retry".into());
        }
        Ok(())
    }

    pub(crate) fn validate(&self, config: &Config) -> Result<()> {
        for path in [&self.snapshot, &self.checker]
            .into_iter()
            .chain(self.rules.as_ref())
            .chain(self.verifier.as_ref())
            .chain(self.settings.as_ref())
            .chain(self.plugin.as_ref())
        {
            crate::config::absolute(path)?;
        }
        if self.rules.is_some() != self.verifier.is_some() {
            return Err("incomplete tool-rule-hook selection".into());
        }
        if config.feedback_instance.is_none() || config.feedback_registry.is_none() {
            return Err("tool policy requires an explicitly registered role instance".into());
        }
        self.validate_agent(&config.agent)
    }
    fn validate_agent(&self, agent: &AgentKind) -> Result<()> {
        if *agent == AgentKind::Claude && self.settings.is_none() {
            return Err("Claude role guard needs local settings".into());
        }
        if *agent == AgentKind::OpenCode && self.plugin.is_none() {
            return Err("OpenCode role guard needs its authorized local bridge".into());
        }
        Ok(())
    }
    fn command(&self) -> String {
        if self.rules.is_none() {
            return format!(
                "{} tool-policy hook {}",
                quote(&self.checker),
                quote(&self.snapshot)
            );
        }
        format!("{} hook", quote(&self.checker))
    }
    pub fn environment(&self) -> Vec<(String, String)> {
        let (Some(rules), Some(verifier)) = (&self.rules, &self.verifier) else {
            return Vec::new();
        };
        vec![
            (RULES_ENV.into(), rules.display().to_string()),
            (CONTEXT_ENV.into(), self.snapshot.display().to_string()),
            (
                VERIFY_ENV.into(),
                json!([verifier, "tool-policy", "verify"]).to_string(),
            ),
        ]
    }
    fn handler(&self, claude: bool) -> Value {
        let mut handler = json!({"type":"command","command":self.command(),"timeout":10});
        if claude {
            handler["onFailure"] = json!("block");
        }
        handler
    }
    fn claude(&mut self, config: &Config, root: &Path) -> Result<()> {
        let mut document = claude_document(&config.agent_args, &config.cwd, self.handler(true))?;
        self.claude_environment(&mut document)?;
        let path = root.join("tool-policy-claude-settings.json");
        model::write_new(&path, &serde_json::to_vec_pretty(&document)?)?;
        self.settings = Some(path);
        Ok(())
    }
    fn claude_environment(&self, document: &mut Value) -> Result<()> {
        let object = document
            .as_object_mut()
            .ok_or("Claude settings must be an object")?;
        let env = object
            .entry("env")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("Claude settings env must be an object")?;
        for (key, value) in self.environment() {
            env.insert(key, json!(value));
        }
        Ok(())
    }
    pub(crate) fn apply(&self, agent: &AgentKind, args: &mut Vec<String>) {
        match agent {
            AgentKind::Codex => {
                if let Some(prepared) = &self.codex_args {
                    *args = prepared.clone();
                    return;
                }
                if !args.iter().any(|arg| arg == "--no-daemon") {
                    args.push("--no-daemon".into());
                }
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
        let mut options =
            json!({"checker":self.checker,"store":self.session_store,"scope":self.session_scope});
        if self.rules.is_none() {
            options["snapshot"] = json!(self.snapshot);
        }
        plugins.push(json!({"package":plugin,"options":options}));
        Ok(content.to_string())
    }
}
pub(super) fn validate_claude(args: &[String], cwd: &Path) -> Result<()> {
    claude_document(args, cwd, json!({"type":"command","command":"fixed hook"})).map(|_| ())
}
fn claude_document(args: &[String], cwd: &Path, handler: Value) -> Result<Value> {
    let mut document = claude_settings(args, cwd)?;
    let object = document
        .as_object_mut()
        .ok_or("Claude --settings must be an object")?;
    validate_claude_environment(object.get("env"))?;
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
    entries.push(json!({"matcher":"^Bash$","hooks":[handler]}));
    Ok(document)
}
fn validate_claude_environment(value: Option<&Value>) -> Result<()> {
    if let Some(value) = value {
        let env = value
            .as_object()
            .ok_or("Claude settings env must be an object")?;
        if env.keys().any(|key| key.starts_with("TOOL_RULE_HOOK_")) {
            return Err(
                "Claude explicit settings env conflicts with role hook process environment".into(),
            );
        }
    }
    Ok(())
}
fn claude_settings(args: &[String], cwd: &Path) -> Result<Value> {
    if args
        .iter()
        .filter(|arg| arg.as_str() == "--settings")
        .count()
        > 1
    {
        return Err("role hook requires at most one Claude --settings source".into());
    }
    let Some(index) = args.iter().rposition(|arg| arg == "--settings") else {
        return Ok(json!({}));
    };
    let input = args
        .get(index + 1)
        .ok_or("Claude --settings value missing")?;
    let text = if input.trim_start().starts_with('{') {
        input.clone()
    } else {
        fs::read_to_string(cwd.join(input))?
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
            rules: Some("/owned/rules".into()),
            verifier: Some("/owned/crewmoor".into()),
            codex_args: None,
            session_store: Some("/owned/state".into()),
            session_scope: Some("owned-realm".into()),
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
    #[test]
    fn relative_claude_settings_use_the_role_startup_directory() {
        const CHILD: &str = "CM6465_ROLE_SETTINGS_PROBE_CHILD";
        if let Some(role) = std::env::var_os(CHILD) {
            relative_settings_probe(Path::new(&role));
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let role = root.path().join("role");
        let dispatcher = root.path().join("dispatcher");
        fs::create_dir(&role).unwrap();
        fs::create_dir(&dispatcher).unwrap();
        let role_text = "{\"env\":{\"SETTINGS_ORIGIN\":\"role\"}}";
        let dispatcher_text = "{\"env\":{\"SETTINGS_ORIGIN\":\"dispatcher\"}}";
        fs::write(role.join("native-settings.json"), role_text).unwrap();
        fs::write(dispatcher.join("native-settings.json"), dispatcher_text).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tool_policy::launch::tests::relative_claude_settings_use_the_role_startup_directory"])
            .env(CHILD, &role).current_dir(&dispatcher).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            fs::read_to_string(role.join("native-settings.json")).unwrap(),
            role_text
        );
        assert_eq!(
            fs::read_to_string(dispatcher.join("native-settings.json")).unwrap(),
            dispatcher_text
        );
    }
    fn relative_settings_probe(role: &Path) {
        use crate::feedback::{self, Evidence, Registration};
        let context = feedback::register(
            role,
            Registration {
                team: "policy-team".into(),
                role: "builder".into(),
                task: "settings-probe".into(),
                cwd: role.into(),
                agent: AgentKind::Claude,
                agent_version: "synthetic".into(),
                evidence: Evidence::Synthetic,
                native_session_id: Some("native-main".into()),
            },
        )
        .unwrap();
        let config: Config = serde_json::from_value(json!({
            "team":"policy-team","role":"builder","runtime":"tui","cwd":role,"store":role,
            "runtime_binary":"/bin/false","runtime_version":"synthetic","agent":"claude",
            "agent_executable":"/bin/false","agent_version":"synthetic","feedback_registry":role,
            "feedback_instance":context.instance,"agent_args":["--settings","native-settings.json"]
        }))
        .unwrap();
        let policy: Policy =
            serde_json::from_value(json!({"checker":"/bin/true","rules":role.join("rules.json")}))
                .unwrap();
        let guard = prepare(&config, &policy, role, &context).unwrap();
        let settings: Value =
            serde_json::from_slice(&fs::read(guard.settings.unwrap()).unwrap()).unwrap();
        assert_eq!(settings["env"]["SETTINGS_ORIGIN"], "role");
        let inline = vec![
            "--settings".into(),
            "{\"env\":{\"SETTINGS_ORIGIN\":\"inline\"}}".into(),
        ];
        assert_eq!(
            claude_settings(&inline, role).unwrap()["env"]["SETTINGS_ORIGIN"],
            "inline"
        );
    }
}
