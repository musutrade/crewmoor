//! TUI identity: the Hook-registered session is authoritative, the pane's Agent
//! process locates it, and Herdr's optional `agent_session` is an extra check.
use super::{Config, Member, Peer, Transport};
use crate::{
    Result,
    feedback::Context,
    native::{pid, strings, text},
    process,
};
use serde_json::Value;
use std::path::Path;

pub fn capture(
    config: &Config,
    context: Context,
    session: String,
    peer: &Peer,
    agent: &Value,
    transport: &impl Transport,
) -> Result<Member> {
    let handle = text(agent, "/pane_id")?;
    if peer.handle != handle {
        return Err("use the exact returned pane handle".into());
    }
    let process = match &peer.agent_executable {
        Some(executable) => Some(observe(config, &context, &handle, executable, transport)?),
        None => None,
    };
    let member = Member {
        context,
        handle,
        terminal_id: Some(text(agent, "/terminal_id")?),
        workspace_id: Some(text(agent, "/workspace_id")?),
        core: None,
        native_session: Some(session),
        process,
    };
    session_matches(&member, agent)?;
    Ok(member)
}
/// Agent kind must match; a container session, when present, must equal the registered one.
pub fn session_matches(member: &Member, value: &Value) -> Result<()> {
    let registration = &member.context.registration;
    if text(value, "/agent")? != registration.agent.name() {
        return Err("bound Agent kind changed".into());
    }
    let container = value.get("agent_session").filter(|v| !v.is_null());
    let Some(container) = container else {
        return member
            .process
            .as_ref()
            .map(|_| ())
            .ok_or_else(|| "no container session and no bound Agent process identity".into());
    };
    let expected = member
        .native_session
        .as_deref()
        .or(registration.native_session_id.as_deref())
        .ok_or("binding has no registered native session")?;
    if text(container, "/kind")? != "id" || text(container, "/value")? != expected {
        return Err("bound Agent native session changed".into());
    }
    Ok(())
}
/// Re-observe the pane's Agent process; any replacement is refused.
pub fn check_process(config: &Config, member: &Member, transport: &impl Transport) -> Result<()> {
    let Some(expected) = &member.process else {
        return Ok(());
    };
    let current = observe(
        config,
        &member.context,
        &member.handle,
        &expected.executable,
        transport,
    )?;
    if &current != expected {
        return Err("bound Agent process changed; refusing message".into());
    }
    Ok(())
}
fn observe(
    config: &Config,
    context: &Context,
    handle: &str,
    executable: &Path,
    transport: &impl Transport,
) -> Result<process::Identity> {
    let value = transport.call(
        config,
        &strings(&["pane", "process-info", "--pane", handle]),
    )?;
    let info = &value["process_info"];
    if info["pane_id"].as_str() != Some(handle) {
        return Err("process-info pane changed".into());
    }
    let list = info["foreground_processes"]
        .as_array()
        .ok_or("missing pane foreground processes")?;
    let kind = context.registration.agent.name();
    let mut matches = list.iter().filter(|v| v["name"] == kind);
    let found = matches
        .next()
        .ok_or("bound Agent process not found in pane")?;
    if matches.next().is_some() {
        return Err("multiple matching Agent processes; location unknown".into());
    }
    process::agent_identity(pid(found, "/pid")?, executable, &context.registration.cwd)
}
