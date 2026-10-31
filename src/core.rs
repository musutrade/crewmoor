use crate::{
    Result,
    native::{Runner, pid, strings, text},
    process,
    record::Record,
};
use serde_json::Value;

pub fn status(r: &Record, runner: &impl Runner) -> Result<Value> {
    let value = runner.call(r, &strings(&["server", "status"]))?;
    if value.get("version").and_then(Value::as_str) != r.config.runtime_version.strip_prefix("hc ")
    {
        return Err("core server version mismatch".into());
    }
    Ok(value)
}
pub fn create(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = status(r, runner)?;
    r.container_id = Some(text(&value, "/core/id")?);
    r.phase = "agent-request-intent".into();
    r.save()?;
    let value = runner.call(r, &launch_args(r))?;
    r.accepted = Some(true);
    r.observations.push(value.clone());
    r.save()?;
    r.handle = Some(text(&value, "/address")?);
    r.save()
}
pub fn launch_args(r: &Record) -> Vec<String> {
    let mut args = strings(&[
        "run",
        "--bg",
        "--no-parent",
        "--no-notify",
        "--name",
        &r.config.role,
        "--cwd",
    ]);
    args.push(r.config.cwd.display().to_string());
    args.extend(strings(&[
        "--label",
        &format!("crewmoor-owner={}", r.owner),
        "--label",
        &format!("crewmoor-team={}", r.config.team),
        "--",
    ]));
    args.extend(r.config.argv());
    args
}
pub fn target(r: &Record, runner: &impl Runner) -> Result<Value> {
    let value = fetch_target(r, runner)?;
    verify_target(r, &value)?;
    Ok(value)
}
pub fn inspect_target(r: &mut Record, runner: &impl Runner) -> Result<Value> {
    let value = fetch_target(r, runner)?;
    verify_identity(r, &value)?;
    bind_generation(r, &value)?;
    verify_target(r, &value)?;
    Ok(value)
}
fn bind_generation(r: &mut Record, value: &Value) -> Result<()> {
    if r.created_ms.is_some() {
        return Ok(());
    }
    if r.accepted != Some(true) {
        return Err("unconfirmed request cannot establish a new generation".into());
    }
    r.created_ms = Some(
        value["created_ms"]
            .as_u64()
            .ok_or("missing creation identity")?,
    );
    r.save()
}
fn fetch_target(r: &Record, runner: &impl Runner) -> Result<Value> {
    let value = status(r, runner)?;
    if r.container_id.as_deref() != Some(text(&value, "/core/id")?.as_str()) {
        return Err("core instance changed".into());
    }
    let handle = r.handle.as_deref().ok_or("missing terminal handle")?;
    let value = runner.call(r, &strings(&["get", handle]))?;
    Ok(value)
}
pub fn verify_target(r: &Record, value: &Value) -> Result<()> {
    verify_identity(r, value)?;
    if value["created_ms"].as_u64() != r.created_ms {
        return Err("terminal generation changed".into());
    }
    if r.created_ms.is_none() {
        return Err("terminal generation unknown".into());
    }
    Ok(())
}
fn verify_identity(r: &Record, value: &Value) -> Result<()> {
    if Some(text(value, "/address")?) != r.handle {
        return Err("terminal handle mismatch".into());
    }
    if value["labels"]["crewmoor-owner"].as_str() != Some(&r.owner) {
        return Err("terminal owner mismatch".into());
    }
    if value["labels"]["crewmoor-team"].as_str() != Some(&r.config.team) {
        return Err("team mismatch".into());
    }
    verify_command(r, value)
}
fn verify_command(r: &Record, value: &Value) -> Result<()> {
    if value["name"].as_str() != Some(&r.config.role) {
        return Err("role mismatch".into());
    }
    if value["cwd"].as_str() != r.config.cwd.to_str() {
        return Err("cwd mismatch".into());
    }
    if value["argv"] != serde_json::to_value(r.config.argv())? {
        return Err("program changed".into());
    }
    Ok(())
}
pub fn observe(r: &mut Record, value: Value) -> Result<()> {
    r.ready = readiness(&value);
    if value["state"].as_str() == Some("live") {
        r.agent_process = process::identity(pid(&value, "/pid")?).ok();
    }
    r.observations.push(value);
    Ok(())
}
pub fn readiness(value: &Value) -> String {
    if value["state"].as_str() != Some("live") {
        return "unknown".into();
    }
    if value["reported"].as_bool() != Some(true) {
        return "unknown".into();
    }
    if value["status"]["app"].as_str() != Some("codex") {
        return "unknown".into();
    }
    value["status"]["state"]
        .as_str()
        .unwrap_or("unknown")
        .into()
}
pub fn stop(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = target(r, runner)?;
    exclusive(r, runner)?;
    if value["state"].as_str() == Some("live") {
        runner.call(
            r,
            &strings(&["kill", r.handle.as_deref().ok_or("missing handle")?]),
        )?;
    }
    finish_terminal(r, runner)
}
fn finish_terminal(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = target(r, runner)?;
    if value["state"].as_str() != Some("finished") {
        return Err("terminal exit unconfirmed".into());
    }
    r.observations.push(value);
    runner.call(
        r,
        &strings(&["rm", r.handle.as_deref().ok_or("missing handle")?]),
    )?;
    r.phase = "terminal-removed".into();
    r.save()?;
    runner.call(r, &strings(&["server", "stop"]))?;
    Ok(())
}
fn exclusive(r: &Record, runner: &impl Runner) -> Result<()> {
    let value = runner.call(r, &strings(&["ls", "--all"]))?;
    let terminals = value["terminals"]
        .as_array()
        .ok_or("missing terminal inventory")?;
    if terminals.len() != 1 {
        return Err("private core contains additional resources; refusing stop".into());
    }
    verify_target(r, &terminals[0])
}
