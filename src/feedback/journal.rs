use super::{Context, model};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    pub received_ms: u64,
    pub source: String,
    pub event: String,
    pub native_session_id: String,
    pub native: Value,
    pub summary: Option<String>,
}
pub const REJECTED: &str = "session-rejected";
/// What one append wrote while holding the journal lock.
pub enum Appended {
    Recorded(Event),
    /// The session differed; only this rejection notice was written.
    Rejected(Event),
}
pub fn append(registry: &Path, context: &Context, event: &Event, enroll: bool) -> Result<()> {
    match record(registry, context, event, enroll)? {
        Appended::Recorded(_) => Ok(()),
        Appended::Rejected(_) => {
            Err("native session differs; feedback not associated; rejection recorded".into())
        }
    }
}
pub fn record(registry: &Path, context: &Context, event: &Event, enroll: bool) -> Result<Appended> {
    let root = model::instance_dir(registry, &context.instance)?;
    let _lock = crate::record::Lock::take(&root)?;
    model::identifier(&event.native_session_id)?;
    if let Some(expected) = differing(&root, context, &event.native_session_id)? {
        // Team halts: record only the minimal notice fact, never adopt the new session.
        let notice = rejection(event, &expected)?;
        write_event(&root, &notice)?;
        return Ok(Appended::Rejected(notice));
    }
    bind(&root, context, &event.native_session_id, enroll)?;
    write_event(&root, event)?;
    Ok(Appended::Recorded(event.clone()))
}
/// Append a fact about this callback's own forward, outside any session association.
pub fn note(registry: &Path, instance: &str, event: &Event) -> Result<()> {
    let root = model::instance_dir(registry, instance)?;
    let _lock = crate::record::Lock::take(&root)?;
    write_event(&root, event)
}
fn write_event(root: &Path, event: &Event) -> Result<()> {
    let mut bytes = serde_json::to_vec(event)?;
    bytes.push(b'\n');
    fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(root.join("events.jsonl"))?
        .write_all(&bytes)?;
    Ok(())
}
/// The already associated session when the incoming one differs from it.
fn differing(root: &Path, context: &Context, session: &str) -> Result<Option<String>> {
    let known = stored(root)?.or_else(|| context.registration.native_session_id.clone());
    Ok(known.filter(|expected| expected != session))
}
fn rejection(event: &Event, expected: &str) -> Result<Event> {
    Ok(Event {
        received_ms: now()?,
        source: REJECTED.into(),
        event: event.event.clone(),
        native_session_id: event.native_session_id.clone(),
        native: json!({"source":event.native.get("source"),"from":event.source,
            "associated_session":expected}),
        summary: None,
    })
}
fn stored(root: &Path) -> Result<Option<String>> {
    match fs::read(root.join("native-session.json")) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
/// Session established by the Agent's own SessionStart, or the explicit registration.
pub fn registered_session(registry: &Path, context: &Context) -> Result<Option<String>> {
    let root = model::instance_dir(registry, &context.instance)?;
    Ok(stored(&root)?.or_else(|| context.registration.native_session_id.clone()))
}
/// Refuse when the Agent's own Hook already reported a different session for this
/// instance. Read-only: the association is kept and the new session is never adopted.
/// Takes no lock, so a concurrent Hook append is never refused because of this check.
pub fn require_unchanged(registry: &Path, context: &Context) -> Result<()> {
    let root = model::instance_dir(registry, &context.instance)?;
    let changed = completed_events(&root)?
        .into_iter()
        .any(|e| e.source == REJECTED && e.native["from"] == "native-hook");
    if changed {
        return Err("Hook reported a changed native session; team halted, binding kept".into());
    }
    Ok(())
}
fn bind(root: &Path, context: &Context, session: &str, enroll: bool) -> Result<()> {
    let path = root.join("native-session.json");
    if path.exists() {
        return Ok(());
    }
    enroll_session(&path, context, session, enroll)
}
fn check_session(expected: &str, actual: &str) -> Result<()> {
    if expected != actual {
        return Err("native session differs; feedback not associated".into());
    }
    Ok(())
}
fn enroll_session(path: &Path, context: &Context, session: &str, enroll: bool) -> Result<()> {
    if let Some(expected) = &context.registration.native_session_id {
        check_session(expected, session)?;
    } else if !enroll {
        return Err("native session unknown; wait for associated SessionStart".into());
    }
    model::write_new(path, &serde_json::to_vec(session)?)
}
pub fn now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
pub fn show(registry: &Path, instance: &str) -> Result<Value> {
    let context = model::load(registry, instance)?;
    let root = model::instance_dir(registry, instance)?;
    let _lock = crate::record::Lock::take(&root)?;
    let events = read_events(&root)?;
    let last_hook = events.iter().rev().find(|e| e.source == "native-hook");
    let last_report = events.iter().rev().find(|e| e.source.ends_with("-report"));
    let rejected = events.iter().rev().find(|e| e.source == REJECTED);
    Ok(
        json!({"context":context,"agent_state":"unknown","latest_native_event":last_hook,
        "latest_business_report":last_report,"latest_session_rejection":rejected,"events":events}),
    )
}
/// Snapshot of fully appended records: an unterminated last line is an append still in
/// progress and is left for the next read. Every completed line must parse.
fn completed_events(root: &Path) -> Result<Vec<Event>> {
    let bytes = match fs::read(root.join("events.jsonl")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let complete = bytes
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |end| end + 1);
    std::str::from_utf8(&bytes[..complete])?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}
fn read_events(root: &Path) -> Result<Vec<Event>> {
    match fs::read_to_string(root.join("events.jsonl")) {
        Ok(text) => text
            .lines()
            .map(|line| Ok(serde_json::from_str(line)?))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}
