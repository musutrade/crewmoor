//! Public checker-to-verifier protocol tests. This Rust test target also serves
//! as its own trusted verifier child, with no test-harness output on that path.

//! The whole target (including the child code) is inventoried and instrumented.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
type Result<T> = tool_rule_hook::Result<T>;
const INSTANCE_ENV: &str = "TOOL_RULE_TEST_INSTANCE";
const SESSION: &str = "native-caller-a";
const SCOPE: &str = "trusted-test-realm";
const INSTANCE: &str = "trusted-instance-a";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--verifier-child") {
        return verifier(&args[1..]);
    }
    valid_verifier_receives_native_event_bound_context_and_identity();
    invalid_verifier_bindings_are_rejected_without_publishing_a_session();
    verifier_failures_deny_instead_of_falling_back_to_independent_rules();
    optional_native_hook_keeps_trusted_session_and_instance_checks();
    codex_binding_passes_native_mode_and_keeps_verifier_failures_strict();
    println!("5 public session-verifier protocol cases passed");
    Ok(())
}

fn verifier(args: &[String]) -> Result<()> {
    let [mode, trace] = args else {
        return Err("verifier fixture arguments missing".into());
    };
    let request: Value = serde_json::from_reader(std::io::stdin())?;
    let path =
        PathBuf::from(std::env::var_os("TOOL_RULE_HOOK_CONTEXT").ok_or("bound context missing")?);
    let context: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let instance = std::env::var(INSTANCE_ENV)?;
    record(Path::new(trace), &request, &path, &instance)?;
    verify_identity(&request, &context, &instance)?;
    respond(mode, &context)
}
fn record(trace: &Path, request: &Value, context: &Path, instance: &str) -> Result<()> {
    let data = json!({"request":request,"context_path":context,"instance":instance});
    fs::write(trace, serde_json::to_vec(&data)?)?;
    Ok(())
}
fn verify_identity(request: &Value, context: &Value, instance: &str) -> Result<()> {
    let (session, directory) = match request["bridge"].as_bool() {
        Some(true) => ("sessionID", "directory"),
        Some(false) => ("session_id", "cwd"),
        None => return Err("checker protocol requires a bridge flag".into()),
    };
    if request["input"][session] != context["session"] {
        return Err("calling session differs from trusted verifier context".into());
    }
    if context["instance"].as_str() != Some(instance) {
        return Err("bound verifier instance differs".into());
    }
    if request["input"][directory] != context["directory"] {
        return Err("calling directory differs from trusted verifier context".into());
    }
    Ok(())
}
fn respond(mode: &str, context: &Value) -> Result<()> {
    match mode {
        "context" => println!("{}", json!({"context":context["label"]})),
        "process-error" => return Err("trusted verifier rejected this request".into()),
        "invalid-json" => println!("invalid verifier JSON"),
        "missing-context" => println!("{{}}"),
        "wrong-context-type" => println!("{}", json!({"context":17})),
        _ => return Err("unknown verifier fixture mode".into()),
    }
    Ok(())
}

struct Fixture {
    root: tempfile::TempDir,
    store: PathBuf,
    rules: PathBuf,
    decoy: PathBuf,
    context: PathBuf,
    trace: PathBuf,
    verifier: Vec<String>,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let rules = write_rules(root.path(), "selected", "restricted");
        let decoy = write_rules(root.path(), "standalone-decoy", "unrelated");
        let context = root.path().join("trusted-context.json");
        fs::write(&context,json!({"session":SESSION,"instance":INSTANCE,"directory":root.path(),"label":"trusted actor A"}).to_string()).unwrap();
        let trace = root.path().join("verifier-trace.json");
        let verifier = vec![
            std::env::current_exe().unwrap().display().to_string(),
            "--verifier-child".into(),
            mode.into(),
            trace.display().to_string(),
        ];
        Self {
            store: root.path().join("owned-store"),
            root,
            rules,
            decoy,
            context,
            trace,
            verifier,
        }
    }
    fn selection(&self) -> Value {
        json!({"scope":SCOPE,"session":SESSION,"directory":self.root.path(),"rules":self.rules,
            "verifier":{"argv":self.verifier,"context":self.context,"env":{INSTANCE_ENV:INSTANCE}}})
    }
    fn event(&self, command: &str) -> Value {
        json!({"tool":"shell","sessionID":SESSION,"directory":self.root.path(),"workspace":null,
            "parentID":"unrelated-parent","messageID":"native-message","id":"native-call",
            "role":"untrusted-role-claim","instance":"untrusted-instance-claim",
            "input":{"command":command}})
    }
    fn checker(&self, args: &[&str], event: &Value) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"));
        command
            .args(args)
            .env("TOOL_RULE_HOOK_RULES", &self.decoy)
            .env_remove("TOOL_RULE_HOOK_VERIFY")
            .env_remove("TOOL_RULE_HOOK_CONTEXT")
            .env_remove("CREWMOOR_FEEDBACK_INSTANCE");
        capture(&mut command, event)
    }
    fn native_hook(&self, event: &Value) -> Value {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"));
        command
            .arg("optional-hook")
            .env("TOOL_RULE_HOOK_RULES", &self.rules)
            .env("TOOL_RULE_HOOK_CONTEXT", &self.context)
            .env(
                "TOOL_RULE_HOOK_VERIFY",
                serde_json::to_string(&self.verifier).unwrap(),
            )
            .env(INSTANCE_ENV, INSTANCE)
            .env_remove("CREWMOOR_FEEDBACK_INSTANCE");
        decode(capture(&mut command, event))
    }
    fn bind(&self, selection: &Value) -> Output {
        self.checker(&["session-bind", self.store.to_str().unwrap()], selection)
    }
    fn bridge(&self, event: &Value) -> Value {
        decode(self.checker(
            &["bridge-session", self.store.to_str().unwrap(), SCOPE],
            event,
        ))
    }
    fn standalone_control(&self) {
        let input = json!({"hook_event_name":"PreToolUse","tool_name":"Bash","session_id":SESSION,
            "cwd":self.root.path(),"tool_input":{"command":"restricted"}});
        assert_eq!(decode(self.checker(&["hook"], &input)), json!({}));
    }
}
fn capture(command: &mut Command, event: &Value) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn write_rules(root: &Path, name: &str, entry: &str) -> PathBuf {
    let path = root.join(format!("{name}.json"));
    fs::write(&path,json!({"base_dir":root,"entries":[{"argv":[entry],"label":name}],"feedback":format!("Policy {name}")}).to_string()).unwrap();
    path
}
fn decode(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn assert_checker_failure(decision: &Value) {
    assert_eq!(decision["decision"], "deny");
    assert!(
        decision["reason"]
            .as_str()
            .unwrap()
            .contains("checker failure")
    );
}

fn valid_verifier_receives_native_event_bound_context_and_identity() {
    let fixture = Fixture::new("context");
    assert_eq!(decode(fixture.bind(&fixture.selection()))["revision"], 1);
    let ordinary = fixture.event("ordinary");
    assert_eq!(fixture.bridge(&ordinary)["decision"], "no_match");
    let trace: Value = serde_json::from_slice(&fs::read(&fixture.trace).unwrap()).unwrap();
    assert_eq!(trace["request"], json!({"bridge":true,"input":ordinary}));
    assert_eq!(trace["context_path"], fixture.context.display().to_string());
    assert_eq!(trace["instance"], INSTANCE);
    let denied = fixture.bridge(&fixture.event("restricted"));
    assert_eq!(denied["decision"], "deny");
    let reason = denied["reason"].as_str().unwrap();
    assert!(reason.contains("Policy selected"));
    assert!(reason.contains("verified_context=trusted actor A"));
    assert!(reason.contains(SESSION));
    let mut context: Value = serde_json::from_slice(&fs::read(&fixture.context).unwrap()).unwrap();
    context["session"] = json!("other-native-session");
    fs::write(&fixture.context, context.to_string()).unwrap();
    assert_checker_failure(&fixture.bridge(&ordinary));
}

fn invalid_verifier_bindings_are_rejected_without_publishing_a_session() {
    let fixture = Fixture::new("context");
    for kind in ["empty-argv", "relative-program", "relative-context"] {
        let mut selection = fixture.selection();
        match kind {
            "empty-argv" => selection["verifier"]["argv"] = json!([]),
            "relative-program" => selection["verifier"]["argv"][0] = json!("relative-verifier"),
            _ => selection["verifier"]["context"] = json!("relative-context.json"),
        }
        assert!(!fixture.bind(&selection).status.success(), "{kind}");
        assert!(!fixture.store.join("bindings.json").exists());
        assert_checker_failure(&fixture.bridge(&fixture.event("ordinary")));
        assert!(!fixture.trace.exists());
    }
}

fn verifier_failures_deny_instead_of_falling_back_to_independent_rules() {
    for unrestricted in [false, true] {
        for mode in [
            "process-error",
            "invalid-json",
            "missing-context",
            "wrong-context-type",
        ] {
            let fixture = Fixture::new(mode);
            fixture.standalone_control();
            let mut selection = fixture.selection();
            if unrestricted {
                selection["rules"] = Value::Null;
            }
            assert_eq!(decode(fixture.bind(&selection))["revision"], 1);
            assert_checker_failure(&fixture.bridge(&fixture.event("restricted")));
            assert!(fixture.trace.is_file());
        }
    }
}

fn optional_native_hook_keeps_trusted_session_and_instance_checks() {
    let fixture = Fixture::new("context");
    let event = json!({"hook_event_name":"PreToolUse","tool_name":"Bash","session_id":SESSION,
        "cwd":fixture.root.path(),"tool_input":{"command":"ordinary"},
        "role":"untrusted-role-claim","instance":"untrusted-instance-claim"});
    assert_eq!(fixture.native_hook(&event), json!({}));
    let trace: Value = serde_json::from_slice(&fs::read(&fixture.trace).unwrap()).unwrap();
    assert_eq!(trace["request"], json!({"bridge":false,"input":event}));
    let mut restricted = event.clone();
    restricted["tool_input"]["command"] = json!("restricted");
    let denied = fixture.native_hook(&restricted);
    assert_eq!(denied["hookSpecificOutput"]["permissionDecision"], "deny");
    let reason = denied["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap();
    assert!(reason.contains("trusted actor A"));
    assert!(reason.contains("Policy selected"));
    let mut context: Value = serde_json::from_slice(&fs::read(&fixture.context).unwrap()).unwrap();
    for field in ["session", "instance"] {
        let original = context[field].clone();
        context[field] = json!("wrong-identity");
        fs::write(&fixture.context, context.to_string()).unwrap();
        let denied = fixture.native_hook(&event);
        assert_eq!(denied["hookSpecificOutput"]["permissionDecision"], "deny");
        let reason = denied["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        assert!(reason.contains("checker failure"));
        context[field] = original;
    }
}

fn codex_binding_passes_native_mode_and_keeps_verifier_failures_strict() {
    for mode in [
        "context",
        "process-error",
        "invalid-json",
        "missing-context",
        "wrong-context-type",
    ] {
        let fixture = Fixture::new(mode);
        let mut selection = fixture.selection();
        selection["scope"] = json!(SESSION);
        let store = fixture.root.path().join("tool-rule-hook");
        assert_eq!(
            decode(fixture.checker(&["codex-session-bind", store.to_str().unwrap()], &selection))["revision"],
            1
        );
        let event = json!({"hook_event_name":"PreToolUse","tool_name":"Bash","session_id":SESSION,
            "cwd":fixture.root.path(),"tool_input":{"command":"restricted"}});
        let mut checker = Command::new(env!("CARGO_BIN_EXE_tool-rule-hook"));
        checker
            .arg("codex-hook")
            .env("PLUGIN_DATA", fixture.root.path())
            .env("TOOL_RULE_HOOK_RULES", &fixture.decoy)
            .env_remove("TOOL_RULE_HOOK_CONTEXT")
            .env_remove("TOOL_RULE_HOOK_VERIFY");
        let decision = decode(capture(&mut checker, &event));
        assert_eq!(decision["hookSpecificOutput"]["permissionDecision"], "deny");
        let reason = decision["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .unwrap();
        let expected = if mode == "context" {
            "trusted actor A"
        } else {
            "checker failure"
        };
        assert!(reason.contains(expected));
        let trace: Value = serde_json::from_slice(&fs::read(&fixture.trace).unwrap()).unwrap();
        assert_eq!(trace["request"], json!({"bridge":false,"input":event}));
    }
}
