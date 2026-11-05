//! OpenCode v2's session API; UI selection never replaces the bound session.
pub mod api;
pub mod binding;
pub mod endpoint;
mod events;
pub mod launch;
pub mod prepare;
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub agent: Option<String>,
    pub model: Option<Model>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl Options {
    pub fn check_session(&self, session: &Value) -> Result<()> {
        if let Some(agent) = &self.agent
            && session["agent"].as_str() != Some(agent)
        {
            return Err("native OpenCode agent profile differs".into());
        }
        if let Some(model) = &self.model
            && session["model"] != serde_json::to_value(model)?
        {
            return Err("native OpenCode model/variant differs".into());
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(agent) = &self.agent {
            crate::feedback::model::identifier(agent)?;
        }
        if let Some(model) = &self.model {
            model.validate()?;
        }
        Ok(())
    }

    /// Exact v2 session.create payload, not nonexistent root CLI model flags.
    pub fn session_payload(&self, cwd: &std::path::Path, role: &str) -> Value {
        let mut value = json!({"location":{"directory":cwd},"title":role});
        if let Some(agent) = &self.agent {
            value["agent"] = json!(agent);
        }
        if let Some(model) = &self.model {
            value["model"] = json!(model);
        }
        value
    }
}
impl Model {
    fn validate(&self) -> Result<()> {
        crate::feedback::model::identifier(&self.id)?;
        crate::feedback::model::identifier(&self.provider_id)?;
        if let Some(variant) = &self.variant {
            crate::feedback::model::identifier(variant)?;
        }
        Ok(())
    }
}
pub fn confirm_exit(record: &crate::record::Record) -> Result<()> {
    if record.config.agent != crate::config::AgentKind::OpenCode {
        return Ok(());
    }
    let registry = record
        .config
        .feedback_registry
        .as_ref()
        .ok_or("missing registry")?;
    let instance = record
        .config
        .feedback_instance
        .as_deref()
        .ok_or("missing instance")?;
    let context = crate::feedback::model::load(registry, instance)?;
    endpoint::load(registry, &context)?.confirm_exit()
}
