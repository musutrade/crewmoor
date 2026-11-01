mod binding;
mod cli;
mod transport;

pub use binding::{Binding, Config, Member, Peer};
pub use cli::cli;
pub use transport::{Native, Transport};

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
    let envelope = Envelope {
        team: &binding.team,
        message,
    };
    let text = serde_json::to_string(&envelope)?;
    let args = strings(&["agent", "prompt", &recipient.handle, &text]);
    let result = transport.call(&binding.config, &args);
    submission(binding, recipient, result)?;
    Ok(Receipt {
        envelope,
        transport: "accepted",
    })
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
    binding.check(sender, transport)?;
    binding.check(recipient, transport)?;
    binding.guard()?;
    Ok(recipient)
}
fn submission(
    binding: &Binding,
    recipient: &Member,
    result: Result<serde_json::Value>,
) -> Result<()> {
    let verified = result.and_then(|value| {
        if value["type"] != "agent_prompted" {
            return Err("missing native prompt acceptance".into());
        }
        binding.verify(recipient, &value["agent"])?;
        binding.guard()
    });
    verified.map_err(|error| {
        format!("message submission unconfirmed: {error}; outcome may be unknown; do not automatically resend")
            .into()
    })
}
