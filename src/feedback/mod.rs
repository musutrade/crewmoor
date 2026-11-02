//! User-level native hooks and explicitly authored business reports.
mod cli;
mod deliver;
mod event;
mod forward;
pub mod install;
mod journal;
pub(crate) mod model;
pub use cli::cli;
pub use deliver::{DELIVERED_FILE, DELIVERY_UNCONFIRMED};
pub use event::{ingest, report};
pub use forward::report_to;
pub use journal::{REJECTED, registered_session, require_unchanged, show};
pub use model::{
    BindingRef, Context, Evidence, Registration, load_binding_ref, load_dispatcher_binding_ref,
    register, write_binding_ref, write_dispatcher_binding_ref,
};
pub const INSTANCE_ENV: &str = "CREWMOOR_FEEDBACK_INSTANCE";

use crate::{Result, message::Transport};
use journal::{Appended, Event};
use std::path::Path;

/// Journal source of a note that forwarding a recorded Hook event was not confirmed.
pub const UNCONFIRMED: &str = "native-hook-unconfirmed";

/// What one Hook callback did. The callback still answers the Agent with `{}`.
pub struct HookOutcome {
    /// The Hook event itself was recorded (neither refused nor rejected).
    pub recorded: bool,
    /// A forward was attempted and failed or timed out; it is never resent.
    pub forward_error: Option<String>,
    /// Delivering the next issue on a dispatcher Stop failed or is unconfirmed; never resent.
    pub delivery_error: Option<String>,
}

/// Record one Hook event, then forward exactly what that append wrote when this is a bound
/// worker: a recorded `Stop`/`SessionEnd`, or this call's own session rejection as a halt.
/// When this is a bound dispatcher and it recorded a `Stop`, deliver the next issue if its
/// assigned one is closed.
pub fn process_hook_event(
    agent: &str,
    registry: &Path,
    instance: &str,
    bytes: &[u8],
    transport: &impl Transport,
    gh: &impl crate::issue::Gh,
) -> HookOutcome {
    let appended = ingest_hook(agent, registry, instance, bytes);
    let recorded = matches!(appended, Ok(Appended::Recorded(_)));
    let written = appended.ok();
    let forward_error = written
        .as_ref()
        .and_then(|written| forward_written(registry, instance, written, transport));
    let delivery_error = match &written {
        Some(Appended::Recorded(stop)) if stop.event == "Stop" => {
            deliver::after_stop(registry, instance, stop, gh, transport)
        }
        _ => None,
    };
    HookOutcome {
        recorded,
        forward_error,
        delivery_error,
    }
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
fn ingest_hook(agent: &str, registry: &Path, instance: &str, bytes: &[u8]) -> Result<Appended> {
    let agent_kind = associated(agent, registry, instance)?;
    event::hook(registry, instance, &agent_kind, bytes)
}
fn forward_written(
    registry: &Path,
    instance: &str,
    written: &Appended,
    transport: &impl Transport,
) -> Option<String> {
    let (record, is_halt) = match written {
        Appended::Recorded(event) if matches!(event.event.as_str(), "Stop" | "SessionEnd") => {
            (event, false)
        }
        Appended::Recorded(_) => return None,
        Appended::Rejected(event) => (event, true),
    };
    let mut error = try_forward(registry, instance, record, is_halt, transport)
        .err()?
        .to_string();
    if note_unconfirmed(registry, instance, record, is_halt, &error).is_err() {
        error.push_str("; unconfirmed note not written");
    }
    Some(error)
}
fn try_forward(
    registry: &Path,
    instance: &str,
    record: &Event,
    is_halt: bool,
    transport: &impl Transport,
) -> Result<()> {
    let Some(binding) = bound_worker(registry, instance)? else {
        return Ok(());
    };
    forward::forward_hook_event(&binding, record, is_halt, transport).map(|_| ())
}
/// The binding written by `team bind`, only when this instance is its worker side.
fn bound_worker(registry: &Path, instance: &str) -> Result<Option<crate::message::Binding>> {
    let Some(binding_ref) = model::load_binding_ref(registry, instance)? else {
        return Ok(None);
    };
    let binding = read_binding(&binding_ref.binding)?;
    Ok((binding.worker.context.instance == instance).then_some(binding))
}
fn read_binding(path: &Path) -> Result<crate::message::Binding> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
/// Free-form errors never reach disk; only whether the prompt was submitted is recorded.
fn note_unconfirmed(
    registry: &Path,
    instance: &str,
    record: &Event,
    is_halt: bool,
    error: &str,
) -> Result<()> {
    let kind = if error.starts_with(crate::message::SUBMISSION_UNCONFIRMED) {
        "submission-unconfirmed"
    } else {
        "not-submitted"
    };
    journal::note(
        registry,
        instance,
        &Event {
            received_ms: journal::now()?,
            source: UNCONFIRMED.into(),
            event: record.event.clone(),
            native_session_id: record.native_session_id.clone(),
            native: serde_json::json!({"forwarded_source":record.source,
                "forwarded_received_ms":record.received_ms,"halt":is_halt,"error":kind}),
            summary: None,
        },
    )
}
