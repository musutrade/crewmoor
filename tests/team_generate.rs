use crewmoor::{app, team::Team};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{PermissionsExt, symlink},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

struct Fixture {
    dir: tempfile::TempDir,
    _listener: UnixListener,
    input: Value,
}
fn executable(root: &Path, name: &str, config: Value) -> PathBuf {
    let path = root.join(name);
    fs::copy(env!("CARGO_BIN_EXE_transport-fixture"), &path).unwrap();
    fs::write(
        path.with_extension("discovery.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    path
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let socket = root.join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        // Official v0.9.5 flat status fields: running/version/socket; client intentionally differs.
        let runtime = executable(
            &bin,
            "herdr",
            json!({"version":"herdr 0.9.5","status":{
            "status":"running","running":true,"version":"0.9.4","socket":socket,"session":null}}),
        );
        let claude = executable(&bin, "claude", json!({"version":"2.1.316 (Claude Code)"}));
        let codex = executable(&bin, "codex", json!({"version":"codex-cli 0.166.0"}));
        let cwd = root.join("project");
        fs::create_dir(&cwd).unwrap();
        let input = json!({"team":"generation-test","task":"explicit-task","runtime":"tui",
            "runtime_binary":runtime,"runtime_version":"STALE","host":{"socket":socket,"server_version":"STALE"},
            "feedback_registry":root.join("registry"),"evidence":"synthetic","dispatcher":"lead",
            "roles":[{"role":"lead","cwd":cwd,"agent":"claude","agent_executable":claude,"agent_version":"STALE",
                "agent_args":["--model","deepseek/deepseek-flash"]},
                {"role":"worker","cwd":cwd,"agent":"codex","agent_executable":codex,"agent_version":"STALE",
                "agent_args":["--no-alt-screen","--model","gpt-6.1-sol"],"initial_prompt":"Explicit caller prompt: \"保留我\""}]});
        Self {
            dir,
            _listener: listener,
            input,
        }
    }
    fn root(&self) -> &Path {
        self.dir.path()
    }
    fn run(&self, extra: &[&str]) -> Output {
        let template = self.root().join("template.toml");
        fs::write(&template, toml::to_string(&self.input).unwrap()).unwrap();
        Command::new(env!("CARGO_BIN_EXE_crewmoor"))
            .args(["team", "generate"])
            .arg(template)
            .args(extra)
            .env("PATH", self.root().join("bin"))
            .output()
            .unwrap()
    }
    fn status(&self, update: impl FnOnce(&mut Value)) {
        let path = self.root().join("bin/herdr.discovery.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        update(&mut config);
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    }
    fn core(&mut self, server: &str, id: &str, doc: Value) -> thread::JoinHandle<Vec<Value>> {
        self.input["runtime"] = json!("core");
        self.input.as_object_mut().unwrap().remove("host");
        self.input["runtime_binary"] = json!(executable(
            &self.root().join("bin"),
            "hc",
            json!({"version":"hc 0.1.17"})
        ));
        let socket = self.root().join("core.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let app = app::Binding {
            session: "explicit-routing-session".into(),
            socket,
            core_id: "actual-core".into(),
            machine: "client-route".into(),
            space_id: Some("client-route/sp-known".into()),
            tab_id: Some("client-route/tb-one".into()),
        };
        let target = self.root().join("app-target.toml");
        fs::write(&target, toml::to_string(&app).unwrap()).unwrap();
        self.input["app_target"] = json!(target);
        self.input["roles"][1]["tab_id"] = json!("client-route/tb-two");
        // StatusResult has core.id/name/version; session is NOT fabricated as a server field.
        let status = json!({"core":{"id":id,"name":"server-host-name"},"version":server,
            "pid":1234,"uptime_ms":0,"holders":[],"live":0,"finished":0});
        serve(listener, vec![status, doc])
    }
}
fn layout() -> Value {
    json!({"version":1,"value":{"spaces":[{"id":"sp-known","tabs":[{"id":"tb-one"},{"id":"tb-two"}]}]}})
}
fn serve(listener: UnixListener, replies: Vec<Value>) -> thread::JoinHandle<Vec<Value>> {
    listener.set_nonblocking(true).unwrap();
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = Vec::new();
        for result in replies {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(pair) => break pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "expected discovery RPC missing");
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            requests.push(request);
            let reply = json!({"jsonrpc":"2.0","id":1,"result":result});
            stream.write_all(format!("{reply}\n").as_bytes()).unwrap();
        }
        requests
    })
}
fn parse(output: &Output) -> Team {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    toml::from_slice::<Team>(&output.stdout).unwrap()
}
fn assert_choices(team: &Team, input: &Value) {
    let actual = serde_json::to_value(team).unwrap();
    for key in [
        "team",
        "task",
        "runtime",
        "feedback_registry",
        "evidence",
        "dispatcher",
        "app_target",
    ] {
        assert_eq!(actual[key], input[key], "{key}");
    }
    for (actual, expected) in actual["roles"]
        .as_array()
        .unwrap()
        .iter()
        .zip(input["roles"].as_array().unwrap())
    {
        for key in [
            "role",
            "cwd",
            "agent",
            "agent_args",
            "initial_prompt",
            "tab_id",
        ] {
            assert_eq!(actual[key], expected[key], "role {key}");
        }
    }
}
fn failed(output: &Output) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "failed generation emitted stdout");
}

#[test]
fn tui_stdout_keeps_client_and_server_distinct_and_preserves_exact_task_choices() {
    let mut f = Fixture::new();
    let alias = f.root().join("cwd-alias");
    symlink(f.root().join("project"), &alias).unwrap();
    f.input["roles"][0]["cwd"] = json!(alias);
    let binary_alias = f.root().join("agent-link");
    symlink(f.root().join("bin/claude"), &binary_alias).unwrap();
    f.input["roles"][0]["agent_executable"] = json!(binary_alias);
    let out = f.run(&[]);
    let team = parse(&out);
    assert_choices(&team, &f.input);
    assert_eq!(team.runtime_version, "herdr 0.9.5");
    assert_eq!(team.host.unwrap().server_version, "0.9.4");
    assert_eq!(team.roles[0].agent_version, "2.1.316 (Claude Code)");
    assert_eq!(team.roles[1].agent_version, "codex-cli 0.166.0");
    assert_eq!(
        team.roles[0].agent_executable,
        f.root().join("bin/claude").canonicalize().unwrap()
    );
    let queries = fs::read_to_string(f.root().join("bin/herdr.discovery.queries.jsonl")).unwrap();
    assert_eq!(
        queries
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>(),
        vec![json!(["--version"]), json!(["status", "server", "--json"])]
    );
}
#[test]
fn omitted_factual_fields_discover_path_but_explicit_missing_hint_never_falls_back() {
    let mut f = Fixture::new();
    for key in ["runtime_binary", "runtime_version"] {
        f.input.as_object_mut().unwrap().remove(key);
    }
    f.input["host"]
        .as_object_mut()
        .unwrap()
        .remove("server_version");
    for role in f.input["roles"].as_array_mut().unwrap() {
        for key in ["agent_executable", "agent_version"] {
            role.as_object_mut().unwrap().remove(key);
        }
    }
    let team = parse(&f.run(&[]));
    assert!(team.runtime_binary.is_absolute());
    assert!(team.roles.iter().all(|r| r.agent_executable.is_absolute()));
    f.input["runtime_binary"] = json!(f.root().join("missing-herdr"));
    failed(&f.run(&[]));
    f.input.as_object_mut().unwrap().remove("runtime_binary");
    f.input["roles"][0]["agent_executable"] = json!("claude");
    failed(&f.run(&[]));
}
#[test]
fn requested_new_file_is_valid_and_existing_or_symlink_output_is_never_overwritten() {
    let f = Fixture::new();
    let path = f.root().join("generated.toml");
    let out = f.run(&["--output", path.to_str().unwrap()]);
    assert!(out.status.success());
    let bytes = fs::read(&path).unwrap();
    let team: Team = toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_choices(&team, &f.input);
    failed(&f.run(&["--output", path.to_str().unwrap()]));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let link = f.root().join("link.toml");
    symlink(&path, &link).unwrap();
    failed(&f.run(&["--output", link.to_str().unwrap()]));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    failed(&f.run(&[
        "--output",
        f.root().join("no-parent/file").to_str().unwrap(),
    ]));
}
#[test]
fn offline_malformed_missing_and_misrouted_status_never_emit_a_team() {
    for change in [
        "offline",
        "malformed",
        "version",
        "socket",
        "different-socket",
        "query-error",
        "version-error",
    ] {
        let f = Fixture::new();
        let other = f.root().join("other.sock");
        fs::write(&other, "").unwrap();
        f.status(|c| match change {
            "offline" => c["status"]["running"] = json!(false),
            "malformed" => c["malformed_status"] = json!(true),
            "version" => {
                c["status"].as_object_mut().unwrap().remove("version");
            }
            "socket" => {
                c["status"].as_object_mut().unwrap().remove("socket");
            }
            "different-socket" => c["status"]["socket"] = json!(other),
            "query-error" => c["status_error"] = json!(true),
            _ => c["version_error"] = json!(true),
        });
        failed(&f.run(&[]));
    }
}
#[test]
fn exact_launch_constraints_are_checked_without_changing_caller_cwd_or_args() {
    for change in [
        "missing-cwd",
        "relative-cwd",
        "bad-name",
        "bad-args",
        "bad-prompt",
        "bad-tui-arg-control",
        "bad-host",
        "bad-role-tab",
    ] {
        let mut f = Fixture::new();
        match change {
            "missing-cwd" => f.input["roles"][0]["cwd"] = json!(f.root().join("absent")),
            "relative-cwd" => f.input["roles"][0]["cwd"] = json!("relative"),
            "bad-name" => {
                f.input["roles"][0]["role"] = json!("1lead");
                f.input["dispatcher"] = json!("1lead");
            }
            "bad-args" => f.input["roles"][0]["agent_args"] = json!(["--invented"]),
            "bad-prompt" => {
                f.input["roles"][0]["initial_prompt"] = json!("Claude positional prompt forbidden")
            }
            "bad-tui-arg-control" => {
                f.input["roles"][0]["agent_args"] = json!(["--model", "bad\nmodel"])
            }
            "bad-host" => {
                f.input.as_object_mut().unwrap().remove("host");
            }
            _ => f.input["roles"][0]["tab_id"] = json!("tb-impossible-on-tui"),
        }
        failed(&f.run(&[]));
    }
}
#[test]
fn missing_nonexecutable_or_failed_agent_discovery_never_selects_another_agent() {
    for change in ["missing", "nonexecutable", "version-error", "empty-version"] {
        let mut f = Fixture::new();
        let path = f.root().join("bin/claude");
        match change {
            "missing" => f.input["roles"][0]["agent_executable"] = json!(f.root().join("missing")),
            "nonexecutable" => {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap()
            }
            _ => fs::write(
                path.with_extension("discovery.json"),
                serde_json::to_vec(&if change == "version-error" {
                    json!({"version_error":true})
                } else {
                    json!({"version":""})
                })
                .unwrap(),
            )
            .unwrap(),
        }
        failed(&f.run(&[]));
    }
}
#[test]
fn core_uses_existing_profile_and_official_queries_preserving_routing_alias_and_role_tabs() {
    let mut f = Fixture::new();
    let server = f.core("0.1.17", "actual-core", layout());
    let target = PathBuf::from(f.input["app_target"].as_str().unwrap());
    let before = fs::read(&target).unwrap();
    let team = parse(&f.run(&[]));
    assert_choices(&team, &f.input);
    assert_eq!(team.runtime_version, "hc 0.1.17");
    assert_eq!(
        fs::read(&target).unwrap(),
        before,
        "existing app target rewritten"
    );
    let calls = server.join().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0],
        json!({"jsonrpc":"2.0","id":1,"method":"server.status","params":{}})
    );
    assert_eq!(
        calls[1],
        json!({"jsonrpc":"2.0","id":1,"method":"doc.get","params":{"name":"herdr-app"}})
    );
    let app: app::Binding = toml::from_str(std::str::from_utf8(&before).unwrap()).unwrap();
    assert_eq!(app.machine, "client-route");
    assert_eq!(app.session, "explicit-routing-session");
}
#[test]
fn core_mismatch_stale_identity_or_invalid_placement_is_rejected_without_output() {
    for change in [
        "mismatch",
        "stale-core",
        "missing-base-tab",
        "missing-role-tab",
    ] {
        let mut f = Fixture::new();
        let mut doc = layout();
        if change == "missing-base-tab" {
            doc["value"]["spaces"][0]["tabs"] = json!([]);
        }
        let server = f.core(
            if change == "mismatch" {
                "0.1.16"
            } else {
                "0.1.17"
            },
            if change == "stale-core" {
                "replaced-core"
            } else {
                "actual-core"
            },
            doc,
        );
        if change == "missing-role-tab" {
            f.input["roles"][1]["tab_id"] = json!("tb-absent");
        }
        let out = f.run(&[]);
        failed(&out);
        let calls = server.join().unwrap();
        assert_eq!(calls.len(), 2);
        if change == "mismatch" {
            assert!(String::from_utf8_lossy(&out.stderr).contains("client/server versions differ"));
        }
    }
}
#[test]
fn core_existing_target_file_is_mandatory_and_never_generated_as_a_side_effect() {
    let mut f = Fixture::new();
    f.input["runtime"] = json!("core");
    f.input.as_object_mut().unwrap().remove("host");
    let target = f.root().join("not-created.toml");
    f.input["app_target"] = json!(target);
    failed(&f.run(&[]));
    assert!(!target.exists());
    fs::write(&target, "invalid target").unwrap();
    failed(&f.run(&[]));
    assert_eq!(fs::read_to_string(target).unwrap(), "invalid target");
}

#[test]
fn core_project_placement_remains_explicit_and_generation_creates_no_project_store() {
    let mut f = Fixture::new();
    let server = f.core("0.1.17", "actual-core", json!({"version":0,"value":null}));
    let target = PathBuf::from(f.input["app_target"].as_str().unwrap());
    let mut app: app::Binding = toml::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    app.space_id = None;
    app.tab_id = None;
    fs::write(&target, toml::to_string(&app).unwrap()).unwrap();
    f.input["roles"][1]
        .as_object_mut()
        .unwrap()
        .remove("tab_id");
    let before = fs::read(&target).unwrap();
    let team = parse(&f.run(&[]));
    assert_choices(&team, &f.input);
    assert_eq!(fs::read(target).unwrap(), before);
    assert!(!f.root().join("registry").exists());
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn offline_core_profile_fails_without_modification_or_output() {
    let mut f = Fixture::new();
    f.input["runtime"] = json!("core");
    f.input.as_object_mut().unwrap().remove("host");
    f.input["runtime_binary"] = json!(executable(
        &f.root().join("bin"),
        "hc",
        json!({"version":"hc 0.1.17"})
    ));
    let profile = app::Binding {
        session: "chosen-session".into(),
        socket: f.root().join("offline.sock"),
        core_id: "chosen-core".into(),
        machine: "routing-alias".into(),
        space_id: None,
        tab_id: None,
    };
    let target = f.root().join("offline-target.toml");
    let before = toml::to_string(&profile).unwrap();
    fs::write(&target, &before).unwrap();
    f.input["app_target"] = json!(target);
    failed(&f.run(&[]));
    assert_eq!(fs::read_to_string(target).unwrap(), before);
}
#[test]
fn invalid_cli_template_and_unknown_fields_do_not_print_partial_toml() {
    for args in [
        vec!["team", "generate"],
        vec!["team", "generate", "x", "--output"],
        vec!["team", "generate", "x", "--bad", "y"],
    ] {
        failed(
            &Command::new(env!("CARGO_BIN_EXE_crewmoor"))
                .args(args)
                .output()
                .unwrap(),
        );
    }
    let mut f = Fixture::new();
    f.input["invented_field"] = json!(true);
    failed(&f.run(&[]));
}
