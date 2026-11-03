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
#[test]
fn read_only_receipt_probe_reports_only_whitelisted_marker_names() {
    for runtime in [Runtime::Core, Runtime::Tui] {
        let (_dir, record, _fake) = common::launch(runtime);
        let output = Command::new(env!("CARGO_BIN_EXE_transport-fixture"))
            .arg("environment-evidence")
            .arg(&record.root)
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(!text.contains("synthetic-transport-session"));
        let report: Value = serde_json::from_str(&text).unwrap();
        let list = report["processes"].as_array().unwrap();
        assert_eq!(list.len(), 3);
        for process in list {
            assert!(
                process["present_markers"]
                    .as_array()
                    .unwrap()
                    .contains(&Value::from("CODEX_SESSION_ID"))
            );
        }
    }
}
#[test]
fn app_direct_exec_strips_inherited_metadata_preserves_native_scope_and_execs_exact_agent_argv() {
    use crewmoor::{app, config::AgentKind, feedback::INSTANCE_ENV, native::strings};
    for agent in [AgentKind::Claude, AgentKind::Codex] {
        for instance in [None, Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")] {
            let (dir, mut config, _) = common::setup(Runtime::Core);
            config.agent = agent.clone();
            config.app = Some(super::app_checks::binding(dir.path()));
            config.feedback_instance = instance.map(Into::into);
            config.agent_args = strings(&["--model", "two words ' ; $(not-a-shell)\n保留"]);
            let record = Record::prepare(config, &dir.path().join("app-env")).unwrap();
            let (_, params) = app::request(&record, &strings(&["run"])).unwrap();
            let argv: Vec<String> = serde_json::from_value(params["argv"].clone()).unwrap();
            let mut cmd = Command::new(&argv[0]);
            cmd.args(&argv[1..]);
            for key in app::CHILD_MARKERS {
                cmd.env(key, "synthetic-inherited-marker");
            }
            let preserved: Vec<_> = PRESERVED
                .iter()
                .copied()
                .chain([
                    "HC_SESSION",
                    "HC_SOCKET",
                    "HC_TERMINAL_ID",
                    "HC_TERMINAL_SOCKET",
                    "HC_SCOPE",
                    "HC_RUNTIME",
                    "TERM",
                    "TERM_PROGRAM",
                    "COLORTERM",
                ])
                .collect();
            for key in &preserved {
                cmd.env(key, "crewmoor-environment-fixture");
            }
            for (key, value) in params["env"].as_object().unwrap() {
                cmd.env(key, value.as_str().unwrap());
            }
            if let Some(instance) = instance {
                cmd.env("CREWMOOR_EXPECTED_INSTANCE", instance);
            } else {
                cmd.env_remove("CREWMOOR_EXPECTED_INSTANCE");
            }
            let removed: Vec<_> = app::CHILD_MARKERS
                .iter()
                .copied()
                .filter(|key| *key != INSTANCE_ENV || instance.is_none())
                .collect();
            cmd.env("CREWMOOR_APP_ENV_PROBE", "1")
                .env("CREWMOOR_PROBE_REMOVED", removed.join(","))
                .env("CREWMOOR_PROBE_PRESERVED", preserved.join(","))
                .current_dir(dir.path())
                .stdout(std::process::Stdio::piped());
            let child = cmd.spawn().unwrap();
            let pid = child.id();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["removed"], true);
            assert_eq!(report["preserved"], true);
            assert_eq!(report["feedback"], true);
            assert_eq!(report["argv"], serde_json::json!(record.config.argv()));
            assert_eq!(
                report["executable"],
                serde_json::json!(record.config.agent_executable.canonicalize().unwrap())
            );
            assert_eq!(
                report["pid"], pid,
                "env must exec the Agent in the same child"
            );
        }
    }
}
