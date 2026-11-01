use crate::{Result, config::Config, process::Identity};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
};
#[derive(Debug, Deserialize, Serialize)]
pub struct Record {
    pub crewmoor_version: String,
    pub config: Config,
    pub owner: String,
    pub session: String,
    pub root: PathBuf,
    pub phase: String,
    pub container_id: Option<String>,
    pub server: Option<Identity>,
    pub socket_inode: Option<u64>,
    pub handle: Option<String>,
    pub terminal_id: Option<String>,
    pub workspace_id: Option<String>,
    pub created_ms: Option<u64>,
    pub agent_process: Option<Identity>,
    pub terminal_process: Option<Identity>,
    pub core_holders: Option<Vec<crate::holders::Holder>>,
    pub ending_process: Option<Identity>,
    pub accepted: Option<bool>,
    #[serde(default)]
    pub request_issued: bool,
    pub request_error: Option<String>,
    pub native_name: Option<String>,
    #[serde(default)]
    pub process_verified: bool,
    pub agent_status: String,
    pub server_owned: bool,
    pub binding_scope: String,
    pub app_terminal: Option<String>,
    pub project: Option<crate::project::Project>,
}
impl Record {
    pub fn prepare(config: Config, root: &Path) -> Result<Self> {
        config.validate()?;
        crate::config::absolute(root)?;
        if root.as_os_str().len() > 45 {
            return Err("state root must be <=45 bytes for native Unix sockets".into());
        }
        fs::DirBuilder::new().mode(0o700).create(root)?;
        let owner = uuid::Uuid::new_v4().simple().to_string();
        let server_owned = config.app.is_none();
        let session = config
            .app
            .as_ref()
            .map(|a| a.session.clone())
            .unwrap_or_else(|| format!("cm-{}", &owner[..12]));
        let result = Self {
            crewmoor_version: env!("CARGO_PKG_VERSION").into(),
            session,
            config,
            owner,
            root: root.canonicalize()?,
            phase: "prepared".into(),
            container_id: None,
            server: None,
            socket_inode: None,
            handle: None,
            terminal_id: None,
            workspace_id: None,
            created_ms: None,
            agent_process: None,
            terminal_process: None,
            core_holders: None,
            ending_process: None,
            accepted: None,
            request_issued: false,
            request_error: None,
            native_name: None,
            process_verified: false,
            agent_status: "unknown".into(),
            server_owned,
            binding_scope: if server_owned { "instance" } else { "terminal" }.into(),
            app_terminal: None,
            project: None,
        };
        result.save()?;
        result.bind()?;
        Ok(result)
    }
    pub fn save(&self) -> Result<()> {
        let temporary = self.root.join("record.new");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, self.root.join("record.json"))?;
        Ok(())
    }
    pub fn load(root: &Path) -> Result<Self> {
        let record: Self = serde_json::from_slice(&fs::read(root.join("record.json"))?)?;
        if record.root != root.canonicalize()? {
            return Err("state root mismatch".into());
        }
        Ok(record)
    }
    pub fn socket(&self) -> PathBuf {
        if let Some(app) = &self.config.app {
            return app.socket.clone();
        }
        match self.config.runtime {
            crate::config::Runtime::Core => self
                .config
                .endpoint_dir
                .as_ref()
                .map(|dir| dir.join(format!("{}.sock", self.session)))
                .unwrap_or_else(|| self.root.join("runtime.sock")),
            crate::config::Runtime::Tui => self
                .root
                .join("config/herdr/sessions")
                .join(&self.session)
                .join("herdr.sock"),
        }
    }
    pub fn capture_socket(&mut self) -> Result<()> {
        UnixStream::connect(self.socket())?;
        self.socket_inode = Some(fs::metadata(self.socket())?.ino());
        Ok(())
    }
    pub fn guard(&self) -> Result<()> {
        if self.server_owned != self.config.app.is_none() {
            return Err("server ownership disagrees with launch mode".into());
        }
        self.binding()?;
        crate::process::same(self.server.as_ref().ok_or("missing server identity")?)?;
        if Some(fs::metadata(self.socket())?.ino()) != self.socket_inode {
            return Err("socket instance changed".into());
        }
        Ok(())
    }
    pub fn lock(&self) -> Result<Lock> {
        Lock::take(&self.root)
    }
    fn binding_path(&self) -> PathBuf {
        self.config.store.join(format!("{}.json", self.config.team))
    }
    fn binding_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(&self.owner, &self.root))?)
    }
    fn bind(&self) -> Result<()> {
        use std::io::Write;
        fs::create_dir_all(&self.config.store)?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(self.binding_path())?;
        file.write_all(&self.binding_bytes()?)?;
        Ok(())
    }
    pub fn binding(&self) -> Result<()> {
        if fs::read(self.binding_path())? != self.binding_bytes()? {
            return Err("team is bound to another container".into());
        }
        Ok(())
    }
    pub fn release(&self) -> Result<()> {
        self.binding()?;
        fs::remove_file(self.binding_path())?;
        Ok(())
    }
}
pub struct Lock {
    path: PathBuf,
}
impl Lock {
    pub fn take(root: &Path) -> Result<Self> {
        let path = root.join("operation.lock");
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        Ok(Self { path })
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
