use super::Transport;
use crate::{
    Result,
    feedback::model::{self, Context},
    native::{strings, text},
};
use serde::{Deserialize, Serialize};
use std::{fs, os::unix::fs::MetadataExt, path::PathBuf};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub instance: String,
    pub handle: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub runtime_binary: PathBuf,
    pub socket: PathBuf,
    pub feedback_registry: PathBuf,
    pub dispatcher: Peer,
    pub worker: Peer,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub context: Context,
    pub handle: String,
    pub terminal_id: String,
    pub workspace_id: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub config: Config,
    pub team: String,
    pub socket_device: u64,
    pub socket_inode: u64,
    pub dispatcher: Member,
    pub worker: Member,
}
impl Config {
    fn validate(&self) -> Result<()> {
        for path in [&self.runtime_binary, &self.socket, &self.feedback_registry] {
            crate::config::absolute(path)?;
        }
        if self.dispatcher.handle == self.worker.handle {
            return Err("dispatcher and worker must have distinct panes".into());
        }
        Ok(())
    }
}
impl Binding {
    pub fn bind(mut config: Config, transport: &impl Transport) -> Result<Self> {
        config.validate()?;
        config.socket = config.socket.canonicalize()?;
        let metadata = fs::metadata(&config.socket)?;
        let dispatcher = capture(&config, &config.dispatcher, transport)?;
        let worker = capture(&config, &config.worker, transport)?;
        same_team(&dispatcher.context, &worker.context)?;
        let result = Self {
            team: dispatcher.context.registration.team.clone(),
            config,
            socket_device: metadata.dev(),
            socket_inode: metadata.ino(),
            dispatcher,
            worker,
        };
        result.guard()?;
        Ok(result)
    }
    pub fn member(&self, role: &str) -> Result<&Member> {
        [&self.dispatcher, &self.worker]
            .into_iter()
            .find(|member| member.context.registration.role == role)
            .ok_or_else(|| "role is not in this fixed pair".into())
    }
    pub fn guard(&self) -> Result<()> {
        let metadata = fs::metadata(&self.config.socket)?;
        if (metadata.dev(), metadata.ino()) != (self.socket_device, self.socket_inode) {
            return Err("container socket instance changed; refusing message".into());
        }
        Ok(())
    }
    pub fn check(&self, member: &Member, transport: &impl Transport) -> Result<()> {
        let value = transport.call(&self.config, &strings(&["agent", "get", &member.handle]))?;
        self.verify(member, &value["agent"])
    }
    pub fn verify(&self, member: &Member, value: &serde_json::Value) -> Result<()> {
        session(&member.context, value)?;
        if text(value, "/pane_id")? != member.handle
            || text(value, "/terminal_id")? != member.terminal_id
            || text(value, "/workspace_id")? != member.workspace_id
        {
            return Err("bound Agent location changed".into());
        }
        Ok(())
    }
}
fn same_team(dispatcher: &Context, worker: &Context) -> Result<()> {
    if dispatcher.registration.team != worker.registration.team {
        return Err("both roles must belong to the same team".into());
    }
    if dispatcher.registration.role == worker.registration.role {
        return Err("dispatcher and worker must have distinct roles".into());
    }
    Ok(())
}
fn capture(config: &Config, peer: &Peer, transport: &impl Transport) -> Result<Member> {
    let context = model::load(&config.feedback_registry, &peer.instance)?;
    let value = transport.call(config, &strings(&["agent", "get", &peer.handle]))?;
    let agent = &value["agent"];
    session(&context, agent)?;
    let handle = text(agent, "/pane_id")?;
    if peer.handle != handle {
        return Err("use the exact returned pane handle".into());
    }
    Ok(Member {
        context,
        handle,
        terminal_id: text(agent, "/terminal_id")?,
        workspace_id: text(agent, "/workspace_id")?,
    })
}
fn session(context: &Context, value: &serde_json::Value) -> Result<()> {
    let expected = context
        .registration
        .native_session_id
        .as_deref()
        .ok_or("message binding requires an explicit native session id")?;
    if text(value, "/agent")? != context.registration.agent.name()
        || text(value, "/agent_session/kind")? != "id"
        || text(value, "/agent_session/value")? != expected
    {
        return Err("bound Agent kind or native session changed".into());
    }
    Ok(())
}
