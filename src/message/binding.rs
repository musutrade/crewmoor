use super::Transport;
use crate::{
    Result,
    config::Runtime,
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
    pub agent_executable: Option<PathBuf>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_runtime")]
    pub runtime: Runtime,
    pub core_id: Option<String>,
    pub core_version: Option<String>,
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
    pub terminal_id: Option<String>,
    pub workspace_id: Option<String>,
    pub core: Option<super::core::Identity>,
    /// Hook-registered session at bind time; absent in bindings saved before #15.
    #[serde(default)]
    pub native_session: Option<String>,
    /// TUI Agent process located at bind time; absent in bindings saved before #15.
    #[serde(default)]
    pub process: Option<crate::process::Identity>,
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
            return Err("dispatcher and worker must have distinct terminal handles".into());
        }
        for path in [&self.dispatcher, &self.worker]
            .into_iter()
            .filter_map(|peer| peer.agent_executable.as_ref())
        {
            crate::config::absolute(path)?;
        }
        self.validate_core()?;
        Ok(())
    }
    fn validate_core(&self) -> Result<()> {
        if self.runtime == Runtime::Core {
            model::identifier(
                self.core_version
                    .as_deref()
                    .ok_or("core_version required")?,
            )?;
            crate::app::component(self.core_id.as_deref().ok_or("core_id required")?)?;
            for peer in [&self.dispatcher, &self.worker] {
                peer.validate_core()?;
            }
        }
        Ok(())
    }
}
impl Peer {
    fn validate_core(&self) -> Result<()> {
        crate::app::component(&self.handle)?;
        crate::config::absolute(
            self.agent_executable
                .as_deref()
                .ok_or("core Agent executable required")?,
        )
    }
}
impl Binding {
    pub fn bind(mut config: Config, transport: &impl Transport) -> Result<Self> {
        config.validate()?;
        config.socket = config.socket.canonicalize()?;
        let metadata = fs::metadata(&config.socket)?;
        super::core::instance(&config, transport)?;
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
        let value = get(&self.config, &member.handle, transport)?;
        self.verify(member, &value)?;
        super::tui::check_process(&self.config, member, transport)?;
        crate::feedback::require_unchanged(&self.config.feedback_registry, &member.context)
    }
    pub fn verify(&self, member: &Member, value: &serde_json::Value) -> Result<()> {
        if self.config.runtime == Runtime::Core {
            return super::core::verify(member, value);
        }
        if member.terminal_id.is_none() || member.workspace_id.is_none() {
            return Err("missing bound TUI location".into());
        }
        super::tui::session_matches(member, value)?;
        if text(value, "/pane_id")? != member.handle
            || value["terminal_id"].as_str() != member.terminal_id.as_deref()
            || value["workspace_id"].as_str() != member.workspace_id.as_deref()
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
    // The Agent's own SessionStart (or explicit registration) is the identity; no wait loop.
    let session = crate::feedback::registered_session(&config.feedback_registry, &context)?
        .ok_or("native session not registered yet; bind after the Agent's SessionStart")?;
    crate::feedback::require_unchanged(&config.feedback_registry, &context)?;
    let value = get(config, &peer.handle, transport)?;
    if config.runtime == Runtime::Core {
        let mut member = super::core::capture(context, peer, &value)?;
        member.native_session = Some(session);
        return Ok(member);
    }
    super::tui::capture(config, context, session, peer, &value, transport)
}
fn default_runtime() -> Runtime {
    Runtime::Tui
}
pub(super) fn get(
    config: &Config,
    handle: &str,
    transport: &impl Transport,
) -> Result<serde_json::Value> {
    match config.runtime {
        Runtime::Tui => {
            Ok(transport.call(config, &strings(&["agent", "get", handle]))?["agent"].clone())
        }
        Runtime::Core => transport.call(config, &strings(&["get", handle])),
    }
}
