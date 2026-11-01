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

#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    pub received_ms: u64,
    pub source: String,
    pub event: String,
    pub native_session_id: String,
    pub native: Value,
    pub summary: Option<String>,
}
pub fn append(registry: &Path, context: &Context, event: Event, enroll: bool) -> Result<()> {
    let root = model::instance_dir(registry, &context.instance)?;
    let _lock = crate::record::Lock::take(&root)?;
    bind(&root, context, &event.native_session_id, enroll)?;
    let mut bytes = serde_json::to_vec(&event)?;
    bytes.push(b'\n');
    fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(root.join("events.jsonl"))?
        .write_all(&bytes)?;
    Ok(())
}
fn bind(root: &Path, context: &Context, session: &str, enroll: bool) -> Result<()> {
    model::identifier(session)?;
    let path = root.join("native-session.json");
    match fs::read(&path) {
        Ok(bytes) => check_session(&serde_json::from_slice::<String>(&bytes)?, session),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            enroll_session(&path, context, session, enroll)
        }
        Err(e) => Err(e.into()),
    }
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
    let last_report = events.iter().rev().find(|e| e.source != "native-hook");
    Ok(
        json!({"context":context,"agent_state":"unknown","latest_native_event":last_hook,
        "latest_business_report":last_report,"events":events}),
    )
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
