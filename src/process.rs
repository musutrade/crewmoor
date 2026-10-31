use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
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
pub fn agent_identity(pid: u32, executable: &Path, cwd: &Path) -> Result<Identity> {
    let result = identity(pid)?;
    if result.executable != executable.canonicalize()? {
        return Err("Agent executable changed".into());
    }
    verify_cwd(&result, cwd)?;
    Ok(result)
}
pub fn verify_cwd(expected: &Identity, cwd: &Path) -> Result<()> {
    same(expected)?;
    if fs::read_link(format!("/proc/{}/cwd", expected.pid))? != cwd.canonicalize()? {
        return Err("Agent cwd changed".into());
    }
    same(expected)
}
pub fn remember(expected: &mut Option<Identity>, current: Option<Identity>) -> Result<bool> {
    let Some(current) = current else {
        return Ok(false);
    };
    if let Some(previous) = expected.as_ref()
        && previous != &current
    {
        return Err("Agent process replaced; refusing adoption".into());
    }
    *expected = Some(current);
    Ok(true)
}

pub fn exited(expected: &Identity) -> Result<bool> {
    let path = format!("/proc/{}/stat", expected.pid);
    match fs::read_to_string(path) {
        Ok(stat) => ended_stat(expected, &stat),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}
pub fn ended_stat(expected: &Identity, stat: &str) -> Result<bool> {
    if start_ticks(stat)? != expected.start_ticks {
        return Ok(true);
    }
    let (_, rest) = stat.rsplit_once(") ").ok_or("invalid process stat")?;
    Ok(matches!(rest.split_whitespace().next(), Some("Z" | "X")))
}
pub fn wait_exited(expected: &Identity) -> Result<()> {
    for _ in 0..100 {
        if exited(expected)? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Err(format!(
        "process {} exit unconfirmed; binding retained",
        expected.pid
    )
    .into())
}
