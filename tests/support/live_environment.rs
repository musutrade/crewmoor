// Read-only acceptance probe: only marker names, never values or unrelated environment entries.
use crewmoor::{Result, config::Runtime, process, record::Record};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};

const MARKERS: &[&str] = &[
    "CODEX_SESSION_ID",
    "CODEX_THREAD_ID",
    "CODEX_TOOL_CALL_ID",
    "CODEX_CI",
    "CODEX_VERSION",
    "CODEX_INTERNAL_ORIGINATOR_OVERRIDE",
    "CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "CI",
    "CONTINUOUS_INTEGRATION",
    "NO_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "FORCE_COLOR",
];
pub fn report(root: Option<&String>) -> Result<()> {
    let r = Record::load(root.ok_or("missing evidence state")?.as_ref())?;
    r.guard()?;
    let server = entry("server", r.server.as_ref().ok_or("server unknown")?)?;
    let agent = entry("agent", r.agent_process.as_ref().ok_or("agent unknown")?)?;
    let secondary = secondary(&r)?;
    let list = vec![server, agent, secondary];
    let value = json!({"session":r.session,"container":r.container_id,"processes":list});
    println!("{value}");
    Ok(())
}
fn secondary(r: &Record) -> Result<Value> {
    match r.config.runtime {
        Runtime::Tui => entry(
            "terminal-process",
            r.terminal_process
                .as_ref()
                .ok_or("terminal process unknown")?,
        ),
        Runtime::Core => {
            let list = r.core_holders.as_ref().ok_or("holder inventory unknown")?;
            if list.len() != 1 {
                return Err("single owned holder required".into());
            }
            entry("holder", &list[0].process)
        }
    }
}
fn entry(role: &str, identity: &process::Identity) -> Result<Value> {
    process::same(identity)?;
    let file = std::fs::File::open(format!("/proc/{}/environ", identity.pid))?;
    let present = marker_names(BufReader::new(file))?;
    process::same(identity)?;
    Ok(json!({"role":role,"pid":identity.pid,"present_markers":present}))
}
fn marker_names(mut input: impl BufRead) -> Result<Vec<String>> {
    let mut names = Vec::new();
    loop {
        let mut key = Vec::new();
        if input.read_until(b'=', &mut key)? == 0 {
            return Ok(names);
        }
        if key.last() != Some(&b'=') {
            return Err("invalid environment entry".into());
        }
        key.pop();
        let key = String::from_utf8(key)?;
        if MARKERS.contains(&key.as_str()) {
            names.push(key);
        }
        input.skip_until(0)?;
    }
}
