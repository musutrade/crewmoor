use crate::{
    Result,
    native::{Runner, pid, strings, text},
    process,
    record::Record,
};
use serde_json::Value;

pub fn create(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let server = runner.call(r, &strings(&["status", "server", "--json"]))?;
    if server["version"].as_str() != Some(expected_version(r)?) {
        return Err("TUI server version mismatch".into());
    }
    r.container_id = Some(container(r, &server)?);
    create_workspace(r, runner)?;
    request_agent(r, runner)
}
fn expected_version(r: &Record) -> Result<&str> {
    if let Some(host) = &r.config.host {
        return Ok(&host.server_version);
    }
    Ok(r.config
        .runtime_version
        .strip_prefix("herdr ")
        .ok_or("TUI runtime version must start with herdr")?)
}
/// Private mode: our own session. Host mode: the existing server's socket, never owned.
fn container(r: &Record, server: &Value) -> Result<String> {
    let Some(host) = &r.config.host else {
        return Ok(r.session.clone());
    };
    if server["socket"].as_str() != host.socket.to_str() {
        return Err("TUI host socket differs from configured host".into());
    }
    Ok(host.socket.display().to_string())
}
pub fn workspace_args(r: &Record) -> Vec<String> {
    let mut args = strings(&["workspace", "create", "--cwd"]);
    args.push(r.config.cwd.display().to_string());
    args.extend(strings(&["--label", &r.owner, "--no-focus"]));
    if let Some(instance) = &r.config.feedback_instance {
        args.push("--env".into());
        args.push(format!("{}={instance}", crate::feedback::INSTANCE_ENV));
    }
    args
}
fn create_workspace(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let value = runner.call(r, &workspace_args(r))?;
    r.handle = Some(text(&value, "/root_pane/pane_id")?);
    r.terminal_id = Some(text(&value, "/root_pane/terminal_id")?);
    r.workspace_id = Some(text(&value, "/workspace/workspace_id")?);
    r.phase = "workspace-created".into();
    r.save()
}
pub fn launch_args(r: &Record) -> Result<Vec<String>> {
    let pane = r.handle.as_deref().ok_or("missing returned pane")?;
    let mut args = strings(&[
        "agent",
        "start",
        &r.config.role,
        "--kind",
        r.config.agent.name(),
        "--pane",
        pane,
        "--timeout",
        "5000",
        "--",
    ]);
    args.extend(r.config.agent_args.clone());
    args.extend(r.config.initial_prompt.clone());
    Ok(args)
}
fn request_agent(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let args = launch_args(r)?;
    r.guard()?;
    r.request_issued = true;
    r.phase = "agent-request-intent".into();
    r.save()?;
    match runner.call(r, &args) {
        Ok(value) => {
            verify_target(r, &value["agent"])?;
            r.accepted = Some(true);
            r.native_name = crate::native::optional_text(&value["agent"], "name")?.map(Into::into);
        }
        Err(error) => r.request_error = Some(crate::failure::code(error.as_ref()).into()),
    }
    r.save()
}
pub fn target(r: &Record, runner: &impl Runner) -> Result<Value> {
    let pane = r.handle.as_deref().ok_or("missing returned pane")?;
    let value = runner.call(r, &strings(&["pane", "get", pane]))?;
    verify_target(r, &value["pane"])?;
    Ok(value)
}
pub fn verify_target(r: &Record, value: &Value) -> Result<()> {
    if value["pane_id"].as_str() != r.handle.as_deref() {
        return Err("pane changed".into());
    }
    if value["terminal_id"].as_str() != r.terminal_id.as_deref() {
        return Err("terminal changed".into());
    }
    if value["workspace_id"].as_str() != r.workspace_id.as_deref() {
        return Err("workspace changed".into());
    }
    Ok(())
}
pub fn observe(r: &mut Record, runner: &impl Runner) -> Result<()> {
    let pane = r.handle.as_deref().ok_or("missing returned pane")?;
    let value = runner.call(r, &strings(&["pane", "process-info", "--pane", pane]))?;
    let info = &value["process_info"];
    if info["pane_id"].as_str() != Some(pane) {
        return Err("process-info pane changed".into());
    }
    r.terminal_process = shell_process(info);
    let current = observed_agent(r, info)?;
    r.process_verified = process::remember(&mut r.agent_process, current)?;
    // Do not persist unrelated foreground argv or container semantic status.
    Ok(())
}
fn shell_process(info: &Value) -> Option<process::Identity> {
    process::identity(pid(info, "/shell_pid").ok()?).ok()
}
fn observed_agent(r: &Record, info: &Value) -> Result<Option<process::Identity>> {
    let Some(list) = info["foreground_processes"].as_array() else {
        return Ok(None);
    };
    let mut matches = list
        .iter()
        .filter(|value| value["name"] == r.config.agent.name());
    let Some(value) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err("multiple matching Agent processes; location unknown".into());
    }
    Ok(Some(process::agent_identity(
        pid(value, "/pid")?,
        &r.config.agent_executable,
        &r.config.cwd,
    )?))
}
pub fn stop(r: &mut Record, runner: &impl Runner) -> Result<bool> {
    if let Some(pane) = r.handle.clone() {
        let may_close = workspace_may_close(r, runner)?;
        close_target(r, &pane, runner)?;
        close_empty_workspace(r, runner, may_close)?;
    }
    if !r.server_owned {
        return Ok(false);
    }
    stop_empty(r, runner)
}
fn stop_empty(r: &Record, runner: &impl Runner) -> Result<bool> {
    let value = runner.call(r, &strings(&["workspace", "list"]))?;
    let list = value["workspaces"]
        .as_array()
        .ok_or("missing workspace inventory")?;
    if !list.is_empty() {
        return Ok(false);
    }
    r.guard()?;
    runner.call(r, &strings(&["session", "stop", &r.session, "--json"]))?;
    Ok(true)
}
fn workspace_may_close(r: &Record, runner: &impl Runner) -> Result<bool> {
    let id = r
        .workspace_id
        .as_deref()
        .ok_or("missing returned workspace")?;
    let value = runner.call(r, &strings(&["workspace", "get", id]))?;
    if value["workspace"]["workspace_id"].as_str() != Some(id) {
        return Err("workspace changed".into());
    }
    Ok(value["workspace"]["pane_count"].as_u64() == Some(1)
        && value["workspace"]["tab_count"].as_u64() == Some(1))
}
fn close_empty_workspace(r: &Record, runner: &impl Runner, eligible: bool) -> Result<()> {
    if !eligible {
        return Ok(());
    }
    let value = runner.call(r, &strings(&["workspace", "list"]))?;
    let list = value["workspaces"]
        .as_array()
        .ok_or("missing workspace inventory")?;
    let Some(own) = list
        .iter()
        .find(|w| w["workspace_id"].as_str() == r.workspace_id.as_deref())
    else {
        return Ok(());
    };
    if !empty_workspace(own) {
        return Ok(());
    }
    let id = r
        .workspace_id
        .as_deref()
        .ok_or("missing returned workspace")?;
    runner.call(r, &strings(&["workspace", "close", id]))?;
    Ok(())
}
fn empty_workspace(value: &Value) -> bool {
    value["pane_count"].as_u64() == Some(0)
        && value["tab_count"].as_u64().is_some_and(|count| count <= 1)
}
fn close_target(r: &mut Record, pane: &str, runner: &impl Runner) -> Result<()> {
    target(r, runner)?;
    r.ending_process = r.agent_process.clone();
    r.save()?;
    runner.call(r, &strings(&["pane", "close", pane]))?;
    runner.absent(r, &strings(&["pane", "get", pane]))?;
    crate::engine::confirm_target_exit(r)?;
    r.phase = "target-removed".into();
    r.save()
}
