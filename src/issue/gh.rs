use crate::Result;
use serde_json::Value;
use std::{
    path::PathBuf,
    process::{Command, Output},
    time::{Duration, Instant},
};

/// One `gh` call with explicit arguments, answering its JSON output.
pub trait Gh {
    fn call(&self, args: &[String]) -> Result<Value>;
}

/// The installed `gh` CLI; its own authentication and proxy configuration apply unchanged.
pub struct Cli {
    pub program: PathBuf,
}
impl Default for Cli {
    fn default() -> Self {
        Self {
            program: "gh".into(),
        }
    }
}
impl Gh for Cli {
    fn call(&self, args: &[String]) -> Result<Value> {
        decode(Command::new(&self.program).args(args).output()?)
    }
}
/// `gh` calls sharing one total deadline; a call past it is refused, never started,
/// and a call still running at the deadline is stopped with an unknown outcome.
pub struct Bounded {
    pub program: PathBuf,
    pub deadline: Instant,
}
impl Gh for Bounded {
    fn call(&self, args: &[String]) -> Result<Value> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left == Duration::ZERO {
            return Err("gh call budget exhausted; call not started".into());
        }
        decode(crate::transport::output(
            Command::new(&self.program).args(args),
            left,
        )?)
    }
}
fn decode(output: Output) -> Result<Value> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh failed ({}): {}", output.status, stderr.trim()).into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
