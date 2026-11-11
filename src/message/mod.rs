mod binding;
mod cli;
mod constraint;
mod core;
#[cfg(target_os = "linux")]
mod size;
mod transport;
mod tui;

pub use binding::{Binding, Config, Member, Peer};
pub use cli::cli;
pub(crate) use constraint::validate as validate_constraint;
pub use transport::{Bounded, Native, NotStarted, Transport};
/// Prefix of every send error raised after the native prompt call was made.
pub const SUBMISSION_UNCONFIRMED: &str = "message submission unconfirmed";

use crate::{Result, feedback::model::identifier, native::strings};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub id: String,
    pub from: String,
    pub to: String,
    pub reply_to: Option<String>,
    pub content: String,
}
#[derive(Debug, Serialize)]
pub struct Envelope<'a> {
    pub team: &'a str,
    /// Current running-team configuration for the bound receiving role.
    pub recipient_constraints: String,
    /// Explicit feedback presentation; ordinary text is never inspected or decoded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_view: Option<FeedbackView>,
    pub message: &'a Message,
}
#[derive(Debug, Serialize)]
pub struct FeedbackView {
    pub version: u8,
    pub fact: serde_json::Value,
}
#[derive(Debug, Serialize)]
pub struct Receipt<'a> {
    pub envelope: Envelope<'a>,
    pub transport: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_turn: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_prompt: Option<crate::opencode::api::PromptReceipt>,
}
impl Message {
    fn validate(&self) -> Result<()> {
        identifier(&self.id)?;
        crate::config::name(&self.from)?;
        crate::config::name(&self.to)?;
        if let Some(reply) = &self.reply_to {
            identifier(reply)?;
            if reply == &self.id {
                return Err("reply_to must differ from message id".into());
            }
        }
        if self.content.trim().is_empty() || self.content.len() > 16_384 {
            return Err("message content must have 1..16384 UTF-8 bytes".into());
        }
        Ok(())
    }
}
/// Submit one envelope with the unchanged business message and current recipient constraints.
pub fn send<'a>(
    binding: &'a Binding,
    message: &'a Message,
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let recipient = route(binding, message, transport)?;
    do_send(binding, recipient, message, None, true, transport)
}
/// Send a halt notification to the dispatcher only.
/// Skips the sender session check because the worker has a rejected session after `/clear`.
pub fn send_halt_to_dispatcher<'a>(
    binding: &'a Binding,
    message: &'a Message,
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let recipient = route_halt(binding, message, transport)?;
    do_send(binding, recipient, message, None, true, transport)
}
/// Add a readable copy only for the feedback producer's explicitly supplied fact.
/// The legacy message content and all routing checks keep their existing semantics.
pub(crate) fn send_feedback<'a>(
    binding: &'a Binding,
    message: &'a Message,
    fact: &impl Serialize,
    is_halt: bool,
    compact_stop: bool,
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let recipient = if is_halt {
        route_halt(binding, message, transport)?
    } else {
        route(binding, message, transport)?
    };
    let view = FeedbackView {
        version: 1,
        fact: serde_json::to_value(fact)?,
    };
    do_send(
        binding,
        recipient,
        message,
        Some(view),
        compact_stop,
        transport,
    )
}
fn route<'a>(
    binding: &'a Binding,
    message: &Message,
    transport: &impl Transport,
) -> Result<&'a Member> {
    message.validate()?;
    let sender = binding.member(&message.from)?;
    let recipient = binding.member(&message.to)?;
    if sender.context.instance == recipient.context.instance {
        return Err("message must address the other bound role".into());
    }
    binding.guard()?;
    core::instance(&binding.config, transport)?;
    binding.check(sender, transport)?;
    binding.check(recipient, transport)?;
    binding.guard()?;
    Ok(recipient)
}
/// Halt route: the fixed worker→dispatcher direction still holds; only the sender's own
/// session check is skipped, because that session has just been rejected.
fn route_halt<'a>(
    binding: &'a Binding,
    message: &Message,
    transport: &impl Transport,
) -> Result<&'a Member> {
    message.validate()?;
    let worker = &binding.worker.context.registration.role;
    let dispatcher = &binding.dispatcher.context.registration.role;
    if &message.from != worker || &message.to != dispatcher || worker == dispatcher {
        return Err("halt notification must go from the bound worker to its dispatcher".into());
    }
    dispatcher_only(binding, transport)
}
/// Container instance and the dispatcher's own Hook session and location; sender not checked.
fn dispatcher_only<'a>(binding: &'a Binding, transport: &impl Transport) -> Result<&'a Member> {
    let recipient = &binding.dispatcher;
    binding.guard()?;
    core::instance(&binding.config, transport)?;
    binding.check(recipient, transport)?;
    binding.guard()?;
    Ok(recipient)
}
fn do_send<'a>(
    binding: &'a Binding,
    recipient: &'a Member,
    message: &'a Message,
    feedback_view: Option<FeedbackView>,
    compact: bool,
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let envelope = Envelope {
        team: &binding.team,
        recipient_constraints: constraint::recipient(binding, &message.to)?,
        feedback_view,
        message,
    };
    let text = if envelope.feedback_view.is_some() && !compact {
        serde_json::to_string_pretty(&envelope)?
    } else {
        serde_json::to_string(&envelope)?
    };
    if recipient.context.registration.agent == crate::config::AgentKind::OpenCode {
        return send_opencode(binding, recipient, envelope, &text, transport);
    }
    #[cfg(target_os = "linux")]
    if binding.config.runtime == crate::config::Runtime::Tui {
        size::linux_argument(&text)?;
    }
    let args = match binding.config.runtime {
        crate::config::Runtime::Tui => strings(&["agent", "prompt", &recipient.handle, &text]),
        crate::config::Runtime::Core => strings(&["send", &recipient.handle, &text]),
    };
    let result = transport.call(&binding.config, &args);
    let native_turn = submission(binding, recipient, result, transport)?;
    Ok(Receipt {
        envelope,
        transport: "accepted",
        native_turn,
        native_prompt: None,
    })
}
fn send_opencode<'a>(
    binding: &'a Binding,
    recipient: &'a Member,
    envelope: Envelope<'a>,
    text: &str,
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let submitted = transport.session_prompt(&binding.config, recipient, text);
    if submitted
        .as_ref()
        .is_err_and(|error| error.is::<NotStarted>())
    {
        return Err(NotStarted.into());
    }
    let verified = submitted
        .and_then(|receipt| {
            binding.guard()?;
            binding.check(&binding.dispatcher, transport)?;
            binding.check(&binding.worker, transport)?;
            Ok(receipt)
        })
        .map_err(|error| {
            format!("{SUBMISSION_UNCONFIRMED}: {error}; do not automatically resend")
        })?;
    Ok(Receipt {
        envelope,
        transport: "accepted",
        native_turn: None,
        native_prompt: Some(verified),
    })
}
fn submission(
    binding: &Binding,
    recipient: &Member,
    result: Result<serde_json::Value>,
    transport: &impl Transport,
) -> Result<Option<u64>> {
    // A prompt that was never started was not submitted; only a started one is unknown.
    let result = match result {
        Err(error) if error.is::<NotStarted>() => return Err(error),
        result => result,
    };
    let verified = result.and_then(|value| {
        if binding.config.runtime == crate::config::Runtime::Core {
            return core::submitted(binding, &value, transport).map(Some);
        }
        if value["type"] != "agent_prompted" {
            return Err("missing native prompt acceptance".into());
        }
        binding.verify(recipient, &value["agent"])?;
        binding.guard()?;
        Ok(None)
    });
    verified.map_err(|error| {
        format!(
            "{SUBMISSION_UNCONFIRMED}: {error}; outcome may be unknown; do not automatically resend"
        )
        .into()
    })
}
