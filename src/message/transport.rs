use super::Config;
use crate::{Result, config::Runtime, failure::NativeFailure};
use serde_json::Value;
use std::time::{Duration, Instant};

/// A native call refused before any CLI process was started; nothing was submitted.
#[derive(Debug)]
pub struct NotStarted;
impl std::fmt::Display for NotStarted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native call budget exhausted; call not started")
    }
}
impl std::error::Error for NotStarted {}
pub trait Transport {
    fn call(&self, config: &Config, args: &[String]) -> Result<Value>;
}
pub struct Native;
impl Transport for Native {
    fn call(&self, config: &Config, args: &[String]) -> Result<Value> {
        call_within(config, args, Duration::from_secs(10))
    }
}
/// Native calls sharing one total deadline; a call past it is refused, never started.
pub struct Bounded {
    deadline: Instant,
}
impl Bounded {
    pub fn new(budget: Duration) -> Self {
        Self::until(Instant::now() + budget)
    }
    /// Share a deadline with other bounded calls of the same callback.
    pub fn until(deadline: Instant) -> Self {
        Self { deadline }
    }
}
impl Transport for Bounded {
    fn call(&self, config: &Config, args: &[String]) -> Result<Value> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(NotStarted.into());
        }
        call_within(config, args, left)
    }
}
fn call_within(config: &Config, args: &[String], limit: Duration) -> Result<Value> {
    if config.runtime == Runtime::Core {
        return super::core::call(config, args, limit);
    }
    let mut cmd = crate::native::endpoint_command(&config.runtime_binary, &config.socket);
    let output = crate::transport::output(cmd.args(args), limit)?;
    if !output.status.success() {
        let value = serde_json::from_slice(&output.stderr).unwrap_or(Value::Null);
        return Err(NativeFailure::decode(&value).into());
    }
    crate::native::payload(serde_json::from_slice(&output.stdout)?, &Runtime::Tui)
}
