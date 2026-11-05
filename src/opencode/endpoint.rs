//! Private facts of this launch, separate from public binding and receipts.
use super::api::Api;
use crate::{
    Result,
    feedback::{Context, model},
    process::{self, Identity},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub instance: String,
    pub session_id: String,
    pub url: String,
    pub password: String,
    pub version: String,
    pub launcher: Identity,
    pub server: Identity,
    pub tui: Identity,
}

pub fn path(registry: &Path, instance: &str) -> Result<PathBuf> {
    Ok(model::instance_dir(registry, instance)?.join("opencode-endpoint.json"))
}

pub fn load(registry: &Path, context: &Context) -> Result<Endpoint> {
    let bytes = private_file(registry, context)?;
    let endpoint: Endpoint = serde_json::from_slice(&bytes)?;
    endpoint.check_context(registry, context)?;
    Ok(endpoint)
}
fn private_file(registry: &Path, context: &Context) -> Result<Vec<u8>> {
    let path = path(registry, &context.instance)?;
    let metadata = fs::symlink_metadata(&path)?;
    let owner = fs::metadata(model::instance_dir(registry, &context.instance)?)?;
    if !metadata.is_file() || metadata.mode() & 0o777 != 0o600 || metadata.uid() != owner.uid() {
        return Err("private OpenCode endpoint type/permissions/owner differs".into());
    }
    Ok(fs::read(path)?)
}

impl Endpoint {
    fn check_context(&self, registry: &Path, context: &Context) -> Result<()> {
        let session = crate::feedback::registered_session(registry, context)?;
        if self.instance != context.instance || Some(&self.session_id) != session.as_ref() {
            return Err("private OpenCode endpoint instance/session differs".into());
        }
        if context.registration.agent != crate::config::AgentKind::OpenCode {
            return Err("endpoint context is not OpenCode".into());
        }
        Ok(())
    }

    pub fn api(&self, timeout: Duration) -> Result<Api> {
        Api::new(&self.url, &self.password, timeout)
    }

    pub fn verify(&self, timeout: Duration) -> Result<Value> {
        for identity in [&self.launcher, &self.server, &self.tui] {
            process::same(identity)?;
        }
        let api = self.api(timeout)?;
        let info = api.get("/api/info")?;
        if info["pid"].as_u64() != Some(u64::from(self.server.pid))
            || info["version"] != self.version
        {
            return Err("private OpenCode server identity/version differs".into());
        }
        api.session(&self.session_id)
    }

    pub fn save(&self, registry: &Path) -> Result<()> {
        model::write_new(&path(registry, &self.instance)?, &serde_json::to_vec(self)?)
    }

    pub fn confirm_exit(&self) -> Result<()> {
        for identity in [&self.tui, &self.launcher, &self.server] {
            process::wait_exited(identity)?;
        }
        Ok(())
    }
}
