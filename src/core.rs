use crate::{
    Result, holders,
    native::{Runner, pid, strings, text},
    process,
    record::Record,
};
use serde_json::Value;

pub fn status(r: &Record, runner: &impl Runner) -> Result<Value> {
    let value = runner.call(r, &strings(&["server", "status"]))?;
    if value["version"].as_str() != r.config.runtime_version.strip_prefix("hc ") {
        return Err("core server version mismatch".into());
    }
    if Some(process::identity(pid(&value, "/pid")?)?) != r.server {
        return Err("native core server changed".into());
    }
    Ok(value)
}
pub fn create(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = status(r, runner)?;
    crate::app::check_host(r, &value)?;
    crate::app::check_layout(r, runner)?;
    r.container_id = Some(text(&value, "/core/id")?);
    request_agent(r, runner)?;
    capture_holders(r, runner)
}
fn request_agent(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let args = launch_args(r)?;
    r.request_issued = true;
    r.phase = "agent-request-intent".into();
    r.save()?;
    let value = runner.call(r, &args)?;
    r.accepted = Some(true);
    let handle = text(&value, "/address")?;
    if let Some(app) = &r.config.app {
        r.app_terminal = Some(format!("{}/{handle}", app.machine));
    }
    r.handle = Some(handle);
    r.save()
}
fn capture_holders(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = instance_status(r, runner)?;
    r.core_holders = Some(holders::read(&value)?);
    r.save()
}
fn instance_status(r: &Record, runner: &impl Runner) -> Result<Value> {
    let value = status(r, runner)?;
    if r.container_id.as_deref() != Some(text(&value, "/core/id")?.as_str()) {
        return Err("core instance changed".into());
    }
    Ok(value)
}
pub fn launch_args(r: &Record) -> Result<Vec<String>> {
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
    ]));
    if let Some(app) = &r.config.app {
        let (space, tab) = app.local_ids()?;
        args.extend(strings(&[
            "--keep",
            "--label",
            &format!("herdr.space={space}"),
            "--label",
            &format!("herdr.tab={tab}"),
        ]));
    }
    if let Some(project) = &r.project {
        args.extend(strings(&[
            "--label",
            &format!("crewmoor-project={}", project.id),
        ]));
    }
    args.push("--".into());
    args.extend(r.config.argv());
    Ok(args)
}
pub fn target(r: &Record, runner: &impl Runner) -> Result<Value> {
    let handle = r.handle.as_deref().ok_or("missing returned terminal")?;
    let value = runner.call(r, &strings(&["get", handle]))?;
    verify_target(r, &value)?;
    Ok(value)
}
pub fn inspect_target(r: &mut Record, runner: &impl Runner) -> Result<Value> {
    let handle = r.handle.as_deref().ok_or("missing returned terminal")?;
    let value = runner.call(r, &strings(&["get", handle]))?;
    verify_identity(r, &value)?;
    if r.created_ms.is_none() {
        r.created_ms = Some(value["created_ms"].as_u64().ok_or("missing generation")?);
        r.save()?;
    }
    verify_target(r, &value)?;
    Ok(value)
}
pub fn verify_target(r: &Record, value: &Value) -> Result<()> {
    verify_identity(r, value)?;
    let generation = r.created_ms.ok_or("unknown terminal generation")?;
    if value["created_ms"].as_u64() != Some(generation) {
        return Err("terminal generation changed".into());
    }
    Ok(())
}
fn verify_identity(r: &Record, value: &Value) -> Result<()> {
    if Some(text(value, "/address")?) != r.handle {
        return Err("terminal changed".into());
    }
    if value["labels"]["crewmoor-owner"].as_str() != Some(&r.owner) {
        return Err("terminal owner changed".into());
    }
    if value["labels"]["crewmoor-team"].as_str() != Some(&r.config.team) {
        return Err("terminal team changed".into());
    }
    if value["argv"] != serde_json::to_value(r.config.argv())? {
        return Err("terminal launch argv changed".into());
    }
    if value["cwd"].as_str() != r.config.cwd.to_str() {
        return Err("terminal cwd changed".into());
    }
    Ok(())
}
pub fn observe(r: &mut Record, value: Value) -> Result<()> {
    r.native_name = crate::native::optional_text(&value, "name")?.map(Into::into);
    let current = observed_process(r, &value)?;
    r.process_verified = process::remember(&mut r.agent_process, current)?;
    Ok(())
}
fn observed_process(r: &Record, value: &Value) -> Result<Option<process::Identity>> {
    if value["pid"].is_null() || value["state"] != "live" {
        return Ok(None);
    }
    Ok(Some(process::agent_identity(
        pid(value, "/pid")?,
        &r.config.agent_executable,
        &r.config.cwd,
    )?))
}
pub fn stop(r: &mut Record, runner: &impl Runner) -> Result<bool> {
    if !r.server_owned && r.handle.is_none() {
        return Err("App target unknown; terminal binding retained".into());
    }
    if r.handle.is_some() {
        stop_target(r, runner)?;
    }
    if !r.server_owned {
        return Ok(false);
    }
    stop_empty(r, runner)
}
fn stop_target(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = target(r, runner)?;
    let handle = r
        .handle
        .as_deref()
        .ok_or("missing returned terminal")?
        .to_owned();
    r.ending_process = r.agent_process.clone();
    r.save()?;
    if value["state"] == "live" {
        runner.call(r, &strings(&["kill", &handle]))?;
    }
    finish_terminal(r, &handle, runner)
}
fn finish_terminal(r: &mut Record, handle: &str, runner: &impl Runner) -> Result<()> {
    let finished = target(r, runner)?;
    if finished["state"] != "finished" {
        return Err("terminal exit unknown".into());
    }
    runner.call(r, &strings(&["rm", handle]))?;
    runner.absent(r, &strings(&["get", handle]))?;
    crate::engine::confirm_target_exit(r)?;
    r.phase = "target-removed".into();
    r.save()
}
fn stop_empty(r: &mut Record, runner: &impl Runner) -> Result<bool> {
    let value = runner.call(r, &strings(&["ls", "--all"]))?;
    let list = value["terminals"]
        .as_array()
        .ok_or("missing terminal inventory")?;
    if !list.is_empty() {
        return Ok(false);
    }
    capture_holders(r, runner)?;
    r.guard()?;
    runner.call(r, &strings(&["server", "stop"]))?;
    Ok(true)
}
