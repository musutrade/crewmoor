use crate::Result;
use serde_json::Value;

#[derive(Debug)]
pub struct NativeFailure {
    pub code: String,
    pub message: String,
}
impl NativeFailure {
    pub fn decode(value: &Value) -> Self {
        let code = value
            .pointer("/error/kind")
            .or_else(|| value.pointer("/error/code"));
        Self {
            code: code.and_then(Value::as_str).unwrap_or("unknown").into(),
            message: value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("unstructured native error")
                .into(),
        }
    }
}
impl std::fmt::Display for NativeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("native {}: {}", self.code, self.message))
    }
}
impl std::error::Error for NativeFailure {}
pub fn require_absent(result: Result<Value>) -> Result<()> {
    match result {
        Ok(_) => Err("native target still exists after removal".into()),
        Err(error) if missing(error.as_ref()) => Ok(()),
        Err(error) => Err(error),
    }
}
fn missing(error: &(dyn std::error::Error + 'static)) -> bool {
    error.downcast_ref::<NativeFailure>().is_some_and(|error| {
        matches!(
            error.code.as_str(),
            "not_found" | "pane_not_found" | "terminal_not_found"
        )
    })
}
