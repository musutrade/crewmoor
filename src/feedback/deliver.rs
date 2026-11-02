//! On the dispatcher's own recorded Stop: when its assigned issue is closed, send it the
//! next open issue once. Still-open, unassigned or empty queues are skipped silently.
use super::journal::{self, Event};
use crate::{
    Result,
    issue::{self, Assignment, Gh, Issue},
    message::{self, Binding, Message, Transport},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;

/// Journal source of a note that delivering the next issue was not confirmed.
pub const DELIVERY_UNCONFIRMED: &str = "issue-delivery-unconfirmed";
/// Closed assignment whose next item was already sent; the same Stop never resends it.
pub const DELIVERED_FILE: &str = "issue-delivered.json";

#[derive(Deserialize, Serialize, PartialEq, Eq)]
struct Delivered {
    repo: String,
    closed_number: u64,
}
/// Why one delivery is unconfirmed. `error` stays in memory; only its kind is written.
struct Failure {
    stage: &'static str,
    /// `not-submitted`, `submission-unconfirmed`, or `accepted` (only the marker failed).
    kind: &'static str,
    assignment: Option<Assignment>,
    next: Option<u64>,
    error: String,
}
impl Failure {
    fn at(
        stage: &'static str,
        assignment: Option<&Assignment>,
    ) -> impl FnOnce(Box<dyn std::error::Error>) -> Self {
        let assignment = assignment.cloned();
        move |error| Self {
            stage,
            kind: "not-submitted",
            assignment,
            next: None,
            error: error.to_string(),
        }
    }
}

/// Deliver after this dispatcher instance recorded `stop`; the error, if any, is noted once.
pub(super) fn after_stop(
    registry: &Path,
    instance: &str,
    stop: &Event,
    gh: &impl Gh,
    transport: &impl Transport,
) -> Option<String> {
    let failure = attempt(registry, instance, gh, transport).err()?;
    let mut error = failure.error.clone();
    if note(registry, instance, stop, &failure).is_err() {
        error.push_str("; unconfirmed note not written");
    }
    Some(error)
}
fn attempt(
    registry: &Path,
    instance: &str,
    gh: &impl Gh,
    transport: &impl Transport,
) -> std::result::Result<(), Failure> {
    let Some((binding, assignment)) =
        pending(registry, instance).map_err(Failure::at("local", None))?
    else {
        return Ok(());
    };
    let at = |stage| Failure::at(stage, Some(&assignment));
    if !issue::state(&assignment, gh).map_err(at("done"))?.closed {
        return Ok(());
    }
    let Some(next) =
        issue::next_open(&assignment.repo, issue::DEFAULT_LABEL, gh).map_err(at("next"))?
    else {
        return Ok(());
    };
    send(registry, instance, &binding, &assignment, &next, transport)
}
/// The dispatcher's binding and assignment, unless one is missing or already delivered.
fn pending(registry: &Path, instance: &str) -> Result<Option<(Binding, Assignment)>> {
    let Some(binding) = bound_dispatcher(registry, instance)? else {
        return Ok(None);
    };
    let Some(assignment) = issue::assigned(registry, instance)? else {
        return Ok(None);
    };
    let delivered = Delivered {
        repo: assignment.repo.clone(),
        closed_number: assignment.number,
    };
    let already = read_delivered(registry, instance)?.is_some_and(|d| d == delivered);
    Ok((!already).then_some((binding, assignment)))
}
/// The binding referenced by the dispatcher side, only when this instance is its dispatcher.
fn bound_dispatcher(registry: &Path, instance: &str) -> Result<Option<Binding>> {
    let Some(binding_ref) = super::model::load_dispatcher_binding_ref(registry, instance)? else {
        return Ok(None);
    };
    let binding = super::read_binding(&binding_ref.binding)?;
    if binding.dispatcher.context.instance != instance {
        return Err("dispatcher binding reference names another dispatcher".into());
    }
    Ok(Some(binding))
}
/// One send; a submitted (or possibly submitted) prompt is marked so it is never resent.
fn send(
    registry: &Path,
    instance: &str,
    binding: &Binding,
    assignment: &Assignment,
    next: &Issue,
    transport: &impl Transport,
) -> std::result::Result<(), Failure> {
    let (kind, error) = submit(binding, assignment, next, transport);
    let marked = match kind {
        "not-submitted" => Ok(()),
        _ => mark(registry, instance, assignment),
    };
    let error = match (error, marked) {
        (None, Ok(())) => return Ok(()),
        (Some(error), Ok(())) => error,
        (error, Err(marker)) => format!(
            "{}; delivered marker not written: {marker}",
            error.as_deref().unwrap_or("next issue delivered")
        ),
    };
    Err(Failure {
        stage: if kind == "accepted" { "marker" } else { "send" },
        kind,
        assignment: Some(assignment.clone()),
        next: Some(next.number),
        error,
    })
}
/// Submit the notice once: its outcome kind and, unless accepted, the error.
fn submit(
    binding: &Binding,
    assignment: &Assignment,
    next: &Issue,
    transport: &impl Transport,
) -> (&'static str, Option<String>) {
    let sent = notice(binding, assignment, next)
        .and_then(|message| message::send_to_dispatcher(binding, &message, transport).map(|_| ()));
    match sent.map_err(|e| e.to_string()) {
        Ok(()) => ("accepted", None),
        Err(e) if e.starts_with(message::SUBMISSION_UNCONFIRMED) => {
            ("submission-unconfirmed", Some(e))
        }
        Err(e) => ("not-submitted", Some(e)),
    }
}
fn notice(binding: &Binding, assignment: &Assignment, next: &Issue) -> Result<Message> {
    let role = binding.dispatcher.context.registration.role.clone();
    let content = json!({
        "system_issue_delivery": true,
        "feedback_instance": binding.dispatcher.context.instance,
        "completed": {"repo": assignment.repo, "number": assignment.number},
        "next": {"repo": assignment.repo, "number": next.number,
            "title": next.title, "url": next.url},
    });
    Ok(Message {
        id: uuid::Uuid::new_v4().simple().to_string(),
        from: role.clone(),
        to: role,
        reply_to: None,
        content: serde_json::to_string(&content)?,
    })
}
fn read_delivered(registry: &Path, instance: &str) -> Result<Option<Delivered>> {
    let path = super::model::instance_dir(registry, instance)?.join(DELIVERED_FILE);
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn mark(registry: &Path, instance: &str, assignment: &Assignment) -> Result<()> {
    let delivered = Delivered {
        repo: assignment.repo.clone(),
        closed_number: assignment.number,
    };
    issue::replace(
        &super::model::instance_dir(registry, instance)?,
        DELIVERED_FILE,
        &serde_json::to_vec(&delivered)?,
    )
}
/// Free-form errors never reach disk; only the stage and whether a prompt was submitted.
fn note(registry: &Path, instance: &str, stop: &Event, failure: &Failure) -> Result<()> {
    let assignment = failure.assignment.as_ref();
    journal::note(
        registry,
        instance,
        &Event {
            received_ms: journal::now()?,
            source: DELIVERY_UNCONFIRMED.into(),
            event: stop.event.clone(),
            native_session_id: stop.native_session_id.clone(),
            native: json!({"forwarded_received_ms": stop.received_ms,
                "repo": assignment.map(|a| &a.repo),
                "closed_number": assignment.map(|a| a.number),
                "next_number": failure.next, "stage": failure.stage, "error": failure.kind}),
            summary: None,
        },
    )
}
