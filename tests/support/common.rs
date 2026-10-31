use crewmoor::{
    Result,
    config::{AgentKind, Config, Runtime},
    native::Runner,
    process,
    record::Record,
};
use serde_json::{Value, json};
use std::{cell::RefCell, os::unix::net::UnixListener, path::Path, process::Child};

pub fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("cm")
        .tempdir_in("/tmp")
        .unwrap()
}
pub fn config(base: &Path, runtime: Runtime) -> Config {
    Config {
        team: "test-team".into(),
        role: "probe".into(),
        runtime,
        cwd: base.into(),
        store: base.join("bindings"),
        endpoint_dir: None,
        runtime_binary: env!("CARGO_BIN_EXE_transport-fixture").into(),
        runtime_version: "fixture 1".into(),
        agent: AgentKind::Codex,
        agent_executable: env!("CARGO_BIN_EXE_transport-fixture").into(),
        agent_version: "fixture 1".into(),
        agent_args: Vec::new(),
        app: None,
    }
}
#[derive(Default)]
pub struct Fake {
    pub calls: RefCell<Vec<Vec<String>>>,
    pub listener: RefCell<Option<UnixListener>>,
    pub child: RefCell<Option<Child>>,
    pub holder: RefCell<Option<Child>>,
    pub agent: RefCell<Option<Child>>,
    pub shell: RefCell<Option<Child>>,
    pub extra: RefCell<bool>,
    pub extra_pane: RefCell<bool>,
    pub extra_tab: RefCell<bool>,
    pub workspace_closed: RefCell<bool>,
    pub leave_workspace: RefCell<bool>,
    pub fail: RefCell<bool>,
    pub ambiguous: RefCell<bool>,
    pub no_agent: RefCell<bool>,
    pub missing_process: RefCell<bool>,
    pub omit_name: RefCell<bool>,
    pub minimal_agent: RefCell<bool>,
    pub changed: RefCell<bool>,
    pub created: RefCell<bool>,
    pub removed: RefCell<bool>,
    pub closed: RefCell<bool>,
    pub finished: RefCell<bool>,
    pub stopped: RefCell<bool>,
    pub leave_actor: RefCell<bool>,
    pub leave_holder: RefCell<bool>,
    pub leave_socket: RefCell<bool>,
}
fn spawn(r: &Record) -> Result<Child> {
    use std::io::Read;
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_transport-fixture"));
    if r.config.app.is_some() {
        for marker in crewmoor::app::HOST_MARKERS {
            command.env_remove(marker);
        }
    } else {
        command.env("CODEX_SESSION_ID", "synthetic-transport-session");
    }
    let mut child = command
        .arg("hold")
        .env("CREWMOOR_TRANSPORT_HANDSHAKE", "1")
        .current_dir(&r.config.cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    let mut byte = [0];
    child
        .stdout
        .take()
        .ok_or("missing pipe")?
        .read_exact(&mut byte)?;
    if byte != *b"R" {
        return Err("fixture exec failed".into());
    }
    Ok(child)
}
fn close(slot: &RefCell<Option<Child>>) -> Result<()> {
    if let Some(mut child) = slot.borrow_mut().take()
        && child.try_wait()?.is_none()
    {
        child.kill()?;
        child.wait()?;
    }
    Ok(())
}
fn child_pid(slot: &RefCell<Option<Child>>) -> Option<u32> {
    slot.borrow().as_ref().map(Child::id)
}
fn missing() -> Box<dyn std::error::Error> {
    Box::new(crewmoor::failure::NativeFailure {
        code: "not_found".into(),
        message: "fixture target absent".into(),
    })
}
impl Fake {
    pub fn terminal(&self, r: &Record) -> Value {
        let state = if *self.finished.borrow() {
            "finished"
        } else {
            "live"
        };
        let mut value = json!({"address":"t7","labels":{"crewmoor-owner":r.owner,"crewmoor-team":r.config.team},
            "argv":r.config.argv(),"cwd":r.config.cwd,"state":state,"created_ms":42,
            "screen":"exact","cols":80,"rows":24,"pid":child_pid(&self.agent),"name":r.config.role});
        if *self.changed.borrow() {
            value["created_ms"] = json!(43);
        }
        if *self.omit_name.borrow() {
            value.as_object_mut().unwrap().remove("name");
        }
        value
    }
    fn start(&self, r: &Record) -> Result<()> {
        let saved = Record::load(&r.root)?;
        assert!(saved.request_issued);
        if !*self.no_agent.borrow() {
            *self.agent.borrow_mut() = Some(spawn(r)?);
        }
        if *self.ambiguous.borrow() {
            return Err(Box::new(crewmoor::failure::NativeFailure {
                code: "timeout".into(),
                message: "input outcome unknown".into(),
            }));
        }
        Ok(())
    }
    fn core(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.first().map(String::as_str) {
            Some("server") => self.server(r, args),
            Some("run") => self.start_core(r),
            Some("get") => self.get_core(r),
            Some("kill") => self.kill_core(),
            Some("rm") => {
                *self.removed.borrow_mut() = true;
                Ok(json!({"removed":"t7"}))
            }
            Some("ls") => Ok(self.inventory()),
            Some("app-document") => Ok(
                json!({"version":1,"value":{"spaces":[{"id":"sp-existing","tabs":[{"id":"tb-existing"}]}]}}),
            ),
            _ => Err("unexpected core call".into()),
        }
    }
    fn start_core(&self, r: &Record) -> Result<Value> {
        *self.holder.borrow_mut() = Some(spawn(r)?);
        *self.created.borrow_mut() = true;
        self.start(r)?;
        Ok(
            json!({"address":"t7","agent":r.config.agent.name(),"cwd":r.config.cwd,"name":r.config.role,"state":"live","turn":0}),
        )
    }
    fn get_core(&self, r: &Record) -> Result<Value> {
        if *self.removed.borrow() {
            return Err(missing());
        }
        Ok(self.terminal(r))
    }
    fn kill_core(&self) -> Result<Value> {
        if !*self.leave_actor.borrow() {
            close(&self.agent)?;
        }
        *self.finished.borrow_mut() = true;
        Ok(json!({"killed":true}))
    }
    fn inventory(&self) -> Value {
        let mut terminals = Vec::new();
        if *self.created.borrow() && !*self.removed.borrow() {
            terminals.push(json!({"address":"t7"}));
        }
        if *self.extra.borrow() {
            terminals.push(json!({"address":"user-terminal"}));
        }
        json!({"terminals":terminals})
    }
    fn server(&self, r: &Record, args: &[String]) -> Result<Value> {
        if args.get(1).map(String::as_str) == Some("stop") {
            if !*self.leave_holder.borrow() {
                close(&self.holder)?;
            }
            self.shutdown(r)?;
            return Ok(json!({}));
        }
        let holders = self.holders();
        Ok(
            json!({"version":"0.1.15","core":{"id":"own-core","name":"fixture"},
            "pid":child_pid(&self.child),"holder":holders.first(),"holders":holders,
            "uptime_ms":1,"live":1,"finished":0}),
        )
    }
    fn holders(&self) -> Vec<Value> {
        child_pid(&self.holder)
            .map(|pid| {
                vec![json!({"pid":pid,"instance":"owned-holder","current":true,"terminals":1})]
            })
            .unwrap_or_default()
    }
    fn tui(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.first().map(String::as_str) {
            Some("status") => Ok(json!({"version":"0.9.5"})),
            Some("workspace") => self.workspace(r, args),
            Some("agent") => {
                self.start(r)?;
                Ok(self.agent_info(r))
            }
            Some("pane") => self.pane(r, args),
            Some("session") => {
                self.shutdown(r)?;
                Ok(json!({}))
            }
            _ => Err("unexpected TUI call".into()),
        }
    }
    pub fn agent_info(&self, r: &Record) -> Value {
        let mut value = self.pane_info();
        if *self.minimal_agent.borrow() {
            return json!({"agent":value});
        }
        value["name"] = json!(r.config.role);
        value["agent"] = json!(r.config.agent.name());
        value["state_change_seq"] = json!(0);
        value["interactive_ready"] = json!(true);
        json!({"agent":value})
    }
    fn pane_info(&self) -> Value {
        let terminal = if *self.changed.borrow() {
            "replaced"
        } else {
            "term-own"
        };
        json!({"pane_id":"w1:p1","terminal_id":terminal,"workspace_id":"w1","tab_id":"w1:t1",
            "focused":false,"agent_status":"working","revision":1})
    }
    fn pane(&self, r: &Record, args: &[String]) -> Result<Value> {
        if *self.closed.borrow() {
            return Err(missing());
        }
        match args.get(1).map(String::as_str) {
            Some("get") => Ok(json!({"pane":self.pane_info()})),
            Some("close") => {
                if !*self.leave_actor.borrow() {
                    close(&self.agent)?;
                }
                close(&self.shell)?;
                *self.closed.borrow_mut() = true;
                Ok(json!({}))
            }
            _ => Ok(self.processes(r)),
        }
    }
    fn processes(&self, r: &Record) -> Value {
        if *self.missing_process.borrow() {
            return json!({"process_info":{"pane_id":"w1:p1"}});
        }
        let mut list = Vec::new();
        if let Some(pid) = child_pid(&self.agent) {
            list.push(json!({"pid":pid,"name":r.config.agent.name(),"argv":["synthetic-sensitive-unrelated-value"]}));
        }
        json!({"process_info":{"pane_id":"w1:p1","shell_pid":child_pid(&self.shell),"foreground_processes":list}})
    }
    fn workspace(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.get(1).map(String::as_str) {
            Some("create") => self.create_workspace(r),
            Some("get") => Ok(json!({"workspace":self.workspace_info(r)})),
            Some("close") => {
                assert_eq!(args.get(2).map(String::as_str), Some("w1"));
                if !*self.leave_workspace.borrow() {
                    *self.workspace_closed.borrow_mut() = true;
                }
                Ok(json!({}))
            }
            Some("list") => Ok(self.workspaces(r)),
            _ => Err("unexpected workspace call".into()),
        }
    }
    fn create_workspace(&self, r: &Record) -> Result<Value> {
        *self.shell.borrow_mut() = Some(spawn(r)?);
        *self.created.borrow_mut() = true;
        Ok(json!({"workspace":self.workspace_info(r),"root_pane":self.pane_info()}))
    }
    fn workspace_info(&self, r: &Record) -> Value {
        let panes = usize::from(!*self.closed.borrow()) + usize::from(*self.extra_pane.borrow());
        let tabs = 1 + usize::from(*self.extra_tab.borrow());
        json!({"workspace_id":"w1","number":1,"label":r.owner,"focused":false,
            "pane_count":panes,"tab_count":tabs,"active_tab_id":"w1:t1","agent_status":"unknown"})
    }
    fn workspaces(&self, r: &Record) -> Value {
        let mut list = Vec::new();
        if *self.created.borrow() && !*self.workspace_closed.borrow() {
            list.push(self.workspace_info(r));
        }
        if *self.extra.borrow() {
            list.push(json!({"workspace_id":"user-workspace"}));
        }
        json!({"workspaces":list})
    }
    fn shutdown(&self, r: &Record) -> Result<()> {
        close(&self.child)?;
        *self.stopped.borrow_mut() = true;
        if !*self.leave_socket.borrow() {
            self.listener.borrow_mut().take();
            std::fs::remove_file(r.socket())?;
        }
        Ok(())
    }
}
impl Runner for Fake {
    fn call(&self, r: &Record, args: &[String]) -> Result<Value> {
        self.calls.borrow_mut().push(args.to_vec());
        if *self.fail.borrow() {
            return Err("simulated transport failure".into());
        }
        match r.config.runtime {
            Runtime::Core => self.core(r, args),
            Runtime::Tui => self.tui(r, args),
        }
    }
    fn boot(&self, r: &Record) -> Result<process::Identity> {
        std::fs::create_dir_all(r.socket().parent().ok_or("missing socket parent")?)?;
        *self.listener.borrow_mut() = Some(UnixListener::bind(r.socket())?);
        let child = spawn(r)?;
        let identity = process::identity(child.id())?;
        *self.child.borrow_mut() = Some(child);
        Ok(identity)
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        for slot in [&self.agent, &self.shell, &self.holder, &self.child] {
            let _ = close(slot);
        }
    }
}
pub fn setup(runtime: Runtime) -> (tempfile::TempDir, Config, Fake) {
    let dir = directory();
    let mut config = config(dir.path(), runtime.clone());
    config.runtime_version = match runtime {
        Runtime::Core => "hc 0.1.15",
        Runtime::Tui => "herdr 0.9.5",
    }
    .into();
    (dir, config, Fake::default())
}
pub fn launch(runtime: Runtime) -> (tempfile::TempDir, Record, Fake) {
    let (dir, cfg, fake) = setup(runtime);
    let r = crewmoor::engine::launch(cfg, &dir.path().join("i"), &fake).unwrap();
    (dir, r, fake)
}
