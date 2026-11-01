//! Explicitly record one authored report and forward that exact record once.
use super::{Context, event, journal::Event};
use crate::{
    Result,
    message::{self, Binding, Message, Transport},
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize)]
struct Fact<'a> {
    feedback_instance: &'a str,
    task: &'a str,
    agent: &'a crate::config::AgentKind,
    evidence: &'a super::Evidence,
    record: &'a Event,
}
pub fn report_to(
    binding: &Binding,
    session: &str,
    source: &str,
    kind: &str,
    summary: &str,
    transport: &impl Transport,
) -> Result<Value> {
    let instance = &binding.worker.context.instance;
    let source_context = source_context(binding)?;
    let (context, record) = event::recorded_report(
        &binding.config.feedback_registry,
        source_context,
        session,
        source,
        kind,
        summary,
    )?;
    forward(binding, &context, &record, transport).map_err(|error| {
        format!("feedback recorded for {instance} at {}; forwarding unconfirmed: {error}; do not repeat report-to or replay this report", record.received_ms).into()
    })
}
fn source_context(binding: &Binding) -> Result<Context> {
    let context = super::model::load(
        &binding.config.feedback_registry,
        &binding.worker.context.instance,
    )?;
    if serde_json::to_value(&context)? != serde_json::to_value(&binding.worker.context)? {
        return Err(
            "feedback context differs from fixed worker binding; report not recorded or forwarded"
                .into(),
        );
    }
    Ok(context)
}
fn forward(
    binding: &Binding,
    context: &Context,
    record: &Event,
    transport: &impl Transport,
) -> Result<Value> {
    let fact = Fact {
        feedback_instance: &context.instance,
        task: &context.registration.task,
        agent: &context.registration.agent,
        evidence: &context.registration.evidence,
        record,
    };
    let message = Message {
        id: uuid::Uuid::new_v4().simple().to_string(),
        from: context.registration.role.clone(),
        to: binding.dispatcher.context.registration.role.clone(),
        reply_to: None,
        content: serde_json::to_string(&fact)?,
    };
    let delivery = message::send(binding, &message, transport)?;
    Ok(json!({"recorded":true,"delivery":delivery}))
}
