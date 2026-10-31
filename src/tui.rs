use crate::{
    Result,
    native::{Runner, pid, strings, text},
    process,
    record::Record,
};
use serde_json::Value;

pub fn create(r: &mut Record, runner: &impl Runner) -> Result<()> {
    verify_server(r, runner)?;
    create_workspace(r, runner)?;
    start_agent(r, runner)
}
fn verify_server(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = runner.call(r, &strings(&["status", "server", "--json"]))?;
    if value["version"].as_str() != r.config.runtime_version.strip_prefix("herdr ") {
        return Err("TUI server version mismatch".into());
    }
    r.container_id = Some(r.session.clone());
    Ok(())
}
fn create_workspace(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = runner.call(
        r,
        &strings(&[
            "workspace",
            "create",
            "--cwd",
            &r.config.cwd.display().to_string(),
            "--label",
            &r.owner,
            "--no-focus",
        ]),
    )?;
    r.observations.push(value.clone());
    r.save()?;
    r.handle = Some(text(&value, "/root_pane/pane_id")?);
    r.terminal_id = Some(text(&value, "/root_pane/terminal_id")?);
    r.workspace_id = Some(text(&value, "/workspace/workspace_id")?);
    r.phase = "workspace-created".into();
    r.save()
}
fn start_agent(r: &mut Record, runner: &impl Runner) -> Result<()> {
    r.phase = "agent-request-intent".into();
    r.save()?;
    let value = runner.call(r, &launch_args(r)?)?;
    r.accepted = Some(true);
    r.observations.push(value);
    r.save()
}
pub fn launch_args(r: &Record) -> Result<Vec<String>> {
    let mut args = strings(&[
        "agent",
        "start",
        &r.config.role,
        "--kind",
        "codex",
        "--pane",
        r.handle.as_deref().ok_or("missing pane")?,
        "--timeout",
        "30000",
        "--",
        "--no-daemon",
    ]);
    args.extend(r.config.agent_args.clone());
    Ok(args)
}
pub fn target(r: &Record, runner: &impl Runner) -> Result<Value> {
    let handle = r.handle.as_deref().ok_or("missing pane")?;
    let value = runner.call(r, &strings(&["agent", "get", handle]))?;
    verify_target(r, &value["agent"])?;
    Ok(value)
}
pub fn verify_target(r: &Record, value: &Value) -> Result<()> {
    if value["pane_id"].as_str() != r.handle.as_deref() {
        return Err("pane changed".into());
    }
    if value["terminal_id"].as_str() != r.terminal_id.as_deref() {
        return Err("terminal changed".into());
    }
    if value["agent"].as_str() != Some("codex") {
        return Err("agent changed".into());
    }
    if value["name"].as_str() != Some(&r.config.role) {
        return Err("logical role changed".into());
    }
    Ok(())
}
pub fn observe(r: &mut Record, value: Value, runner: &impl Runner) -> Result<()> {
    r.ready = readiness(&value["agent"]);
    let info = runner.call(
        r,
        &strings(&[
            "pane",
            "process-info",
            "--pane",
            r.handle.as_deref().ok_or("missing pane")?,
        ]),
    )?;
    r.agent_process = agent_process(&info);
    r.observations.extend([value, info]);
    Ok(())
}
pub fn readiness(value: &Value) -> String {
    if value["interactive_ready"].as_bool() != Some(true) {
        return "unknown".into();
    }
    value["agent_status"].as_str().unwrap_or("unknown").into()
}
fn agent_process(info: &Value) -> Option<process::Identity> {
    let processes = info
        .pointer("/process_info/foreground_processes")?
        .as_array()?;
    let agent = processes.iter().find(|value| {
        value["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("codex"))
    })?;
    process::identity(pid(agent, "/pid").ok()?).ok()
}
pub fn stop(r: &mut Record, runner: &impl Runner) -> Result<()> {
    target(r, runner)?;
    let id = exclusive_workspace(r, runner)?;
    runner.call(r, &strings(&["workspace", "close", &id]))?;
    r.phase = "workspace-closed".into();
    r.save()?;
    runner.call(r, &strings(&["session", "stop", &r.session, "--json"]))?;
    Ok(())
}
fn exclusive_workspace(r: &Record, runner: &impl Runner) -> Result<String> {
    let value = runner.call(r, &strings(&["workspace", "list"]))?;
    let list = value["workspaces"]
        .as_array()
        .ok_or("missing workspace inventory")?;
    if list.len() != 1 {
        return Err("private TUI contains additional resources; refusing stop".into());
    }
    if list[0]["label"].as_str() != Some(&r.owner) {
        return Err("workspace ownership mismatch".into());
    }
    verify_workspace(r, &list[0])?;
    text(&list[0], "/workspace_id")
}
fn verify_workspace(r: &Record, value: &Value) -> Result<()> {
    if value["workspace_id"].as_str() != r.workspace_id.as_deref() {
        return Err("workspace changed".into());
    }
    if value["pane_count"].as_u64() != Some(1) {
        return Err("additional panes exist; refusing stop".into());
    }
    if value["tab_count"].as_u64() != Some(1) {
        return Err("additional tabs exist; refusing stop".into());
    }
    Ok(())
}
