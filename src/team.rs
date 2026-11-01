//! Thin team entries over existing launch (#2), feedback registration (#4) and
//! message binding (#9). One team directory per start; never reused or overwritten.
use crate::{
    Result,
    config::{AgentKind, Config, Host, Runtime},
    feedback::{self, Context, Evidence, Registration, model},
    message::{self, Binding, Peer, Transport},
    record::Record,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

/// Team configuration; `runtime` is declared once and roles cannot override it.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Team {
    pub team: String,
    pub task: String,
    pub runtime: Runtime,
    pub runtime_binary: PathBuf,
    pub runtime_version: String,
    pub host: Option<Host>,
    pub feedback_registry: PathBuf,
    pub evidence: Evidence,
    pub dispatcher: String,
    pub roles: Vec<Role>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub role: String,
    pub cwd: PathBuf,
    pub agent: AgentKind,
    pub agent_executable: PathBuf,
    pub agent_version: String,
    #[serde(default)]
    pub agent_args: Vec<String>,
    pub initial_prompt: Option<String>,
}
impl Team {
    pub fn validate(&self) -> Result<()> {
        crate::config::name(&self.team)?;
        model::identifier(&self.task)?;
        crate::config::absolute(&self.feedback_registry)?;
        if self.runtime != Runtime::Tui || self.host.is_none() {
            return Err("team start currently needs runtime=tui with an existing host".into());
        }
        let mut names: Vec<_> = self.roles.iter().map(|r| r.role.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        if names.len() != self.roles.len() || names.len() < 2 {
            return Err("team needs at least two distinct roles".into());
        }
        self.role(&self.dispatcher).map(|_| ())
    }
    pub fn role(&self, name: &str) -> Result<&Role> {
        self.roles
            .iter()
            .find(|r| r.role == name)
            .ok_or_else(|| format!("role {name} is not in this team").into())
    }
    /// Dispatcher first, so workers are never launched without it.
    fn ordered(&self) -> Vec<&Role> {
        let mut list: Vec<_> = self.roles.iter().collect();
        list.sort_by_key(|r| r.role != self.dispatcher);
        list
    }
    fn host(&self) -> Result<&Host> {
        self.host.as_ref().ok_or_else(|| "team host missing".into())
    }
    fn launch_config(&self, role: &Role, dir: &Path, instance: &str) -> Config {
        Config {
            team: self.team.clone(),
            role: role.role.clone(),
            runtime: self.runtime.clone(),
            cwd: role.cwd.clone(),
            store: dir.into(),
            endpoint_dir: None,
            runtime_binary: self.runtime_binary.clone(),
            runtime_version: self.runtime_version.clone(),
            agent: role.agent.clone(),
            agent_executable: role.agent_executable.clone(),
            agent_version: role.agent_version.clone(),
            agent_args: role.agent_args.clone(),
            app: None,
            app_target: None,
            host: self.host.clone(),
            feedback_instance: Some(instance.into()),
            initial_prompt: role.initial_prompt.clone(),
        }
    }
    fn registration(&self, role: &Role) -> Registration {
        Registration {
            team: self.team.clone(),
            role: role.role.clone(),
            task: self.task.clone(),
            cwd: role.cwd.clone(),
            agent: role.agent.clone(),
            agent_version: role.agent_version.clone(),
            evidence: self.evidence.clone(),
            native_session_id: None,
        }
    }
}
pub fn read(path: &Path) -> Result<Team> {
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}
fn role_dir(dir: &Path, role: &str) -> PathBuf {
    dir.join("roles").join(role)
}
pub fn launch_root(dir: &Path, role: &str) -> PathBuf {
    role_dir(dir, role).join("launch")
}
fn load(dir: &Path) -> Result<Team> {
    let team: Team = serde_json::from_slice(&fs::read(dir.join("team.json"))?)?;
    team.validate()?;
    Ok(team)
}
fn context(dir: &Path, role: &str) -> Result<Context> {
    Ok(serde_json::from_slice(&fs::read(
        role_dir(dir, role).join("context.json"),
    )?)?)
}
fn new_dir(path: &Path) -> Result<()> {
    fs::DirBuilder::new().mode(0o700).create(path)?;
    Ok(())
}
/// Create `<state>/teams/<team>/` and start every role once, dispatcher first.
pub fn start(
    team: Team,
    state: &Path,
    launch: &mut impl FnMut(Config, &Path) -> Result<Record>,
) -> Result<Value> {
    let dir = create(&team, state)?;
    let mut roles = Vec::new();
    for role in team.ordered() {
        roles.push(start_role(&team, role, &dir, launch).map_err(|e| {
            format!(
                "team {} role {} start failed: {e}",
                dir.display(),
                role.role
            )
        })?);
    }
    Ok(json!({"team_dir":dir,"roles":roles}))
}
fn create(team: &Team, state: &Path) -> Result<PathBuf> {
    team.validate()?;
    crate::config::absolute(state)?;
    let parent = state.join("teams");
    model::private_dir(&parent)?;
    let dir = parent.join(&team.team);
    new_dir(&dir).map_err(|e| format!("team directory exists or is unusable: {e}"))?;
    model::write_new(&dir.join("team.json"), &serde_json::to_vec_pretty(team)?)?;
    new_dir(&dir.join("roles"))?;
    Ok(dir)
}
fn start_role(
    team: &Team,
    role: &Role,
    dir: &Path,
    launch: &mut impl FnMut(Config, &Path) -> Result<Record>,
) -> Result<Value> {
    let root = role_dir(dir, &role.role);
    new_dir(&root)?;
    let context = feedback::register(&team.feedback_registry, team.registration(role))?;
    model::write_new(
        &root.join("context.json"),
        &serde_json::to_vec_pretty(&context)?,
    )?;
    let config = team.launch_config(role, dir, &context.instance);
    let record = launch(config, &launch_root(dir, &role.role))?;
    Ok(
        json!({"role":role.role,"feedback_instance":context.instance,
        "handle":record.handle,"accepted":record.accepted,
        "process_verified":record.process_verified,"phase":record.phase}),
    )
}
/// Bind one worker with the dispatcher. Errors if a session is not yet registered.
pub fn bind(dir: &Path, worker: &str, transport: &impl Transport) -> Result<Value> {
    let team = load(dir)?;
    let path = binding_path(&team, dir, worker)?;
    let binding = Binding::bind(message_config(&team, dir, worker)?, transport)?;
    for member in [&binding.dispatcher, &binding.worker] {
        launched(dir, member)?;
    }
    model::write_new(&path, &serde_json::to_vec_pretty(&binding)?)?;
    Ok(
        json!({"binding":path,"dispatcher_session":binding.dispatcher.native_session,
        "worker_session":binding.worker.native_session}),
    )
}
fn binding_path(team: &Team, dir: &Path, worker: &str) -> Result<PathBuf> {
    if worker == team.dispatcher {
        return Err("bind a worker role, not the dispatcher".into());
    }
    team.role(worker)?;
    let path = role_dir(dir, worker).join("binding.json");
    if path.exists() {
        return Err("worker binding already exists; never overwritten".into());
    }
    Ok(path)
}
fn message_config(team: &Team, dir: &Path, worker: &str) -> Result<message::Config> {
    Ok(message::Config {
        runtime: team.runtime.clone(),
        core_id: None,
        core_version: None,
        runtime_binary: team.runtime_binary.clone(),
        socket: team.host()?.socket.clone(),
        feedback_registry: team.feedback_registry.clone(),
        dispatcher: peer(dir, &team.dispatcher)?,
        worker: peer(dir, worker)?,
    })
}
fn peer(dir: &Path, role: &str) -> Result<Peer> {
    let context = context(dir, role)?;
    let record = Record::load(&launch_root(dir, role))?;
    Ok(Peer {
        instance: context.instance,
        handle: record.handle.ok_or("role has no returned pane")?,
        agent_executable: Some(record.config.agent_executable),
    })
}
/// The bound pane/process must be exactly what this team directory launched.
fn launched(dir: &Path, member: &message::Member) -> Result<()> {
    let record = Record::load(&launch_root(dir, &member.context.registration.role))?;
    let same = record.terminal_id == member.terminal_id
        && record.workspace_id == member.workspace_id
        && (record.agent_process.is_none() || record.agent_process == member.process);
    if !same {
        return Err("bound Agent is not the resource launched by this team".into());
    }
    Ok(())
}
/// Call the existing stop once for each launched role that is not already ended.
pub fn stop(dir: &Path, operate: &mut impl FnMut(&Path) -> Result<Record>) -> Result<Value> {
    let team = load(dir)?;
    let mut results = Vec::new();
    let mut failed = false;
    for role in team.ordered() {
        let result = stop_role(dir, &role.role, operate);
        failed |= result.is_err();
        let result = result.unwrap_or_else(|e| json!({"error":e.to_string()}));
        results.push(json!({"role":role.role,"result":result}));
    }
    let summary = json!({"team_dir":dir,"roles":results});
    if failed {
        return Err(format!("team stop incomplete; evidence retained: {summary}").into());
    }
    Ok(summary)
}
fn stop_role(
    dir: &Path,
    role: &str,
    operate: &mut impl FnMut(&Path) -> Result<Record>,
) -> Result<Value> {
    let root = launch_root(dir, role);
    if !root.join("record.json").exists() {
        return Ok(json!("not-launched"));
    }
    let record = Record::load(&root)?;
    if ["stopped", "target-stopped-host-retained"].contains(&record.phase.as_str()) {
        return Ok(json!(record.phase));
    }
    let stopped = operate(&root)?;
    Ok(json!(stopped.phase))
}
const USAGE: &str =
    "usage: crewmoor team start CONFIG STATE | bind TEAM_DIR WORKER | stop TEAM_DIR";
pub fn cli(args: &[String]) -> Result<()> {
    let (cmd, rest) = args.split_first().ok_or(USAGE)?;
    let value = match cmd.as_str() {
        "start" => start_cli(rest),
        "bind" => bind_cli(rest),
        "stop" => stop_cli(rest),
        _ => Err(USAGE.into()),
    }?;
    let encoded = serde_json::to_string_pretty(&value)?;
    println!("{encoded}");
    Ok(())
}
fn start_cli(args: &[String]) -> Result<Value> {
    let [config, state] = args else {
        return Err(USAGE.into());
    };
    start_native(config, state)
}
fn bind_cli(args: &[String]) -> Result<Value> {
    let [dir, worker] = args else {
        return Err(USAGE.into());
    };
    bind(Path::new(dir), worker, &message::Native)
}
fn stop_cli(args: &[String]) -> Result<Value> {
    let [dir] = args else {
        return Err(USAGE.into());
    };
    stop(Path::new(dir), &mut stop_native)
}
fn start_native(config: &str, state: &str) -> Result<Value> {
    start(
        read(Path::new(config))?,
        Path::new(state),
        &mut crate::engine::launch_native,
    )
}
fn stop_native(root: &Path) -> Result<Record> {
    crate::engine::operate("stop", root)
}
