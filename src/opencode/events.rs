//! Native SSE is volatile. Keep exact event names/data and never turn it into hooks.
use super::api::Api;
use crate::{
    Result,
    config::Config,
    feedback::{
        self, Context,
        journal::{self, Event},
    },
};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    time::Duration,
};

pub fn enroll(config: &Config, context: &Context, session: &Value) -> Result<()> {
    let registry = config
        .feedback_registry
        .as_ref()
        .ok_or("missing feedback_registry")?;
    let id = crate::native::text(session, "/id")?;
    let event = Event {
        received_ms: journal::now()?,
        source: "native-api".into(),
        event: "session.create".into(),
        native_session_id: id,
        native: session.clone(),
        summary: None,
    };
    journal::append(registry, context, &event, true)
}
pub fn start(config: &Config, context: &Context, url: &str, password: &str) -> Result<()> {
    let response = Api::new(url, password, Duration::from_secs(10))?.stream()?;
    let registry = config.feedback_registry.clone().ok_or("missing registry")?;
    let context = Context {
        instance: context.instance.clone(),
        registration: context.registration.clone(),
    };
    std::thread::spawn(move || {
        if let Err(error) = consume(&registry, &context, BufReader::new(response)) {
            eprintln!("OpenCode native SSE ended; gap is not replayed: {error}");
        }
        let _ = gap(&registry, &context);
    });
    Ok(())
}
fn consume(registry: &Path, context: &Context, mut reader: impl BufRead) -> Result<()> {
    let mut data = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        if line.trim_end().is_empty() {
            if !data.is_empty() {
                record(registry, context, &serde_json::from_str(&data)?)?;
                data.clear();
            }
        } else if let Some(part) = line.strip_prefix("data:") {
            data.push_str(part.trim());
            data.push('\n');
        }
    }
}
fn record(registry: &Path, context: &Context, value: &Value) -> Result<()> {
    let Some(kind) = value["type"].as_str() else {
        return Ok(());
    };
    let Some(id) = value["data"]["sessionID"].as_str() else {
        return Ok(());
    };
    if !collected(kind) || feedback::registered_session(registry, context)?.as_deref() != Some(id) {
        return Ok(());
    }
    let event = Event {
        received_ms: journal::now()?,
        source: "native-sse".into(),
        event: kind.into(),
        native_session_id: id.into(),
        native: value.clone(),
        summary: None,
    };
    feedback::record_native(registry, context, &event, &crate::message::Native)
}
fn collected(kind: &str) -> bool {
    [
        "session.execution.",
        "session.retry.",
        "permission.",
        "form.",
    ]
    .iter()
    .any(|prefix| kind.starts_with(prefix))
}
fn gap(registry: &Path, context: &Context) -> Result<()> {
    let session = feedback::registered_session(registry, context)?
        .ok_or("native session missing at SSE end")?;
    let event = Event {
        received_ms: journal::now()?,
        source: "native-api".into(),
        event: "event.subscribe-ended".into(),
        native_session_id: session,
        native: serde_json::json!({"state":"unknown","volatile":true,"replayed":false}),
        summary: None,
    };
    journal::append(registry, context, &event, false)
}
