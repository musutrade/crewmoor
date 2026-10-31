use super::common;
use crewmoor::{config::Runtime, record::Record};
use serde_json::Value;
use std::process::Command;

const REMOVED: &[&str] = &[
    "CODEX_SESSION_ID",
    "CODEX_TOOL_CALL_ID",
    "CODEX_CI",
    "CODEX_VERSION",
    "NO_COLOR",
    "CODEX_THREAD_ID",
    "CODEX_INTERNAL_ORIGINATOR_OVERRIDE",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS",
    "HERDR_SOCKET_PATH",
    "HERDR_SESSION",
    "HERDR_CONFIG_PATH",
    "HERDR_PANE_ID",
    "HERDR_TAB_ID",
    "HERDR_WORKSPACE_ID",
    "HERDR_ENV",
    "HC_SOCKET",
    "HC_SESSION",
    "HC_TERMINAL_ID",
    "HC_TERMINAL_SOCKET",
    "HC_SCOPE",
    "HC_RUNTIME",
    "CI",
    "CONTINUOUS_INTEGRATION",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "FORCE_COLOR",
    "TERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "COLORTERM",
];
const PRESERVED: &[&str] = &[
    "PATH",
    "HOME",
    "CODEX_HOME",
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "SSH_AUTH_SOCK",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "CARGO_TARGET_DIR",
    "RUSTUP_DIST_SERVER",
    "RUSTUP_UPDATE_ROOT",
    "CARGO_HTTP_PROXY",
    "CARGO_HTTPS_PROXY",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "HERDR_PROXY",
    "CUSTOM_PROXY",
    "HERDR_BIN_PATH",
];

#[test]
fn parent_metadata_is_removed_before_core_and_tui_server_holder_agent_inheritance() {
    for runtime in [Runtime::Core, Runtime::Tui] {
        let dir = common::directory();
        let record =
            Record::prepare(common::config(dir.path(), runtime), &dir.path().join("i")).unwrap();
        let output = poisoned_parent(&record).output().unwrap();
        assert!(output.status.success());
        let mut report: Value = serde_json::from_slice(&output.stdout).unwrap();
        for _ in 0..3 {
            assert_eq!(report["removed"], true);
            assert_eq!(report["preserved"], true);
            report = report["child"].take();
        }
        assert!(report.is_null());
    }
}
fn poisoned_parent(record: &Record) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_transport-fixture"));
    for name in REMOVED.iter().chain(PRESERVED) {
        cmd.env(name, "crewmoor-environment-fixture");
    }
    cmd.env("CREWMOOR_PROBE_REMOVED", REMOVED.join(","))
        .env("CREWMOOR_PROBE_PRESERVED", PRESERVED.join(","))
        .arg("environment-parent")
        .arg(&record.root);
    cmd
}
