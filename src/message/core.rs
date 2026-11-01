//! Existing local core only. No CLI autostart and no semantic Agent state.
use super::{Binding, Config, Member, Peer, Transport};
use crate::{
    Result,
    config::Runtime,
    feedback::Context,
    native::{pid, strings, text},
    process,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub created_ms: u64,
    pub argv: Vec<String>,
    pub process: process::Identity,
}
pub fn instance(config: &Config, transport: &impl Transport) -> Result<()> {
    if config.runtime != Runtime::Core {
        return Ok(());
    }
    let status = transport.call(config, &strings(&["server", "status"]))?;
    if status["core"]["id"].as_str() != config.core_id.as_deref() {
        return Err("configured core instance changed".into());
    }
    if status["version"].as_str() != config.core_version.as_deref() {
        return Err("configured core version changed".into());
    }
    Ok(())
}
pub fn capture(context: Context, peer: &Peer, value: &Value) -> Result<Member> {
    context
        .registration
        .native_session_id
        .as_deref()
        .ok_or("explicit native session association required")?;
    if text(value, "/address")? != peer.handle {
        return Err("use the exact local core address".into());
    }
    let identity = capture_process(&context, peer, value)?;
    let member = Member {
        context,
        handle: peer.handle.clone(),
        terminal_id: None,
        workspace_id: None,
        core: Some(identity),
    };
    verify(&member, value)?;
    Ok(member)
}
fn capture_process(context: &Context, peer: &Peer, value: &Value) -> Result<Identity> {
    let executable = peer
        .agent_executable
        .as_deref()
        .ok_or("missing core Agent executable")?;
    let identity = Identity {
        created_ms: value["created_ms"]
            .as_u64()
            .ok_or("missing terminal generation")?,
        argv: serde_json::from_value(value["argv"].clone())?,
        process: process::agent_identity(
            pid(value, "/pid")?,
            executable,
            &context.registration.cwd,
        )?,
    };
    check_argv(&identity, executable)?;
    Ok(identity)
}
fn check_argv(identity: &Identity, executable: &std::path::Path) -> Result<()> {
    let first = identity.argv.first().ok_or("missing Agent launch argv")?;
    if std::path::Path::new(first).canonicalize()? != executable.canonicalize()? {
        return Err("core launch executable differs".into());
    }
    Ok(())
}
pub fn verify(member: &Member, value: &Value) -> Result<()> {
    let identity = member
        .core
        .as_ref()
        .ok_or("missing bound core terminal identity")?;
    verify_location(member, identity, value)?;
    if value["argv"] != serde_json::to_value(&identity.argv)? {
        return Err("core terminal launch argv changed".into());
    }
    process::verify_cwd(&identity.process, &member.context.registration.cwd)
}
fn verify_location(member: &Member, identity: &Identity, value: &Value) -> Result<()> {
    if value["address"].as_str() != Some(&member.handle)
        || value["created_ms"].as_u64() != Some(identity.created_ms)
        || value["pid"].as_u64() != Some(u64::from(identity.process.pid))
        || value["cwd"].as_str() != member.context.registration.cwd.to_str()
    {
        return Err("core terminal generation/process/location changed".into());
    }
    Ok(())
}
pub fn submitted(binding: &Binding, value: &Value, transport: &impl Transport) -> Result<u64> {
    let turn = value["turn"]
        .as_u64()
        .ok_or("missing core send acceptance turn")?;
    binding.guard()?;
    instance(&binding.config, transport)?;
    binding.check(&binding.dispatcher, transport)?;
    binding.check(&binding.worker, transport)?;
    binding.guard()?;
    Ok(turn)
}
pub fn call(config: &Config, args: &[String]) -> Result<Value> {
    let (method, params) = request(args)?;
    crate::app::exchange(&config.socket, method, params)
}
fn request(args: &[String]) -> Result<(&'static str, Value)> {
    match args {
        [server, status] if server == "server" && status == "status" => {
            Ok(("server.status", json!({})))
        }
        _ => terminal_request(args),
    }
}
fn terminal_request(args: &[String]) -> Result<(&'static str, Value)> {
    match args {
        [get, terminal] if get == "get" => {
            crate::app::component(terminal)?;
            Ok(("terminal.get", json!({"terminal":terminal})))
        }
        [send, terminal, text] if send == "send" => {
            crate::app::component(terminal)?;
            Ok((
                "terminal.send",
                json!({"terminal":terminal,"text":text,"force":false,"wait":false}),
            ))
        }
        _ => Err("core messages permit only existing local status/get/send".into()),
    }
}
