//! Real owned child processes standing in for pane Agents, so new TUI bindings can
//! locate a process through the synthetic `pane process-info`.
use serde_json::{Value, json};
use std::process::{Child, Command, Stdio};

pub const EXECUTABLE: &str = env!("CARGO_BIN_EXE_transport-fixture");

pub struct Agents {
    children: Vec<(String, String, Child)>,
}
impl Agents {
    /// One held process per `(pane, kind)`, running in `cwd`.
    pub fn spawn(cwd: &std::path::Path, panes: &[(&str, &str)]) -> Self {
        let children = panes
            .iter()
            .map(|(pane, kind)| {
                let child = Command::new(EXECUTABLE)
                    .arg("hold")
                    .current_dir(cwd)
                    .stdin(Stdio::piped())
                    .spawn()
                    .unwrap();
                ((*pane).to_owned(), (*kind).to_owned(), child)
            })
            .collect();
        Self { children }
    }
    /// `processes.PANE` entries for the CLI transport fixture.
    pub fn processes(&self) -> Value {
        let mut map = json!({});
        for (pane, kind, child) in &self.children {
            map[pane] = json!([{"name":kind,"pid":child.id()}]);
        }
        map
    }
}
impl Drop for Agents {
    fn drop(&mut self) {
        for (_, _, child) in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
