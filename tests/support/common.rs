use crewmoor::{
    Result,
    config::{AgentKind, Config, Runtime},
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
        agent: AgentKind::Codex,
        agent_executable: env!("CARGO_BIN_EXE_transport-fixture").into(),
        agent_version: "fixture 1".into(),
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
    pub holder: RefCell<Option<Child>>,
    pub agent: RefCell<Option<Child>>,
    pub shell: RefCell<Option<Child>>,
    pub empty_shell: RefCell<bool>,
    pub extra_process: RefCell<bool>,
    pub removed: RefCell<bool>,
    pub closed: RefCell<bool>,
    pub leave_actor: RefCell<bool>,
    pub leave_holder: RefCell<bool>,
    pub leave_socket: RefCell<bool>,
    pub fail_creation: RefCell<bool>,
    pub fail_after_holder: RefCell<bool>,
    pub created: RefCell<bool>,
    pub multiple_holders: RefCell<bool>,
    pub changed_holder: RefCell<bool>,
    pub omit_holder: RefCell<bool>,
    pub omit_name: RefCell<bool>,
    pub null_name: RefCell<bool>,
    pub missing_process: RefCell<bool>,
    pub null_process: RefCell<bool>,
    pub omit_pid: RefCell<bool>,
    pub minimal_terminal: RefCell<bool>,
    pub ambiguous_start: RefCell<bool>,
}
fn spawn(cwd: &Path) -> Result<Child> {
    use std::io::Read;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_transport-fixture"))
        .arg("hold")
        .env("CODEX_SESSION_ID", "synthetic-transport-session")
        .env("CREWMOOR_TRANSPORT_HANDSHAKE", "1")
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    let mut marker = [0];
    child
        .stdout
        .take()
        .ok_or("missing transport pipe")?
        .read_exact(&mut marker)?;
    if marker != *b"R" {
        return Err("transport exec handshake failed".into());
    }
    Ok(child)
}
fn spawn_shell(cwd: &Path) -> Result<Child> {
    use std::io::Read;
    let mut child = std::process::Command::new("/bin/bash")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "printf 'R\\n'; read -r transport_end",
        ])
        .current_dir(cwd)
        .env("CODEX_SESSION_ID", "synthetic-transport-session")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let mut marker = [0];
    child
        .stdout
        .take()
        .ok_or("missing shell pipe")?
        .read_exact(&mut marker)?;
    if marker != *b"R" {
        return Err("shell exec handshake failed".into());
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
        if *self.minimal_terminal.borrow() {
            return json!({"address":"t7","labels":{"crewmoor-owner":r.owner,"crewmoor-team":r.config.team},"argv":r.config.argv(),"cwd":r.config.cwd,"state":state,"created_ms":42,"screen":"exact","cols":80,"rows":24});
        }
        let mut value = json!({"address":"t7","name":r.config.role,"created_ms":42,"cwd":r.config.cwd,
            "argv":r.config.argv(),"state":state,"pid":child_pid(&self.agent),
            "labels":{"crewmoor-owner":r.owner,"crewmoor-team":r.config.team},"reported":false,
            "screen":"exact","cols":80,"rows":24});
        if *self.omit_name.borrow() {
            value.as_object_mut().unwrap().remove("name");
        }
        if *self.null_name.borrow() {
            value["name"] = Value::Null;
        }
        if *self.omit_pid.borrow() {
            value.as_object_mut().unwrap().remove("pid");
        }
        value
    }
    fn core(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.first().map(String::as_str) {
            Some("server") => self.server(r, args),
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
        if *self.fail_creation.borrow() {
            return Err("failed before terminal creation".into());
        }
        *self.holder.borrow_mut() = Some(spawn(&r.config.cwd)?);
        if *self.fail_after_holder.borrow() {
            return Err("failed after holder creation, before terminal".into());
        }
        *self.agent.borrow_mut() = Some(spawn(&r.config.cwd)?);
        *self.created.borrow_mut() = true;
        Ok(
            json!({"address":"t7","agent":"codex","cwd":r.config.cwd,"name":r.config.role,"state":"live","turn":0}),
        )
    }
    fn get_core(&self, r: &Record) -> Result<Value> {
        if *self.removed.borrow() || !*self.created.borrow() {
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
        let mut terminals = Vec::new();
        if *self.created.borrow() && !*self.removed.borrow() {
            terminals.push(json!({"address":"t7","agent":"codex","cwd":r.config.cwd,"name":r.config.role,"state":"live","status":"idle","turn":1}));
        }
        if *self.extra.borrow() {
            terminals.push(json!({"address":"unowned"}));
        }
        json!({"terminals":terminals})
    }
    fn tui(&self, r: &Record, args: &[String]) -> Result<Value> {
        match args.first().map(String::as_str) {
            Some("status") => Ok(json!({"version":"0.9.5"})),
            Some("workspace") => self.workspace(r, args),
            Some("pane") => self.pane(r, args),
            Some("session") => {
                self.shutdown()?;
                self.close_endpoint(r)?;
                Ok(json!({}))
            }
            _ => Ok(json!({})),
        }
    }
    fn pane(&self, r: &Record, args: &[String]) -> Result<Value> {
        if args.get(1).map(String::as_str) == Some("run") {
            return self.start_tui(r);
        }
        if *self.closed.borrow() {
            return Err(missing("pane_not_found"));
        }
        if args.get(1).map(String::as_str) == Some("get") {
            let pane = self.pane_info();
            return Ok(json!({"pane":pane}));
        }
        Ok(self.processes(r))
    }
    fn start_tui(&self, r: &Record) -> Result<Value> {
        *self.agent.borrow_mut() = Some(spawn(&r.config.cwd)?);
        if *self.ambiguous_start.borrow() {
            return Err(Box::new(crewmoor::failure::NativeFailure {
                code: "timeout".into(),
                message: "ambiguous input admission".into(),
            }));
        }
        Ok(Value::Null)
    }
    fn pane_info(&self) -> Value {
        json!({"pane_id":"w1:p1","terminal_id":"term-own","workspace_id":"w1","tab_id":"w1:t1","focused":false,"agent_status":"unknown","revision":1})
    }
    fn processes(&self, r: &Record) -> Value {
        if *self.missing_process.borrow() {
            return json!({"process_info":{"pane_id":"w1:p1"}});
        }
        if *self.null_process.borrow() {
            return json!({"process_info":{"pane_id":"w1:p1","shell_pid":null,"foreground_processes":null}});
        }
        let shell = child_pid(&self.shell);
        let foreground = if *self.empty_shell.borrow() || child_pid(&self.agent).is_none() {
            shell
        } else {
            child_pid(&self.agent)
        };
        let name = if foreground == shell {
            "bash"
        } else {
            r.config.agent.name()
        };
        let mut processes = vec![json!({"name":name,"pid":foreground})];
        if *self.extra_process.borrow() {
            processes.push(json!({"name":"other","pid":1}));
        }
        json!({"process_info":{"pane_id":"w1:p1","shell_pid":shell,"foreground_process_group_id":foreground,"foreground_processes":processes}})
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
                if *self.fail_creation.borrow() {
                    return Err("failed before workspace creation".into());
                }
                *self.shell.borrow_mut() = Some(spawn_shell(&r.config.cwd)?);
                *self.created.borrow_mut() = true;
                let workspace = self.workspaces(r)["workspaces"][0].clone();
                let pane = self.pane_info();
                Ok(json!({"workspace":workspace,"root_pane":pane}))
            }
        }
    }
    fn workspaces(&self, r: &Record) -> Value {
        let mut workspaces = Vec::new();
        if *self.created.borrow() && !*self.closed.borrow() {
            workspaces
                .push(json!({"workspace_id":"w1","label":r.owner,"pane_count":1,"tab_count":1,"number":1,"focused":false,"active_tab_id":"w1:t1","agent_status":"unknown"}));
        }
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
    fn server(&self, r: &Record, args: &[String]) -> Result<Value> {
        if args.get(1).map(String::as_str) == Some("stop") {
            close(&self.child)?;
            if !*self.leave_holder.borrow() {
                close(&self.holder)?;
            }
            self.close_endpoint(r)?;
            return Ok(json!({}));
        }
        let holders = self.holders();
        let live = self.live_count();
        let finished =
            usize::from(*self.created.borrow() && *self.killed.borrow() && !*self.removed.borrow());
        let mut value = json!({"version":"0.1.15","core":{"id":"own-core","name":"fixture"},"pid":child_pid(&self.child),"holder":holders.first(),"holders":holders,"uptime_ms":1,"live":live,"finished":finished});
        if *self.omit_holder.borrow() {
            value.as_object_mut().unwrap().remove("holder");
        }
        Ok(value)
    }
    fn holders(&self) -> Vec<Value> {
        let mut list = Vec::new();
        if let Some(pid) = child_pid(&self.holder) {
            let instance = if *self.changed_holder.borrow() {
                "changed-holder"
            } else {
                "owned-holder"
            };
            let terminals = self.live_count();
            list.push(json!({"pid":pid,"instance":instance,"current":true,"terminals":terminals}));
            if *self.multiple_holders.borrow() {
                list.push(json!({"pid":pid,"instance":"other","current":false,"terminals":0}));
            }
        }
        list
    }
    fn live_count(&self) -> usize {
        usize::from(*self.created.borrow() && !*self.killed.borrow() && !*self.removed.borrow())
            + usize::from(*self.extra.borrow())
    }
    fn close_endpoint(&self, r: &Record) -> Result<()> {
        if !*self.leave_socket.borrow() {
            self.listener.borrow_mut().take();
            std::fs::remove_file(r.socket())?;
        }
        Ok(())
    }
    pub fn replace_shell(&self, r: &Record) -> Result<()> {
        close(&self.shell)?;
        *self.shell.borrow_mut() = Some(spawn_shell(&r.config.cwd)?);
        Ok(())
    }
    pub fn replace_holder(&self, r: &Record) -> Result<()> {
        close(&self.holder)?;
        *self.holder.borrow_mut() = Some(spawn(&r.config.cwd)?);
        Ok(())
    }
    fn shutdown(&self) -> Result<()> {
        close(&self.agent)?;
        close(&self.shell)?;
        close(&self.holder)?;
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
        let child = spawn(&r.config.cwd)?;
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
    let (dir, config, fake) = setup(runtime);
    let record = crewmoor::engine::launch(config, &dir.path().join("instance"), &fake).unwrap();
    (dir, record, fake)
}
pub fn setup(runtime: Runtime) -> (tempfile::TempDir, Config, Fake) {
    let dir = directory();
    let fake = Fake::default();
    let mut config = config(dir.path(), runtime.clone());
    config.runtime_version = match runtime {
        Runtime::Core => "hc 0.1.15",
        Runtime::Tui => "herdr 0.9.5",
    }
    .into();
    (dir, config, fake)
}
