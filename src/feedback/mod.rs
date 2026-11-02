//! User-level native hooks and explicitly authored business reports.
mod cli;
mod event;
mod forward;
pub mod install;
mod journal;
pub(crate) mod model;
pub use cli::cli;
pub use event::{ingest, report};
pub use forward::report_to;
pub use journal::{REJECTED, last_completed_event, registered_session, require_unchanged, show};
pub use model::{
    BindingRef, Context, Evidence, Registration, load_binding_ref, register, write_binding_ref,
};
pub const INSTANCE_ENV: &str = "CREWMOOR_FEEDBACK_INSTANCE";

use crate::{Result, message::Transport};
use std::path::Path;

/// Process one hook event: ingest into the journal, then attempt forwarding if this is a bound
/// worker and the event is Stop, SessionEnd, or a session-rejected halt record.
/// Forwarding errors are suppressed; the hook always returns `{}`.
/// Returns `true` if the normal event was ingested successfully.
pub fn process_hook_event(
    agent: &str,
    registry: &Path,
    instance: &str,
    bytes: &[u8],
    transport: &impl Transport,
) -> bool {
    let ingest_ok = ingest_hook(agent, registry, instance, bytes).is_ok();
    let event_name = event_name_from_bytes(bytes);
    let session = session_from_bytes(bytes);
    if let Some(name) = event_name {
        maybe_forward(
            registry,
            instance,
            &name,
            ingest_ok,
            session.as_deref(),
            transport,
        );
    }
    ingest_ok
}

/// The Agent kind of a registered context that this callback's Agent matches.
pub(crate) fn associated(
    agent: &str,
    registry: &Path,
    instance: &str,
) -> Result<crate::config::AgentKind> {
    let agent_kind: crate::config::AgentKind = serde_json::from_value(agent.into())?;
    let context = model::load(registry, instance)?;
    if context.registration.agent != agent_kind {
        return Err("unassociated Agent kind".into());
    }
    Ok(agent_kind)
}
fn ingest_hook(agent: &str, registry: &Path, instance: &str, bytes: &[u8]) -> Result<()> {
    let agent_kind = associated(agent, registry, instance)?;
    event::ingest(registry, instance, &agent_kind, bytes)
}

fn event_name_from_bytes(bytes: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|v| v["hook_event_name"].as_str().map(str::to_owned))
}

fn session_from_bytes(bytes: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|v| v["session_id"].as_str().map(str::to_owned))
}

fn maybe_forward(
    registry: &Path,
    instance: &str,
    event_name: &str,
    ingest_ok: bool,
    incoming_session: Option<&str>,
    transport: &impl Transport,
) {
    // Forward Stop/SessionEnd normally; always check for session-rejected halt.
    if ingest_ok && !matches!(event_name, "Stop" | "SessionEnd") {
        return;
    }
    let _ = try_forward(registry, instance, ingest_ok, incoming_session, transport);
}

fn try_forward(
    registry: &Path,
    instance: &str,
    ingest_ok: bool,
    incoming_session: Option<&str>,
    transport: &impl Transport,
) -> Result<()> {
    let Some(binding) = bound_worker(registry, instance)? else {
        return Ok(());
    };
    let Some(record) = journal::last_completed_event(registry, instance)? else {
        return Ok(());
    };
    // Confirm a session-rejected record belongs to this invocation via session match.
    let is_halt = record.source == REJECTED
        && incoming_session.is_some_and(|s| s == record.native_session_id);
    if !ingest_ok && !is_halt {
        // Ingest failed for a non-rejection reason; nothing to forward.
        return Ok(());
    }
    forward::forward_hook_event(&binding, &record, is_halt, transport).map(|_| ())
}
/// The binding written by `team bind`, only when this instance is its worker side.
fn bound_worker(registry: &Path, instance: &str) -> Result<Option<crate::message::Binding>> {
    let Some(binding_ref) = model::load_binding_ref(registry, instance)? else {
        return Ok(None);
    };
    let binding: crate::message::Binding =
        serde_json::from_slice(&std::fs::read(&binding_ref.binding)?)?;
    Ok((binding.worker.context.instance == instance).then_some(binding))
}
