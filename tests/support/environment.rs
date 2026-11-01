// A Rust-only inherited-environment probe. It prints booleans, never environment values.
use crewmoor::{Result, config::Runtime, native, record::Record};
use serde_json::{Value, json};
use std::process::Command;

pub fn dispatch(args: &[String]) -> Option<Result<()>> {
    if args.iter().any(|arg| arg == "environment-parent") {
        return Some(parent(args.last()));
    }
    if args.iter().any(|arg| arg == "environment-node") {
        return Some(node(args));
    }
    None
}
fn load(root: Option<&String>) -> Result<Record> {
    Record::load(root.ok_or("missing probe state")?.as_ref())
}
fn parent(root: Option<&String>) -> Result<()> {
    let record = load(root)?;
    let mut cmd = match record.config.runtime {
        Runtime::Core => native::command(&record),
        Runtime::Tui => native::server_command(&record)?,
    };
    let report = invoke(&mut cmd, &record, 2)?;
    println!("{report}");
    Ok(())
}
fn invoke(cmd: &mut Command, record: &Record, depth: u32) -> Result<Value> {
    let output = cmd
        .stdout(std::process::Stdio::piped())
        .arg("environment-node")
        .arg(depth.to_string())
        .arg(&record.root)
        .output()?;
    if !output.status.success() {
        return Err("environment probe child failed".into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn node(args: &[String]) -> Result<()> {
    let record = load(args.last())?;
    let depth: u32 = args.iter().rev().nth(1).ok_or("missing depth")?.parse()?;
    let child = descendant(&record, depth)?;
    let removed = removed(&record)?;
    let preserved = preserved()?;
    let report = json!({"removed": removed, "preserved": preserved, "child": child});
    println!("{report}");
    Ok(())
}
fn descendant(record: &Record, depth: u32) -> Result<Value> {
    match depth {
        0 => Ok(Value::Null),
        _ => invoke(
            &mut Command::new(std::env::current_exe()?),
            record,
            depth - 1,
        ),
    }
}
fn removed(record: &Record) -> Result<bool> {
    Ok(std::env::var("CREWMOOR_PROBE_REMOVED")?
        .split(',')
        .all(|name| expected_scope(record, name) == std::env::var_os(name)))
}
fn expected_scope(record: &Record, name: &str) -> Option<std::ffi::OsString> {
    match (&record.config.runtime, name) {
        (Runtime::Core, "HC_SESSION") => Some(record.session.clone().into()),
        (Runtime::Core, "HC_SOCKET") => Some(record.socket().into_os_string()),
        _ => None,
    }
}
fn preserved() -> Result<bool> {
    Ok(std::env::var("CREWMOOR_PROBE_PRESERVED")?
        .split(',')
        .all(|name| std::env::var(name).ok().as_deref() == Some("crewmoor-environment-fixture")))
}
