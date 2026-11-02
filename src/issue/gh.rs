use crate::Result;
use serde_json::Value;
use std::{path::PathBuf, process::Command};

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
        let output = Command::new(&self.program).args(args).output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("gh failed ({}): {}", output.status, stderr.trim()).into());
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }
}
