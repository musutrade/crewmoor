mod binding;
mod cli;
mod core;
mod transport;
mod tui;

pub use binding::{Binding, Config, Member, Peer};
pub use cli::cli;
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
    pub message: &'a Message,
}
#[derive(Debug, Serialize)]
pub struct Receipt<'a> {
    pub envelope: Envelope<'a>,
    pub transport: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_turn: Option<u64>,
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
pub fn send<'a>(
    binding: &'a Binding,
    message: &'a Message,
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let recipient = route(binding, message, transport)?;
    do_send(binding, recipient, message, transport)
}
/// Send a halt notification to the dispatcher only.
/// Skips the sender session check because the worker has a rejected session after `/clear`.
pub fn send_halt_to_dispatcher<'a>(
    binding: &'a Binding,
    message: &'a Message,
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let recipient = route_halt(binding, message, transport)?;
    do_send(binding, recipient, message, transport)
}
/// Send a system notice to the dispatcher about its own work (the next issue item).
/// Sender and recipient are both the dispatcher role; only the dispatcher is checked.
pub fn send_to_dispatcher<'a>(
    binding: &'a Binding,
    message: &'a Message,
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let recipient = route_self(binding, message, transport)?;
    do_send(binding, recipient, message, transport)
}
fn route_self<'a>(
    binding: &'a Binding,
    message: &Message,
    transport: &impl Transport,
) -> Result<&'a Member> {
    message.validate()?;
    let dispatcher = &binding.dispatcher.context.registration.role;
    if &message.from != dispatcher || &message.to != dispatcher {
        return Err("system notice must go from and to the bound dispatcher".into());
    }
    dispatcher_only(binding, transport)
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
    transport: &impl Transport,
) -> Result<Receipt<'a>> {
    let envelope = Envelope {
        team: &binding.team,
        message,
    };
    let text = serde_json::to_string(&envelope)?;
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
