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
    let Some(id) = session_id(value) else {
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
fn session_id(value: &Value) -> Option<&str> {
    if value["type"] == "form.created" {
        return value["data"]["form"]["sessionID"].as_str();
    }
    value["data"]["sessionID"].as_str()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::{Evidence, Registration};
    use serde_json::json;
    use std::io::Cursor;

    fn setup() -> (tempfile::TempDir, Context, Config) {
        let root = tempfile::tempdir().unwrap();
        let context = feedback::register(
            root.path(),
            Registration {
                team: "native-regression".into(),
                role: "probe".into(),
                task: "unit".into(),
                cwd: root.path().into(),
                agent: crate::config::AgentKind::OpenCode,
                agent_version: "opencode v2.2.5".into(),
                evidence: Evidence::Synthetic,
                native_session_id: Some("ses_A".into()),
            },
        )
        .unwrap();
        let config=serde_json::from_value(json!({"team":"native-regression","role":"probe","runtime":"core","cwd":root.path(),"store":root.path().join("store"),"runtime_binary":"/unused/hc","runtime_version":"hc 0.1.17","agent":"opencode","agent_executable":"/unused/opencode.exe","agent_version":"opencode v2.2.5","feedback_registry":root.path(),"feedback_instance":context.instance})).unwrap();
        (root, context, config)
    }
    fn stream(values: &[Value]) -> Cursor<String> {
        let mut wire = String::from(": native keepalive\n\n");
        for value in values {
            wire.push_str(&format!("event: message\ndata: {value}\n\n"));
        }
        Cursor::new(wire)
    }
    #[test]
    fn exact_native_event_names_and_payloads_survive_without_manufactured_hooks() {
        let (root, context, config) = setup();
        let fact = json!({"id":"ses_A","native_field":{"中文":"引号\"和路径\\及\n换行"}});
        enroll(&config, &context, &fact).unwrap();
        let events = [
            json!({"id":"evt_one","created":1,"type":"session.execution.interrupted","data":{"sessionID":"ses_A","reason":"user"}}),
            json!({"id":"evt_two","created":2,"type":"permission.asked","data":{"id":"permission_one","sessionID":"ses_A","action":"edit","resources":["/owned/file"],"message":"中文 \"引号\" \\路径\n换行"}}),
        ];
        consume(root.path(), &context, stream(&events)).unwrap();
        let shown = feedback::show(root.path(), &context.instance).unwrap();
        assert_eq!(shown["events"][0]["event"], "session.create");
        assert_eq!(shown["events"][0]["native"], fact);
        assert_eq!(shown["events"][1]["native"], events[0]);
        assert_eq!(shown["latest_native_event"]["native"], events[1]);
        assert_eq!(shown["agent_state"], "unknown");
        gap(root.path(), &context).unwrap();
        let ended = feedback::show(root.path(), &context.instance).unwrap();
        assert_eq!(
            ended["latest_native_event"]["event"],
            "event.subscribe-ended"
        );
        assert_eq!(ended["latest_native_event"]["native"]["replayed"], false);
    }
    #[test]
    fn real_v2_nested_form_owner_is_used_and_global_or_foreign_forms_do_not_bind() {
        let (root, context, _) = setup();
        // Form.Created has data.form, unlike Permission and execution events.
        let form = json!({"type":"form.created","data":{"form":{"id":"form_one","sessionID":"ses_A","title":"Question","fields":[{"type":"string","key":"answer","title":"Answer"}]}}});
        let mut foreign = form.clone();
        foreign["data"]["form"]["sessionID"] = json!("ses_B");
        let mut global = form.clone();
        global["data"]["form"]["sessionID"] = json!("global");
        let events = [
            json!({"type":"server.connected","data":{}}),
            json!({"data":{"sessionID":"ses_A"}}),
            foreign,
            global,
            form.clone(),
            json!({"type":"session.text.delta","data":{"sessionID":"ses_A","text":"not collected"}}),
        ];
        consume(root.path(), &context, stream(&events)).unwrap();
        let shown = feedback::show(root.path(), &context.instance).unwrap();
        assert_eq!(shown["events"].as_array().unwrap().len(), 1);
        assert_eq!(shown["latest_native_event"]["native"], form);
        assert!(shown["latest_session_rejection"].is_null());
        assert_eq!(
            feedback::registered_session(root.path(), &context)
                .unwrap()
                .as_deref(),
            Some("ses_A")
        );
    }
    #[test]
    fn malformed_native_sse_and_missing_registry_are_errors_without_replay() {
        let (root, context, mut config) = setup();
        assert!(consume(root.path(), &context, Cursor::new("data: broken\n\n")).is_err());
        config.feedback_registry = None;
        assert!(enroll(&config, &context, &json!({"id":"ses_A"})).is_err());
    }
}
