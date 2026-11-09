use super::{Binding, Message, Receipt, SUBMISSION_UNCONFIRMED, Transport};
use crate::Result;
use serde::Serialize;

/// B's exact message and outcome; A remains the top-level accepted receipt.
#[derive(Debug, Serialize)]
pub struct ConstraintAttempt {
    pub message: Message,
    #[serde(flatten)]
    pub outcome: ConstraintOutcome,
}

#[derive(Debug, Serialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum ConstraintOutcome {
    Accepted {
        #[serde(skip_serializing_if = "Option::is_none")]
        native_turn: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        native_prompt: Option<crate::opencode::api::PromptReceipt>,
    },
    Failed {
        error: String,
    },
    Unknown {
        error: String,
    },
}
impl ConstraintOutcome {
    pub fn accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }
}

pub(crate) fn validate(content: Option<&str>) -> Result<()> {
    if content.is_some_and(|text| !text.trim().is_empty() && text.len() > 16_384) {
        return Err("message constraint must have at most 16384 UTF-8 bytes when nonblank".into());
    }
    Ok(())
}

/// Prepare and validate B before submitting A. Whitespace only disables B; it never rewrites it.
pub(super) fn prepare(binding: &Binding, message: &Message) -> Result<Option<Message>> {
    if message.from != binding.dispatcher.context.registration.role
        || message.to != binding.worker.context.registration.role
    {
        return Ok(None);
    }
    validate(binding.config.worker_constraint.as_deref())?;
    let Some(content) = binding
        .config
        .worker_constraint
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    else {
        return Ok(None);
    };
    let constraint = Message {
        id: format!("constraint-{}", uuid::Uuid::new_v4()),
        from: message.from.clone(),
        to: message.to.clone(),
        reply_to: Some(message.id.clone()),
        content: content.into(),
    };
    constraint.validate()?;
    Ok(Some(constraint))
}

pub(super) fn attempt(
    binding: &Binding,
    message: Message,
    transport: &impl Transport,
) -> ConstraintAttempt {
    // Reuse the full checks for B, bypassing only send's automatic-followup orchestration.
    let result = super::route(binding, &message, transport)
        .and_then(|recipient| super::do_send(binding, recipient, &message, None, true, transport));
    let outcome = outcome(result);
    ConstraintAttempt { message, outcome }
}

fn outcome(result: Result<Receipt<'_>>) -> ConstraintOutcome {
    match result {
        Ok(receipt) => ConstraintOutcome::Accepted {
            native_turn: receipt.native_turn,
            native_prompt: receipt.native_prompt,
        },
        Err(error) => failed(error.to_string()),
    }
}
fn failed(error: String) -> ConstraintOutcome {
    if error.starts_with(SUBMISSION_UNCONFIRMED) {
        ConstraintOutcome::Unknown { error }
    } else {
        ConstraintOutcome::Failed { error }
    }
}
