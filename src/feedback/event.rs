use super::{
    journal::{self, Event},
    model,
};
use crate::{Result, config::AgentKind};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Input {
    session_id: String,
    cwd: PathBuf,
    hook_event_name: String,
    turn_id: Option<String>,
    source: Option<String>,
    reason: Option<String>,
    tool_name: Option<String>,
    notification_type: Option<String>,
    error: Option<String>,
    stop_hook_active: Option<bool>,
    agent_id: Option<String>,
}
pub fn events(agent: &AgentKind) -> &'static [&'static str] {
    match agent {
        AgentKind::Claude => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "SessionEnd",
            "StopFailure",
            "Notification",
            "PermissionRequest",
            "PostToolUseFailure",
        ],
        AgentKind::Codex => &[
            "SessionStart",
            "UserPromptSubmit",
            "Stop",
            "SessionEnd",
            "Interrupt",
            "PermissionRequest",
        ],
    }
}
pub fn ingest(registry: &Path, instance: &str, agent: &AgentKind, bytes: &[u8]) -> Result<()> {
    let context = model::load(registry, instance)?;
    let input = checked_input(&context, agent, bytes)?;
    let native = metadata(&input)?;
    let enroll = input.hook_event_name == "SessionStart";
    journal::append(
        registry,
        &context,
        Event {
            received_ms: journal::now()?,
            source: "native-hook".into(),
            event: input.hook_event_name,
            native_session_id: input.session_id,
            native,
            summary: None,
        },
        enroll,
    )
}
fn checked_input(context: &super::Context, agent: &AgentKind, bytes: &[u8]) -> Result<Input> {
    let input: Input = serde_json::from_slice(bytes)?;
    if context.registration.agent != *agent || input.cwd.canonicalize()? != context.registration.cwd
    {
        return Err("Agent/cwd does not match registered CrewMoor instance".into());
    }
    if !events(agent).contains(&input.hook_event_name.as_str()) {
        return Err("uncollected native event".into());
    }
    let enroll = input.hook_event_name == "SessionStart";
    if enroll && input.agent_id.is_some() {
        return Err("subagent cannot establish main instance identity".into());
    }
    Ok(input)
}
fn metadata(input: &Input) -> Result<Value> {
    let mut result = json!({});
    for (name, value) in [
        ("turn_id", &input.turn_id),
        ("source", &input.source),
        ("reason", &input.reason),
        ("tool_name", &input.tool_name),
        ("notification_type", &input.notification_type),
        ("error", &input.error),
        ("agent_id", &input.agent_id),
    ] {
        if let Some(value) = value {
            model::identifier(value)?;
            result[name] = json!(value);
        }
    }
    if let Some(value) = input.stop_hook_active {
        result["stop_hook_active"] = json!(value);
    }
    Ok(result)
}
pub fn report(
    registry: &Path,
    instance: &str,
    session: &str,
    source: &str,
    kind: &str,
    summary: &str,
) -> Result<()> {
    let context = model::load(registry, instance)?;
    if !["agent", "skill"].contains(&source)
        || !["ready", "progress", "complete", "blocked"].contains(&kind)
    {
        return Err("business report needs explicit agent/skill source and report type".into());
    }
    if summary.trim().is_empty() || summary.len() > 2048 {
        return Err("business summary must have 1..2048 bytes".into());
    }
    journal::append(
        registry,
        &context,
        Event {
            received_ms: journal::now()?,
            source: format!("{source}-report"),
            event: kind.into(),
            native_session_id: session.into(),
            native: json!({}),
            summary: Some(summary.into()),
        },
        false,
    )
}
