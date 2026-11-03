//! Rust-only CLI double for read-only executable/version/status discovery.
//! Sidecar fields are test harness controls, never claimed as native response fields.
use crewmoor::Result;
use serde_json::Value;
use std::{fs, io::Write, path::Path};

pub fn dispatch(args: &[String]) -> Option<Result<()>> {
    let executable = std::env::current_exe().ok()?;
    let config = executable.with_extension("discovery.json");
    config.is_file().then(|| run(&config, &args[1..]))
}
fn run(path: &Path, args: &[String]) -> Result<()> {
    let config: Value = serde_json::from_slice(&fs::read(path)?)?;
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.with_extension("queries.jsonl"))?;
    let line = serde_json::to_string(args)?;
    log.write_all(format!("{line}\n").as_bytes())?;
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--version"] => version(&config),
        ["status", "server", "--json"] => status(&config),
        _ => Err("discovery unexpectedly attempted a non-query CLI command".into()),
    }
}
fn version(config: &Value) -> Result<()> {
    if config["version_error"] == true {
        return Err("synthetic version failure".into());
    }
    let version = config["version"].as_str().ok_or("missing test version")?;
    println!("{version}");
    Ok(())
}
fn status(config: &Value) -> Result<()> {
    if config["status_error"] == true {
        return Err("synthetic status failure".into());
    }
    if config["malformed_status"] == true {
        println!("invalid-json");
        return Ok(());
    }
    let status = config.get("status").ok_or("missing test status")?;
    println!("{status}");
    Ok(())
}
