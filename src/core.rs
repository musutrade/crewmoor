use crate::{
    Result, holders,
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
    if Some(process::identity(pid(&value, "/pid")?)?) != r.server {
        return Err("native core server process changed".into());
    }
    Ok(value)
}
pub fn create(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = status(r, runner)?;
    r.container_id = Some(text(&value, "/core/id")?);
    r.core_holders = Some(holders::read(&value)?);
    r.phase = "agent-request-intent".into();
    r.request_issued = true;
    r.save()?;
    let result = request_agent(r, runner);
    let captured = capture_holders(r, runner);
    result?;
    captured
}
fn request_agent(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = runner.call(r, &launch_args(r))?;
    r.accepted = Some(true);
    r.observations.push(value.clone());
    r.save()?;
    r.handle = Some(text(&value, "/address")?);
    r.save()
}
fn capture_holders(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = instance_status(r, runner)?;
    let current = holders::read(&value)?;
    let previous = r
        .core_holders
        .as_ref()
        .ok_or("launch holder inventory unknown")?;
    if !previous.is_empty() && previous != &current {
        return Err("launch holder changed".into());
    }
    r.core_holders = Some(current);
    r.observations.push(value);
    r.save()
}
fn instance_status(r: &Record, runner: &impl Runner) -> Result<Value> {
    let value = status(r, runner)?;
    if r.container_id.as_deref() != Some(text(&value, "/core/id")?.as_str()) {
        return Err("core instance changed or unknown".into());
    }
    Ok(value)
}
fn verify_holders(r: &Record, runner: &impl Runner) -> Result<usize> {
    let expected = r
        .core_holders
        .as_ref()
        .ok_or("holder ownership unknown; refusing stop/query")?;
    let current = holders::read(&instance_status(r, runner)?)?;
    if *expected != current {
        return Err("holder identity changed; refusing stop/query".into());
    }
    Ok(current.len())
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
    if verify_holders(r, runner)? != 1 {
        return Err("target requires one recorded current holder".into());
    }
    let handle = r.handle.as_deref().ok_or("missing terminal handle")?;
    exclusive(r, runner)?;
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
    crate::native::match_optional(value, "name", &r.config.role)?;
    if value["cwd"].as_str() != r.config.cwd.to_str() {
        return Err("cwd mismatch".into());
    }
    if value["argv"] != serde_json::to_value(r.config.argv())? {
        return Err("program changed".into());
    }
    Ok(())
}
pub fn observe(r: &mut Record, value: Value) -> Result<()> {
    r.native_name = crate::native::optional_text(&value, "name")?.map(Into::into);
    let current = observed_process(r, &value)?;
    r.process_verified = process::remember(&mut r.agent_process, current)?;
    r.observations.push(value);
    Ok(())
}
fn observed_process(r: &Record, value: &Value) -> Result<Option<process::Identity>> {
    if value["state"] != "live" || value["pid"].is_null() {
        return Ok(None);
    }
    Ok(Some(process::agent_identity(
        pid(value, "/pid")?,
        &r.config.agent_executable,
        &r.config.cwd,
    )?))
}
pub fn stop(r: &mut Record, runner: &impl Runner) -> Result<()> {
    r.guard()?;
    if r.handle.is_none() {
        r.require_uncreated()?;
        return stop_empty(r, runner);
    }
    stop_target(r, runner)
}
fn stop_target(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = target(r, runner)?;
    exclusive(r, runner)?;
    prepare_ending(r, &value)?;
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
    runner.absent(
        r,
        &strings(&["get", r.handle.as_deref().ok_or("missing handle")?]),
    )?;
    r.phase = "terminal-removed".into();
    r.save()?;
    stop_empty(r, runner)
}
fn stop_empty(r: &Record, runner: &impl Runner) -> Result<()> {
    verify_holders(r, runner)?;
    let value = runner.call(r, &strings(&["ls", "--all"]))?;
    let list = value["terminals"]
        .as_array()
        .ok_or("missing terminal inventory")?;
    if !list.is_empty() {
        return Err("private core contains resources; refusing empty-instance stop".into());
    }
    r.guard()?;
    runner.call(r, &strings(&["server", "stop"]))?;
    Ok(())
}
fn prepare_ending(r: &mut Record, value: &Value) -> Result<()> {
    r.agent_process
        .as_ref()
        .ok_or("Agent birth fingerprint unknown; refusing stop")?;
    if value["state"].as_str() == Some("live") {
        crate::engine::verified_process(r)?;
        let current = process::identity(pid(value, "/pid")?)?;
        if Some(&current) != r.agent_process.as_ref() {
            return Err("Agent process changed".into());
        }
        r.ending_process = Some(current);
    } else {
        r.ending_process = r.agent_process.clone();
    }
    r.save()
}
fn exclusive(r: &Record, runner: &impl Runner) -> Result<()> {
    let value = runner.call(r, &strings(&["ls", "--all"]))?;
    let terminals = value["terminals"]
        .as_array()
        .ok_or("missing terminal inventory")?;
    if terminals.len() != 1 {
        return Err("private core contains additional resources; refusing stop".into());
    }
    if Some(text(&terminals[0], "/address")?) != r.handle {
        return Err("inventory target changed".into());
    }
    Ok(())
}
