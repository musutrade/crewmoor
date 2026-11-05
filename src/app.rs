//! Existing App core: official plain JSON-RPC, never CLI autostart.
use crate::{Result, config::Runtime, native::strings, process, record::Record};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::PermissionsExt,
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub session: String,
    pub socket: PathBuf,
    pub core_id: String,
    pub machine: String,
    pub space_id: Option<String>,
    pub tab_id: Option<String>,
}
impl Binding {
    pub fn validate(&self, runtime: &Runtime) -> Result<()> {
        if *runtime != Runtime::Core {
            return Err("App binding requires core".into());
        }
        crate::config::name(&self.session)?;
        component(&self.core_id)?;
        component(&self.machine)?;
        crate::config::absolute(&self.socket)?;
        check_env_program()?;
        self.validate_ids()?;
        Ok(())
    }
    fn validate_ids(&self) -> Result<()> {
        match (&self.space_id, &self.tab_id) {
            (None, None) => Ok(()),
            (Some(_), Some(_)) => self.local_ids().map(|_| ()),
            _ => Err("provide both space_id and tab_id, or omit both for project placement".into()),
        }
    }
    pub fn local_ids(&self) -> Result<(&str, &str)> {
        let space = self
            .space_id
            .as_deref()
            .ok_or("project space not resolved")?;
        let tab = self.tab_id.as_deref().ok_or("project tab not resolved")?;
        Ok((
            local_id(&self.machine, space)?,
            local_id(&self.machine, tab)?,
        ))
    }
}
pub(crate) fn component(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err("invalid App machine/local identity".into());
    }
    Ok(())
}
fn local_part<'a>(machine: &str, value: &'a str) -> &'a str {
    value.strip_prefix(&format!("{machine}/")).unwrap_or(value)
}
pub fn local_id<'a>(machine: &str, value: &'a str) -> Result<&'a str> {
    let local = local_part(machine, value);
    component(local)?;
    Ok(local)
}
pub fn connect(r: &Record) -> Result<process::Identity> {
    let value = call(r, &strings(&["server", "status"]))?;
    let app = r.config.app.as_ref().ok_or("missing App binding")?;
    if value["core"]["id"].as_str() != Some(&app.core_id) {
        return Err("configured App core identity differs".into());
    }
    crate::native::identity_from(&value)
}
pub fn call(r: &Record, args: &[String]) -> Result<Value> {
    let (method, params) = request(r, args)?;
    exchange(&r.socket(), method, params)
}
pub fn request(r: &Record, args: &[String]) -> Result<(&'static str, Value)> {
    match args {
        [server, status] if server == "server" && status == "status" => {
            Ok(("server.status", json!({})))
        }
        [document] if document == "app-document" => Ok(("doc.get", json!({"name":"herdr-app"}))),
        [action, terminal] => terminal_request(action, terminal),
        [action, ..] if action == "run" => Ok(("terminal.run", run_params(r)?)),
        _ => Err("App mode forbids service/layout operations".into()),
    }
}
fn terminal_request(action: &str, terminal: &str) -> Result<(&'static str, Value)> {
    let method = match action {
        "get" => "terminal.get",
        "kill" => "terminal.kill",
        "rm" => "terminal.rm",
        _ => return Err("App mode forbids service/layout operations".into()),
    };
    if terminal.contains('/') {
        return Err("App operation must address the local owned terminal".into());
    }
    Ok((method, json!({"terminal":terminal})))
}
fn run_params(r: &Record) -> Result<Value> {
    let app = r.config.app.as_ref().ok_or("missing App binding")?;
    let (space, tab) = app.local_ids()?;
    let mut labels = json!({"crewmoor-owner":r.owner,"crewmoor-team":r.config.team,
        "herdr.space":space,"herdr.tab":tab});
    if let Some(project) = &r.project {
        labels["crewmoor-project"] = json!(project.id);
    }
    let mut env = json!({});
    if let Some(instance) = &r.config.feedback_instance {
        env[crate::feedback::INSTANCE_ENV] = json!(instance);
    }
    Ok(
        json!({"argv":crate::core::launch_argv(r),"cwd":r.config.cwd,"name":r.config.role,
        "keep":true,"no_parent":true,"no_notify":true,"wait":false,"env":env,
        "labels":labels}),
    )
}
/// The App layout document from the target's socket, read-only (`doc.get`).
pub fn layout(app: &Binding) -> Result<Value> {
    exchange(&app.socket, "doc.get", json!({"name":"herdr-app"}))
}
pub fn exchange(socket: &std::path::Path, method: &str, params: Value) -> Result<Value> {
    exchange_within(socket, method, params, Duration::from_secs(10))
}
pub fn exchange_within(
    socket: &std::path::Path,
    method: &str,
    params: Value,
    limit: Duration,
) -> Result<Value> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(limit))?;
    stream.set_write_timeout(Some(limit))?;
    let request = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
    stream.write_all(format!("{request}\n").as_bytes())?;
    let mut line = String::new();
    BufReader::new(stream)
        .take(1_048_576)
        .read_line(&mut line)?;
    decode(serde_json::from_str(&line)?)
}
pub fn decode(reply: Value) -> Result<Value> {
    if reply["jsonrpc"] != "2.0" || reply["id"] != 1 {
        return Err("invalid core RPC envelope".into());
    }
    if let Some(error) = reply.get("error") {
        return Err(Box::new(crate::failure::NativeFailure {
            code: error["data"]["kind"].as_str().unwrap_or("unknown").into(),
            message: error["message"]
                .as_str()
                .unwrap_or("core RPC failed")
                .into(),
        }));
    }
    Ok(reply
        .get("result")
        .ok_or("missing core RPC result")?
        .clone())
}
/// Strip inherited launcher context in the new child, never alter the existing core.
/// Native core terminal/session routing, authentication, tools and proxies are retained.
pub const CHILD_MARKERS: &[&str] = &[
    "HERDR_SOCKET_PATH",
    "HERDR_SESSION",
    "HERDR_CONFIG_PATH",
    "HERDR_PANE_ID",
    "HERDR_TAB_ID",
    "HERDR_WORKSPACE_ID",
    "HERDR_ENV",
    crate::feedback::INSTANCE_ENV,
    "CODEX_THREAD_ID",
    "CODEX_SESSION_ID",
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
pub const ENV_PROGRAM: &str = "/usr/bin/env";
fn check_env_program() -> Result<()> {
    let meta = std::fs::metadata(ENV_PROGRAM)?;
    if !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
        return Err("App child environment executable unavailable".into());
    }
    Ok(())
}
/// core 0.1.17 env is a string overlay, so remove keys with env's direct exec argv.
pub fn child_argv(config: &crate::config::Config) -> Vec<String> {
    let mut argv = child_prefix(config.feedback_instance.is_some());
    argv.extend(config.argv());
    argv
}
/// Only this fixed prefix is recognized when binding the resulting core terminal.
pub fn child_prefix(feedback: bool) -> Vec<String> {
    let mut argv = vec![ENV_PROGRAM.into()];
    for key in CHILD_MARKERS {
        if *key != crate::feedback::INSTANCE_ENV || !feedback {
            argv.extend(["-u".into(), (*key).into()]);
        }
    }
    argv.push("--".into());
    argv
}
pub fn check_host(r: &Record, status: &Value) -> Result<()> {
    let Some(app) = &r.config.app else {
        return Ok(());
    };
    if status["core"]["id"].as_str() != Some(&app.core_id) {
        return Err("App core changed".into());
    }
    let server = r.server.as_ref().ok_or("missing external server")?;
    check_host_processes(server, status)
}
/// Reuse the launch host guard for read-only generation before emitting a configuration.
pub(crate) fn check_host_processes(server: &process::Identity, status: &Value) -> Result<()> {
    process::same(server)?;
    for holder in crate::holders::read(status)? {
        process::same(&holder.process)?;
    }
    process::same(server)
}
pub fn check_layout(r: &mut Record, runner: &impl crate::native::Runner) -> Result<()> {
    let Some(app) = r.config.app.clone() else {
        return Ok(());
    };
    let doc = runner.call(r, &strings(&["app-document"]))?;
    if app.space_id.is_none() && app.tab_id.is_none() {
        return crate::project::prepare(r, &app, &doc);
    }
    existing_layout(&app, &doc)
}
/// Explicit placement requires an existing space; App adoption may create its tab.
pub fn existing_layout(app: &Binding, doc: &Value) -> Result<()> {
    let (space_id, _) = app.local_ids()?;
    let spaces = doc["value"]["spaces"]
        .as_array()
        .ok_or("App layout unavailable; request withheld")?;
    let space = spaces
        .iter()
        .find(|s| s["id"].as_str() == Some(space_id))
        .ok_or("configured App space does not exist; request withheld")?;
    // App's SharedSpace defaults omitted tabs to an empty list.
    if let Some(tabs) = space.get("tabs") {
        tabs.as_array().ok_or("App tabs unavailable")?;
    }
    Ok(())
}
