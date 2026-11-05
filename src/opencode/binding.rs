use super::{
    api::PromptReceipt,
    endpoint::{self, Endpoint},
};
use crate::{
    Result,
    config::AgentKind,
    feedback::Context,
    message::{Config, Member},
    process::Identity,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    pub instance: String,
    pub session_id: String,
    pub url: String,
    pub version: String,
    pub launcher: Identity,
    pub server: Identity,
    pub tui: Identity,
}
impl From<&Endpoint> for Bound {
    fn from(endpoint: &Endpoint) -> Self {
        Self {
            instance: endpoint.instance.clone(),
            session_id: endpoint.session_id.clone(),
            url: endpoint.url.clone(),
            version: endpoint.version.clone(),
            launcher: endpoint.launcher.clone(),
            server: endpoint.server.clone(),
            tui: endpoint.tui.clone(),
        }
    }
}
pub fn capture(config: &Config, context: &Context) -> Result<Option<Bound>> {
    if context.registration.agent != AgentKind::OpenCode {
        return Ok(None);
    }
    let endpoint = endpoint::load(&config.feedback_registry, context)?;
    endpoint.verify(Duration::from_secs(10))?;
    Ok(Some(Bound::from(&endpoint)))
}
pub fn check(config: &Config, member: &Member) -> Result<()> {
    if member.context.registration.agent != AgentKind::OpenCode {
        return Ok(());
    }
    let endpoint = matched(config, member)?;
    endpoint.verify(Duration::from_secs(10))?;
    Ok(())
}
fn matched(config: &Config, member: &Member) -> Result<Endpoint> {
    let expected = member
        .opencode
        .as_ref()
        .ok_or("missing bound OpenCode API identity")?;
    let endpoint = endpoint::load(&config.feedback_registry, &member.context)?;
    if &Bound::from(&endpoint) != expected
        || member.native_session.as_deref() != Some(&endpoint.session_id)
    {
        return Err("bound OpenCode endpoint/session identity changed; refusing adoption".into());
    }
    Ok(endpoint)
}
pub fn prompt(
    config: &Config,
    member: &Member,
    text: &str,
    timeout: Duration,
) -> Result<PromptReceipt> {
    let endpoint = matched(config, member)?;
    endpoint.api(timeout)?.prompt(&endpoint.session_id, text)
}
