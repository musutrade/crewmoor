//! Existing App core: official plain JSON-RPC, never CLI autostart.
use crate::{Result, config::Runtime, native::strings, process, record::Record};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
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
        json!({"argv":r.config.argv(),"cwd":r.config.cwd,"name":r.config.role,
        "keep":true,"no_parent":true,"no_notify":true,"wait":false,"env":env,
        "labels":labels}),
    )
}
pub fn exchange(socket: &std::path::Path, method: &str, params: Value) -> Result<Value> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
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
// Only these metadata keys are inspected; values and other environment are never reported.
pub const HOST_MARKERS: &[&str] = &[
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
pub fn check_host(r: &Record, status: &Value) -> Result<()> {
    let Some(app) = &r.config.app else {
        return Ok(());
    };
    if status["core"]["id"].as_str() != Some(&app.core_id) {
        return Err("App core changed".into());
    }
    let server = r.server.as_ref().ok_or("missing external server")?;
    clean_process(server)?;
    for holder in crate::holders::read(status)? {
        clean_process(&holder.process)?;
    }
    Ok(())
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
pub fn existing_layout(app: &Binding, doc: &Value) -> Result<()> {
    let (space_id, tab_id) = app.local_ids()?;
    let spaces = doc["value"]["spaces"]
        .as_array()
        .ok_or("App layout unavailable; request withheld")?;
    let space = spaces
        .iter()
        .find(|s| s["id"].as_str() == Some(space_id))
        .ok_or("configured App space does not exist; request withheld")?;
    let tabs = space["tabs"].as_array().ok_or("App tabs unavailable")?;
    if !tabs.iter().any(|t| t["id"].as_str() == Some(tab_id)) {
        return Err("configured App tab does not exist in this space; request withheld".into());
    }
    Ok(())
}
pub fn clean_process(identity: &process::Identity) -> Result<()> {
    process::same(identity)?;
    let bytes = std::fs::read(format!("/proc/{}/environ", identity.pid))?;
    if contaminated(&bytes) {
        return Err("App host has developer metadata; terminal request withheld".into());
    }
    process::same(identity)
}
pub fn contaminated(bytes: &[u8]) -> bool {
    bytes.split(|b| *b == 0).any(|entry| {
        let key = entry.split(|b| *b == b'=').next().unwrap_or_default();
        HOST_MARKERS.iter().any(|marker| key == marker.as_bytes())
    })
}
