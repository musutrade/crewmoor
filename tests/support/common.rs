use crewmoor::{
    Result,
    config::{Config, Runtime},
    native::Runner,
    process,
    record::Record,
};
use serde_json::{Value, json};
use std::{cell::RefCell, os::unix::net::UnixListener, path::Path, process::Child};

pub fn config(base: &Path, runtime: Runtime) -> Config {
    Config {
        team: "test-team".into(),
        role: "probe".into(),
        runtime,
        cwd: base.to_path_buf(),
        store: base.join("bindings"),
        endpoint_dir: None,
        runtime_binary: env!("CARGO_BIN_EXE_transport-fixture").into(),
        runtime_version: "fixture 1".into(),
        agent_version: "codex-cli test".into(),
        agent_args: Vec::new(),
    }
}
pub fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("cm")
        .tempdir_in("/tmp")
        .unwrap()
}
#[derive(Default)]
pub struct Fake {
    pub calls: RefCell<Vec<Vec<String>>>,
    pub listener: RefCell<Option<UnixListener>>,
    pub killed: RefCell<bool>,
    pub extra: RefCell<bool>,
    pub fail: RefCell<bool>,
    pub child: RefCell<Option<Child>>,
    pub agent: RefCell<Option<Child>>,
    pub shell: RefCell<Option<Child>>,
    pub empty_shell: RefCell<bool>,
    pub extra_process: RefCell<bool>,
    pub removed: RefCell<bool>,
    pub closed: RefCell<bool>,
    pub leave_actor: RefCell<bool>,
}
fn spawn() -> Result<Child> {
    Ok(
        std::process::Command::new(env!("CARGO_BIN_EXE_transport-fixture"))
            .arg("hold")
            .stdin(std::process::Stdio::piped())
            .spawn()?,
    )
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
fn missing(code: &str) -> Box<dyn std::error::Error> {
    Box::new(crewmoor::failure::NativeFailure {
        code: code.into(),
        message: "fixture missing target".into(),
    })
}
impl Fake {
    pub fn terminal(&self, r: &Record) -> Value {
        let state = if *self.killed.borrow() {
            "finished"
        } else {
            "live"
        };
        json!({"address":"t7","name":r.config.role,"created_ms":42,"cwd":r.config.cwd,
            "argv":r.config.argv(),"state":state,"pid":child_pid(&self.agent),
            "labels":{"crewmoor-owner":r.owner,"crewmoor-team":r.config.team},"reported":false})
    }
    pub fn agent(&self, r: &Record) -> Value {
        json!({"agent":{"pane_id":"w1:p1","terminal_id":"term-own","agent":"codex","name":r.config.role,"interactive_ready":true,"agent_status":"idle"}})
    }
    fn core(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.first().map(String::as_str) {
            Some("server") => self.server(args),
            Some("run") => self.start_core(r),
            Some("get") => self.get_core(r),
            Some("ls") => Ok(self.inventory(r)),
            Some("kill") => self.kill_core(),
            Some("rm") => {
                *self.removed.borrow_mut() = true;
                Ok(json!({"removed":"t7"}))
            }
            _ => Ok(json!({})),
        }
    }
    fn start_core(&self, r: &Record) -> Result<Value> {
        *self.agent.borrow_mut() = Some(spawn()?);
        Ok(
            json!({"address":"t7","agent":"codex","cwd":r.config.cwd,"name":r.config.role,"state":"live","turn":0}),
        )
    }
    fn get_core(&self, r: &Record) -> Result<Value> {
        if *self.removed.borrow() {
            return Err(missing("not_found"));
        }
        Ok(self.terminal(r))
    }
    fn kill_core(&self) -> Result<Value> {
        if !*self.leave_actor.borrow() {
            close(&self.agent)?;
        }
        *self.killed.borrow_mut() = true;
        Ok(json!({"killed":true}))
    }
    fn inventory(&self, r: &Record) -> Value {
        let mut terminals = vec![
            json!({"address":"t7","agent":"codex","cwd":r.config.cwd,"name":r.config.role,"state":"live","status":"idle","turn":1}),
        ];
        if *self.extra.borrow() {
            terminals.push(json!({"address":"unowned"}));
        }
        json!({"terminals":terminals})
    }
    fn tui(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.first().map(String::as_str) {
            Some("status") => Ok(json!({"version":"0.9.5"})),
            Some("workspace") => self.workspace(r, args),
            Some("agent") => {
                if args.get(1).map(String::as_str) == Some("start") {
                    *self.agent.borrow_mut() = Some(spawn()?);
                }
                Ok(self.agent(r))
            }
            Some("pane") => self.pane(args),
            Some("session") => self.shutdown().map(|_| json!({})),
            _ => Ok(json!({})),
        }
    }
    fn pane(&self, args: &[String]) -> Result<Value> {
        if *self.closed.borrow() {
            return Err(missing("pane_not_found"));
        }
        if args.get(1).map(String::as_str) == Some("get") {
            return Ok(
                json!({"pane":{"pane_id":"w1:p1","terminal_id":"term-own","workspace_id":"w1"}}),
            );
        }
        Ok(self.processes())
    }
    fn processes(&self) -> Value {
        let shell = child_pid(&self.shell);
        let foreground = if *self.empty_shell.borrow() {
            shell
        } else {
            child_pid(&self.agent)
        };
        let mut processes = vec![json!({"name":"codex","pid":foreground})];
        if *self.extra_process.borrow() {
            processes.push(json!({"name":"other","pid":1}));
        }
        json!({"process_info":{"shell_pid":shell,"foreground_process_group_id":foreground,"foreground_processes":processes}})
    }
    fn workspace(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.get(1).map(String::as_str) {
            Some("list") => Ok(self.workspaces(r)),
            Some("close") => {
                close(&self.agent)?;
                close(&self.shell)?;
                *self.closed.borrow_mut() = true;
                Ok(json!({}))
            }
            _ => {
                *self.shell.borrow_mut() = Some(spawn()?);
                Ok(
                    json!({"workspace":{"workspace_id":"w1"},"root_pane":{"pane_id":"w1:p1","terminal_id":"term-own"}}),
                )
            }
        }
    }
    fn workspaces(&self, r: &Record) -> Value {
        let mut workspaces =
            vec![json!({"workspace_id":"w1","label":r.owner,"pane_count":1,"tab_count":1})];
        if *self.extra.borrow() {
            workspaces.push(json!({"workspace_id":"unowned"}));
        }
        json!({"workspaces":workspaces})
    }
    pub fn discard_agent(&self) -> Result<()> {
        close(&self.agent)?;
        *self.empty_shell.borrow_mut() = true;
        Ok(())
    }
    fn server(&self, args: &[String]) -> Result<Value> {
        if args.get(1).map(String::as_str) == Some("stop") {
            close(&self.child)?;
        }
        Ok(json!({"version":"0.1.15","core":{"id":"own-core"},"pid":child_pid(&self.child)}))
    }
    fn shutdown(&self) -> Result<()> {
        close(&self.agent)?;
        close(&self.shell)?;
        close(&self.child)
    }
}
impl Runner for Fake {
    fn call(&self, r: &Record, args: &[String]) -> Result<Value> {
        self.calls.borrow_mut().push(args.to_vec());
        if *self.fail.borrow() {
            return Err("simulated ambiguous delivery".into());
        }
        match r.config.runtime {
            Runtime::Core => self.core(r, args),
            Runtime::Tui => self.tui(r, args),
        }
    }
    fn boot(&self, r: &Record) -> Result<process::Identity> {
        std::fs::create_dir_all(r.socket().parent().ok_or("missing parent")?)?;
        *self.listener.borrow_mut() = Some(UnixListener::bind(r.socket())?);
        let child = spawn()?;
        let identity = process::identity(child.id())?;
        *self.child.borrow_mut() = Some(child);
        Ok(identity)
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
pub fn launch(runtime: Runtime) -> (tempfile::TempDir, Record, Fake) {
    let dir = directory();
    let fake = Fake::default();
    let mut config = config(dir.path(), runtime.clone());
    config.runtime_version = match runtime {
        Runtime::Core => "hc 0.1.15",
        Runtime::Tui => "herdr 0.9.5",
    }
    .into();
    let record = crewmoor::engine::launch(config, &dir.path().join("instance"), &fake).unwrap();
    (dir, record, fake)
}
