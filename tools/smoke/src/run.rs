//! Evidence directory, recorded command execution and bounded file waits.
use crate::Result;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub struct Run {
    pub plan: Value,
    pub evidence: PathBuf,
    pub work: PathBuf,
    step: Cell<u32>,
    pub team_dir: RefCell<Option<PathBuf>>,
}
pub fn sha256(path: &Path) -> Result<String> {
    let output = Command::new("sha256sum").arg(path).output()?;
    let text = String::from_utf8(output.stdout)?;
    Ok(text
        .split_whitespace()
        .next()
        .ok_or("sha256sum failed")?
        .into())
}
impl Run {
    pub fn create(plan: Value, digest: &str) -> Result<Self> {
        let evidence = PathBuf::from(text(&plan, "evidence")?);
        fs::create_dir(&evidence)?; // a fresh directory per run; never reused
        let work = evidence.join("work");
        fs::create_dir(&work)?;
        fs::write(evidence.join("approved-plan.sha256"), digest)?;
        fs::write(
            evidence.join("plan.json"),
            serde_json::to_vec_pretty(&plan)?,
        )?;
        Ok(Self {
            plan,
            evidence,
            work,
            step: Cell::new(0),
            team_dir: RefCell::new(None),
        })
    }
    pub fn get(&self, key: &str) -> Result<String> {
        text(&self.plan, key)
    }
    /// Run one command once and save argv, exit code, stdout and stderr.
    pub fn exec(&self, label: &str, argv: &[String]) -> Result<(i32, String)> {
        self.exec_full(label, argv)
            .map(|(code, out, _)| (code, out))
    }
    /// As `exec`, also returning stderr.
    pub fn exec_full(&self, label: &str, argv: &[String]) -> Result<(i32, String, String)> {
        let n = self.step.get() + 1;
        self.step.set(n);
        let output = Command::new("timeout")
            .arg("60")
            .args(argv)
            .env("HERDR_SOCKET_PATH", self.get("host_socket")?)
            .env_remove("HERDR_PANE_ID")
            .env_remove("CREWMOOR_FEEDBACK_INSTANCE")
            .output()?;
        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let record = json!({"step":n,"label":label,"argv":argv,"exit":code,
            "stdout":stdout,"stderr":stderr});
        self.save(&format!("{n:02}-{label}.json"), &record)?;
        println!("step {n:02} {label}: exit {code}");
        Ok((code, stdout, stderr))
    }
    pub fn save(&self, name: &str, value: &Value) -> Result<()> {
        fs::write(self.evidence.join(name), serde_json::to_vec_pretty(value)?)?;
        Ok(())
    }
    /// Bounded wait for a file fact; records the outcome either way, never retries actions.
    pub fn wait(&self, label: &str, seconds: u64, check: impl Fn() -> bool) -> Result<bool> {
        let start = Instant::now();
        let mut met = check();
        while !met && start.elapsed() < Duration::from_secs(seconds) {
            std::thread::sleep(Duration::from_secs(2));
            met = check();
        }
        let waited = start.elapsed().as_secs();
        self.save(
            &format!("wait-{label}.json"),
            &json!({"met":met,"seconds":waited}),
        )?;
        println!("wait {label}: met={met} after {waited}s");
        Ok(met)
    }
    /// Write SHA256SUMS over every evidence file.
    pub fn seal(&self) -> Result<()> {
        let mut lines = Vec::new();
        for path in files(&self.evidence)? {
            let name = path.strip_prefix(&self.evidence)?.display().to_string();
            lines.push(format!("{}  {name}", sha256(&path)?));
        }
        lines.sort();
        fs::write(self.evidence.join("SHA256SUMS"), lines.join("\n") + "\n")?;
        Ok(())
    }
}
fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut list = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            list.extend(files(&path)?);
        } else if path.file_name().is_some_and(|n| n != "SHA256SUMS") {
            list.push(path);
        }
    }
    Ok(list)
}
pub fn text(value: &Value, key: &str) -> Result<String> {
    Ok(value[key]
        .as_str()
        .ok_or_else(|| format!("plan missing {key}"))?
        .into())
}
