use crewmoor::{
    Result,
    config::{Config, Runtime},
    native::{Runner, strings},
    process,
    record::Record,
};
use serde_json::{Value, json};
use std::{cell::RefCell, os::unix::net::UnixListener, path::Path};

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
pub struct Fake {
    pub calls: RefCell<Vec<Vec<String>>>,
    pub listener: RefCell<Option<UnixListener>>,
    pub killed: RefCell<bool>,
    pub extra: RefCell<bool>,
    pub fail: RefCell<bool>,
    pub child: RefCell<Option<std::process::Child>>,
}
impl Default for Fake {
    fn default() -> Self {
        Self {
            calls: RefCell::default(),
            listener: RefCell::default(),
            killed: RefCell::new(false),
            extra: RefCell::new(false),
            fail: RefCell::new(false),
            child: RefCell::new(None),
        }
    }
}
impl Fake {
    pub fn terminal(&self, r: &Record) -> Value {
        let state = if *self.killed.borrow() {
            "finished"
        } else {
            "live"
        };
        json!({"address":"t7", "name":r.config.role,"created_ms":42,"cwd":r.config.cwd,
            "argv":r.config.argv(),"state":state,"pid":std::process::id(),
            "labels":{"crewmoor-owner":r.owner,"crewmoor-team":r.config.team}, "reported":false})
    }
    pub fn agent(&self, r: &Record) -> Value {
        json!({"agent":{"pane_id":"w1:p1","terminal_id":"term-own","agent":"codex","name":r.config.role,"interactive_ready":true,"agent_status":"idle"}})
    }
    fn core(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.first().map(String::as_str) {
            Some("server") => self.server(args),
            Some("run") => Ok(
                json!({"address":"t7","agent":"codex","cwd":r.config.cwd,"name":r.config.role,"state":"live","turn":0}),
            ),
            Some("get") => Ok(self.terminal(r)),
            Some("ls") => Ok(self.inventory(r)),
            Some("kill") => {
                *self.killed.borrow_mut() = true;
                Ok(json!({"killed":true}))
            }
            _ => Ok(json!({})),
        }
    }
    fn inventory(&self, r: &Record) -> Value {
        let mut terminals = vec![self.terminal(r)];
        if *self.extra.borrow() {
            terminals.push(json!({"address":"unowned"}));
        }
        json!({"terminals":terminals})
    }
    fn tui(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.first().map(String::as_str) {
            Some("status") => Ok(json!({"version":"0.9.5"})),
            Some("workspace") => self.workspace(r, args),
            Some("agent") => Ok(self.agent(r)),
            Some("pane") => Ok(
                json!({"process_info":{"foreground_processes":[{"name":"codex","pid":std::process::id()}]}}),
            ),
            Some("session") => self.shutdown().map(|_| json!({})),
            _ => Ok(json!({})),
        }
    }
    fn workspace(&self, r: &Record, args: &[String]) -> Result<Value> {
        if args == strings(&["workspace", "list"]) {
            let mut workspaces =
                vec![json!({"workspace_id":"w1","label":r.owner,"pane_count":1,"tab_count":1})];
            if *self.extra.borrow() {
                workspaces.push(json!({"workspace_id":"unowned"}));
            }
            return Ok(json!({"workspaces":workspaces}));
        }
        Ok(
            json!({"workspace":{"workspace_id":"w1"},"root_pane":{"pane_id":"w1:p1","terminal_id":"term-own"}}),
        )
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
        let child = std::process::Command::new(env!("CARGO_BIN_EXE_transport-fixture"))
            .arg("hold")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        let identity = process::identity(child.id())?;
        *self.child.borrow_mut() = Some(child);
        Ok(identity)
    }
}
impl Fake {
    fn server(&self, args: &[String]) -> Result<Value> {
        if args.get(1).map(String::as_str) == Some("stop") {
            self.shutdown()?;
        }
        Ok(json!({"version":"0.1.15","core":{"id":"own-core"},"pid":std::process::id()}))
    }
    fn shutdown(&self) -> Result<()> {
        if let Some(mut child) = self.child.borrow_mut().take() {
            child.kill()?;
            child.wait()?;
        }
        Ok(())
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
