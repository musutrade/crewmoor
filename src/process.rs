use crate::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Identity {
    pub pid: u32,
    pub start_ticks: String,
    pub executable: PathBuf,
}
pub fn start_ticks(stat: &str) -> Result<String> {
    let (_, rest) = stat.rsplit_once(") ").ok_or("invalid /proc stat")?;
    Ok(rest
        .split_whitespace()
        .nth(19)
        .ok_or("missing process start time")?
        .into())
}
pub fn identity(pid: u32) -> Result<Identity> {
    let base = PathBuf::from(format!("/proc/{pid}"));
    Ok(Identity {
        pid,
        start_ticks: start_ticks(&fs::read_to_string(base.join("stat"))?)?,
        executable: fs::read_link(base.join("exe"))?,
    })
}
pub fn same(expected: &Identity) -> Result<()> {
    if &identity(expected.pid)? != expected {
        return Err("process changed; refusing operation".into());
    }
    Ok(())
}
