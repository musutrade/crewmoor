use crate::Result;
use std::{fs, path::Path};
const CODEX: &str = include_str!("../assets/plugin/.codex-plugin/plugin.json");
const CLAUDE: &str = include_str!("../assets/plugin/.claude-plugin/plugin.json");
const CODEX_HOOKS: &str = include_str!("../assets/plugin/hooks/codex-hooks.json");
const CLAUDE_HOOKS: &str = include_str!("../assets/plugin/hooks/hooks.json");
const CATALOG: &str = include_str!("../assets/catalog/.agents/plugins/marketplace.json");
pub fn marketplace(root: &Path) -> Result<()> {
    fs::create_dir(root)?;
    fs::create_dir_all(root.join(".agents/plugins"))?;
    fs::write(root.join(".agents/plugins/marketplace.json"), CATALOG)?;
    fs::create_dir(root.join("plugins"))?;
    export(&root.join("plugins/tool-rule-hook"))
}
pub fn export(root: &Path) -> Result<()> {
    let executable = std::env::current_exe()?;
    let executable = executable
        .to_str()
        .ok_or("checker installation path is not UTF-8")?;
    fs::create_dir(root)?;
    for (relative, source) in [
        (".codex-plugin/plugin.json", CODEX),
        (".claude-plugin/plugin.json", CLAUDE),
        ("hooks/codex-hooks.json", CODEX_HOOKS),
        ("hooks/hooks.json", CLAUDE_HOOKS),
    ] {
        let mode = if relative == "hooks/codex-hooks.json" {
            "codex-hook"
        } else {
            "optional-hook"
        };
        let command = format!("'{}' {mode}", executable.replace('\'', "'\\''"));
        let quoted = serde_json::to_string(&command)?;
        let path = root.join(relative);
        fs::create_dir_all(path.parent().ok_or("plugin asset has no parent")?)?;
        fs::write(
            path,
            source.replace("\"__TOOL_RULE_HOOK_COMMAND__\"", &quoted),
        )?;
    }
    Ok(())
}
