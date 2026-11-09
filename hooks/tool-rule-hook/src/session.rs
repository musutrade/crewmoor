//! Explicit controller-owned bindings. Native tool calls only read this store.
use crate::{Policy, Result, hook};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Verifier {
    pub argv: Vec<String>,
    pub context: PathBuf,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub scope: String,
    pub session: String,
    pub directory: PathBuf,
    pub workspace: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub rules: Option<PathBuf>,
    pub verifier: Option<Verifier>,
    #[serde(deserialize_with = "required_option")]
    pub policy: Option<Policy>,
    pub revision: u64,
    pub ended: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    scope: String,
    session: String,
    directory: PathBuf,
    workspace: Option<String>,
    #[serde(deserialize_with = "required_option")]
    rules: Option<PathBuf>,
    verifier: Option<Verifier>,
    expected_revision: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    scope: String,
    session: String,
    expected_revision: Option<u64>,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Store {
    bindings: BTreeMap<String, Binding>,
}
fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}
fn key(scope: &str, session: &str) -> Result<String> {
    crate::nonblank(scope)?;
    crate::nonblank(session)?;
    Ok(serde_json::to_string(&("opencode", scope, session))?)
}
fn read(root: &Path) -> Result<Store> {
    crate::absolute(root)?;
    match fs::read_to_string(root.join("bindings.json")) {
        Ok(text) => {
            crate::bounded_source(&text)?;
            Ok(serde_json::from_str(&text)?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(error) => Err(error.into()),
    }
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::absolute(path)?;
    let parent = path.parent().ok_or("atomic destination has no parent")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}
fn mutate(root: &Path, operation: impl FnOnce(&mut Store) -> Result<Value>) -> Result<Value> {
    crate::absolute(root)?;
    fs::create_dir_all(root)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("bindings.lock"))?;
    lock.lock()?;
    let mut store = read(root)?;
    let result = operation(&mut store)?;
    publish(root, &store)?;
    Ok(result)
}
fn publish(root: &Path, store: &Store) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(store)?;
    crate::bounded_source(std::str::from_utf8(&bytes)?)?;
    atomic(&root.join("bindings.json"), &bytes)
}
pub fn control(mode: &str, root: &Path, input: Value) -> Result<Value> {
    match mode {
        "session-bind" | "session-update" => select(root, input, mode == "session-update"),
        "session-end" => end(root, serde_json::from_value(input)?),
        "session-show" => show(root, serde_json::from_value(input)?),
        _ => Err("unknown session control operation".into()),
    }
}
fn select(root: &Path, input: Value, updating: bool) -> Result<Value> {
    let selection: Selection = serde_json::from_value(input)?;
    let binding = prepare(&selection)?;
    let identity = key(&selection.scope, &selection.session)?;
    mutate(root, |store| {
        let revision = next_revision(
            store.bindings.get(&identity),
            selection.expected_revision,
            updating,
        )?;
        let binding = Binding {
            revision,
            ..binding
        };
        store.bindings.insert(identity, binding);
        Ok(json!({"revision":revision}))
    })
}
fn prepare(selection: &Selection) -> Result<Binding> {
    crate::absolute(&selection.directory)?;
    let policy = selection.rules.as_deref().map(hook::load).transpose()?;
    if let Some(policy) = &policy {
        policy.validate()?;
    }
    if let Some(verifier) = &selection.verifier {
        verifier.validate()?;
    }
    Ok(Binding {
        scope: selection.scope.clone(),
        session: selection.session.clone(),
        directory: selection.directory.clone(),
        workspace: selection.workspace.clone(),
        rules: selection.rules.clone(),
        verifier: selection.verifier.clone(),
        policy,
        revision: 1,
        ended: false,
    })
}
fn next_revision(current: Option<&Binding>, expected: Option<u64>, updating: bool) -> Result<u64> {
    if !updating {
        if current.is_some() || expected.is_some() {
            return Err(
                "session already bound or unexpected revision; binding was not replaced".into(),
            );
        }
        return Ok(1);
    }
    let current = current.ok_or("cannot update an unbound session")?;
    if current.ended || expected != Some(current.revision) {
        return Err("session ended or revision conflict; previous binding retained".into());
    }
    Ok(current
        .revision
        .checked_add(1)
        .ok_or("revision exhausted")?)
}
fn end(root: &Path, identity: Identity) -> Result<Value> {
    let key = key(&identity.scope, &identity.session)?;
    mutate(root, |store| {
        let current = store.bindings.get_mut(&key).ok_or("session is unbound")?;
        current.revision = next_revision(Some(current), identity.expected_revision, true)?;
        current.ended = true;
        Ok(json!({"revision":current.revision,"ended":true}))
    })
}
fn show(root: &Path, identity: Identity) -> Result<Value> {
    let store = read(root)?;
    let binding = store
        .bindings
        .get(&key(&identity.scope, &identity.session)?)
        .ok_or("session is unbound")?;
    Ok(
        json!({"scope":binding.scope,"session":binding.session,"revision":binding.revision,"ended":binding.ended,"rules":binding.rules}),
    )
}
pub fn resolve(root: &Path, scope: &str, event: &Value) -> Result<Binding> {
    let session = event["sessionID"]
        .as_str()
        .ok_or("native calling session missing")?;
    let store = read(root)?;
    let binding=store.bindings.get(&key(scope,session)?).ok_or("native session is unbound; trusted controller must bind this exact session; no parent inheritance")?;
    if binding.ended {
        return Err("native session binding has ended; start a new bound session".into());
    }
    check_location(binding, event)?;
    Ok(binding.clone())
}
fn check_location(binding: &Binding, event: &Value) -> Result<()> {
    if event["directory"].as_str() != binding.directory.to_str() {
        return Err("native session directory differs from bound location".into());
    }
    if event["workspace"].as_str() != binding.workspace.as_deref() {
        return Err("native session workspace differs from bound location".into());
    }
    Ok(())
}
impl Verifier {
    fn validate(&self) -> Result<()> {
        let program = self
            .argv
            .first()
            .ok_or("context verifier command is empty")?;
        crate::absolute(Path::new(program))?;
        crate::absolute(&self.context)?;
        Ok(())
    }
    pub fn check(&self, event: &Value, budget: u64) -> Result<String> {
        self.validate()?;
        let mut command = std::process::Command::new(&self.argv[0]);
        command
            .args(&self.argv[1..])
            .envs(&self.env)
            .env(hook::CONTEXT_ENV, &self.context);
        let result = hook::execute(
            command,
            &json!({"bridge":true,"input":event}),
            budget.min(1_000),
        )?;
        Ok(result["context"]
            .as_str()
            .ok_or("context verifier response missing")?
            .into())
    }
}
