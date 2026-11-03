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
    // Present on Claude Stop: count lets the dispatcher know tasks exist without storing content.
    #[serde(default)]
    background_tasks: Vec<serde_json::Value>,
    #[serde(default)]
    session_crons: Vec<serde_json::Value>,
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
    match hook(registry, instance, agent, bytes)? {
        journal::Appended::Recorded(_) => Ok(()),
        journal::Appended::Rejected(_) => {
            Err("native session differs; feedback not associated; rejection recorded".into())
        }
        journal::Appended::Refused => Err("native Hook refused; rejection recorded".into()),
    }
}
/// One native Hook event, and exactly what its append wrote.
pub fn hook(
    registry: &Path,
    instance: &str,
    agent: &AgentKind,
    bytes: &[u8],
) -> Result<journal::Appended> {
    let context = model::load(registry, instance)?;
    let input: Input = match serde_json::from_slice(bytes) {
        Ok(input) => input,
        Err(_) => return journal::refuse(registry, instance, "unknown", "invalid-input"),
    };
    if let Err(reason) = checked_input(registry, &context, agent, &input) {
        return refuse_input(registry, instance, &input, reason);
    }
    let native = match metadata(&input) {
        Ok(native) => native,
        Err(_) => return refuse_input(registry, instance, &input, "invalid-metadata"),
    };
    let enroll = input.hook_event_name == "SessionStart";
    journal::record(
        registry,
        &context,
        &Event {
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
fn checked_input(
    registry: &Path,
    context: &super::Context,
    agent: &AgentKind,
    input: &Input,
) -> std::result::Result<(), &'static str> {
    if context.registration.agent != *agent {
        return Err("agent-mismatch");
    }
    if !events(agent).contains(&input.hook_event_name.as_str()) {
        return Err("uncollected-event");
    }
    let enroll = input.hook_event_name == "SessionStart";
    if enroll && input.agent_id.is_some() {
        return Err("subagent-enrollment");
    }
    model::identifier(&input.session_id).map_err(|_| "invalid-session")?;
    initial_association(registry, context, input, enroll)
}
fn initial_association(
    registry: &Path,
    context: &super::Context,
    input: &Input,
    enroll: bool,
) -> std::result::Result<(), &'static str> {
    if journal::registered_session(registry, context)
        .map_err(|_| "association-unavailable")?
        .is_some()
    {
        return Ok(());
    }
    if !enroll {
        return Err("session-unbound");
    }
    // cwd can change during a session. It only corroborates its first main SessionStart;
    // journal::record checks the immutable session again under the append lock.
    if input.cwd.canonicalize().ok().as_ref() != Some(&context.registration.cwd) {
        return Err("initial-cwd-mismatch");
    }
    Ok(())
}
fn refuse_input(
    registry: &Path,
    instance: &str,
    input: &Input,
    reason: &str,
) -> Result<journal::Appended> {
    let event = input.hook_event_name.as_str();
    let event = if events(&AgentKind::Claude).contains(&event)
        || events(&AgentKind::Codex).contains(&event)
    {
        event
    } else {
        "unknown"
    };
    journal::refuse(registry, instance, event, reason)
}
fn metadata(input: &Input) -> Result<Value> {
    let mut result = json!({});
    for (name, value) in [
        ("turn_id", &input.turn_id),
        ("source", &input.source),
        ("reason", &input.reason),
        ("tool_name", &input.tool_name),
        ("notification_type", &input.notification_type),
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
    if input.hook_event_name == "Stop" {
        let bt = input.background_tasks.len();
        let sc = input.session_crons.len();
        if bt > 0 {
            result["background_tasks_count"] = json!(bt);
        }
        if sc > 0 {
            result["session_crons_count"] = json!(sc);
        }
    }
    error_metadata(input, &mut result);
    Ok(result)
}
fn error_metadata(input: &Input, result: &mut Value) {
    match input.hook_event_name.as_str() {
        "PostToolUseFailure" => result["error_present"] = json!(input.error.is_some()),
        "StopFailure" => result["error"] = json!(stop_failure_kind(input.error.as_deref())),
        _ => {}
    }
}
fn stop_failure_kind(value: Option<&str>) -> &str {
    const KINDS: &[&str] = &[
        "rate_limit",
        "overloaded",
        "authentication_failed",
        "oauth_org_not_allowed",
        "account_on_hold",
        "billing_error",
        "invalid_request",
        "model_not_found",
        "server_error",
        "max_output_tokens",
        "cloud_credential_error",
        "unknown",
    ];
    value
        .filter(|value| KINDS.contains(value))
        .unwrap_or("unknown")
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
    recorded_report(registry, context, session, source, kind, summary).map(|_| ())
}
pub(super) fn recorded_report(
    registry: &Path,
    context: super::Context,
    session: &str,
    source: &str,
    kind: &str,
    summary: &str,
) -> Result<(super::Context, Event)> {
    if !["agent", "skill"].contains(&source)
        || !["ready", "progress", "complete", "blocked"].contains(&kind)
    {
        return Err("business report needs explicit agent/skill source and report type".into());
    }
    if summary.trim().is_empty() || summary.len() > 2048 {
        return Err("business summary must have 1..2048 bytes".into());
    }
    let event = Event {
        received_ms: journal::now()?,
        source: format!("{source}-report"),
        event: kind.into(),
        native_session_id: session.into(),
        native: json!({}),
        summary: Some(summary.into()),
    };
    journal::append(registry, &context, &event, false)?;
    Ok((context, event))
}
