use super::Config;
use crate::{Result, config::Runtime, failure::NativeFailure};
use serde_json::Value;
use std::time::Duration;

pub trait Transport {
    fn call(&self, config: &Config, args: &[String]) -> Result<Value>;
}
pub struct Native;
impl Transport for Native {
    fn call(&self, config: &Config, args: &[String]) -> Result<Value> {
        let mut cmd = crate::native::endpoint_command(&config.runtime_binary, &config.socket);
        let output = crate::transport::output(cmd.args(args), Duration::from_secs(10))?;
        if !output.status.success() {
            let value = serde_json::from_slice(&output.stderr).unwrap_or(Value::Null);
            return Err(NativeFailure::decode(&value).into());
        }
        crate::native::payload(serde_json::from_slice(&output.stdout)?, &Runtime::Tui)
    }
}
