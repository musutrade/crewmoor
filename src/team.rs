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
    /// Existing App/core target file (`app::Binding`); core teams only.
    pub app_target: Option<PathBuf>,
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
    /// App tab for this role only (core teams); overrides the app_target `tab_id`.
    pub tab_id: Option<String>,
}
impl Team {
    pub fn validate(&self) -> Result<()> {
        crate::config::name(&self.team)?;
        model::identifier(&self.task)?;
        crate::config::absolute(&self.feedback_registry)?;
        self.validate_target()?;
        self.validate_names()?;
        self.validate_tabs()?;
        let mut names: Vec<_> = self.roles.iter().map(|r| r.role.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        if names.len() != self.roles.len() || names.len() < 2 {
            return Err("team needs at least two distinct roles".into());
        }
        self.role(&self.dispatcher).map(|_| ())
    }
    /// TUI teams use an existing host; core teams use an existing App/core target.
    fn validate_target(&self) -> Result<()> {
        match (&self.runtime, &self.host, &self.app_target) {
            (Runtime::Tui, Some(_), None) => Ok(()),
            (Runtime::Core, None, Some(path)) => crate::config::absolute(path),
            (Runtime::Tui, _, _) => Err("runtime=tui team needs host and no app_target".into()),
            (Runtime::Core, _, _) => Err("runtime=core team needs app_target and no host".into()),
        }
    }
    /// Role names become path components: same rule as team names, checked before any write.
    fn validate_names(&self) -> Result<()> {
        for role in &self.roles {
            crate::config::name(&role.role)?;
        }
        crate::config::name(&self.dispatcher)
    }
    /// Per-role tabs place core terminals in the existing App only.
    fn validate_tabs(&self) -> Result<()> {
        if self.runtime != Runtime::Core && self.roles.iter().any(|r| r.tab_id.is_some()) {
            return Err("role tab_id needs runtime=core".into());
        }
        Ok(())
    }
    /// The App target with this role's tab; the target must name its space.
    fn role_app(&self, role: &Role) -> Result<Option<crate::app::Binding>> {
        let Some(tab) = &role.tab_id else {
            return Ok(None);
        };
        let mut app = self.app()?;
        if app.space_id.is_none() {
            return Err("role tab_id needs space_id and tab_id in app_target".into());
        }
        app.tab_id = Some(tab.clone());
        app.validate(&self.runtime)?;
        Ok(Some(app))
    }
    /// With per-role tabs, every role's effective tab (its own or the target's) must
    /// exist in the App layout before the team directory is written or any role runs.
    fn check_role_tabs(
        &self,
        layout: &mut impl FnMut(&crate::app::Binding) -> Result<Value>,
    ) -> Result<()> {
        if self.roles.iter().all(|r| r.tab_id.is_none()) {
            return Ok(());
        }
        let target = self.app()?;
        let mut apps = Vec::new();
        for role in &self.roles {
            apps.push(self.role_app(role)?.unwrap_or_else(|| target.clone()));
        }
        let doc = layout(&target)?;
        for app in &apps {
            crate::app::existing_layout(app, &doc)?;
        }
        Ok(())
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
    fn app(&self) -> Result<crate::app::Binding> {
        let path = self.app_target.as_ref().ok_or("team app_target missing")?;
        let app: crate::app::Binding = toml::from_str(&fs::read_to_string(path)?)?;
        app.validate(&self.runtime)?;
        Ok(app)
    }
    /// An App record binds `<store>/<team>.json`; each role keeps its own store.
    fn store(&self, role: &Role, dir: &Path) -> PathBuf {
        match self.runtime {
            Runtime::Tui => dir.into(),
            Runtime::Core => role_dir(dir, &role.role),
        }
    }
    fn launch_config(&self, role: &Role, dir: &Path, instance: &str) -> Result<Config> {
        let app = self.role_app(role)?;
        let app_target = app.is_none().then(|| self.app_target.clone()).flatten();
        Ok(Config {
            team: self.team.clone(),
            role: role.role.clone(),
            runtime: self.runtime.clone(),
            cwd: role.cwd.clone(),
            store: self.store(role, dir),
            endpoint_dir: None,
            runtime_binary: self.runtime_binary.clone(),
            runtime_version: self.runtime_version.clone(),
            agent: role.agent.clone(),
            agent_executable: role.agent_executable.clone(),
            agent_version: role.agent_version.clone(),
            agent_args: role.agent_args.clone(),
            app,
            app_target,
            host: self.host.clone(),
            feedback_instance: Some(instance.into()),
            initial_prompt: role.initial_prompt.clone(),
        })
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
/// `layout` reads the App layout document; it is called only for per-role tabs.
pub fn start(
    team: Team,
    state: &Path,
    layout: &mut impl FnMut(&crate::app::Binding) -> Result<Value>,
    launch: &mut impl FnMut(Config, &Path) -> Result<Record>,
) -> Result<Value> {
    team.validate()?;
    team.check_role_tabs(layout)?;
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
    let config = team.launch_config(role, dir, &context.instance)?;
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
    record_binding(&team, &path, &binding)?;
    Ok(
        json!({"binding":path,"dispatcher_session":binding.dispatcher.native_session,
        "worker_session":binding.worker.native_session}),
    )
}
/// The binding, then the worker's reference to it so its Hook callback can forward, then
/// the dispatcher's reference (first bind wins) so its own Stop can deliver the next issue.
fn record_binding(team: &Team, path: &Path, binding: &Binding) -> Result<()> {
    model::write_new(path, &serde_json::to_vec_pretty(binding)?)?;
    feedback::model::write_binding_ref(
        &team.feedback_registry,
        &binding.worker.context.instance,
        path,
    )?;
    feedback::model::write_dispatcher_binding_ref(
        &team.feedback_registry,
        &binding.dispatcher.context.instance,
        path,
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
/// Socket, core id and core version for the message binding.
type Endpoint = (PathBuf, Option<String>, Option<String>);
fn endpoint(team: &Team) -> Result<Endpoint> {
    if team.runtime == Runtime::Tui {
        return Ok((team.host()?.socket.clone(), None, None));
    }
    let app = team.app()?;
    // Same version form as the core status check: `hc 0.1.15` reports `0.1.15`.
    let version = team.runtime_version.strip_prefix("hc ");
    let version = version.unwrap_or(&team.runtime_version);
    Ok((app.socket, Some(app.core_id), Some(version.into())))
}
fn message_config(team: &Team, dir: &Path, worker: &str) -> Result<message::Config> {
    let (socket, core_id, core_version) = endpoint(team)?;
    Ok(message::Config {
        runtime: team.runtime.clone(),
        core_id,
        core_version,
        runtime_binary: team.runtime_binary.clone(),
        socket,
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
    if !same_resource(&record, member) {
        return Err("bound Agent is not the resource launched by this team".into());
    }
    Ok(())
}
/// Core terminals have no pane ids: the launched generation and Agent process identify them.
fn same_resource(record: &Record, member: &message::Member) -> bool {
    if let Some(core) = &member.core {
        return record.created_ms == Some(core.created_ms)
            && record.handle.as_deref() == Some(member.handle.as_str())
            && record
                .agent_process
                .as_ref()
                .is_none_or(|p| p == &core.process);
    }
    record.terminal_id == member.terminal_id
        && record.workspace_id == member.workspace_id
        && (record.agent_process.is_none() || record.agent_process == member.process)
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
        &mut crate::app::layout,
        &mut crate::engine::launch_native,
    )
}
fn stop_native(root: &Path) -> Result<Record> {
    crate::engine::operate("stop", root)
}
