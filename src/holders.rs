use crate::{
    Result,
    native::{pid, text},
    process,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Holder {
    pub instance: String,
    pub process: process::Identity,
}
pub fn read(status: &Value) -> Result<Vec<Holder>> {
    let list = status["holders"]
        .as_array()
        .ok_or("holder inventory unknown")?;
    if list.len() > 1 {
        return Err("multiple holders; refusing ownership assumption".into());
    }
    let current = status.get("holder").filter(|value| !value.is_null());
    if list.first() != current {
        return Err("current holder disagrees with holder inventory".into());
    }
    list.iter().map(parse).collect()
}
fn parse(value: &Value) -> Result<Holder> {
    if value["current"].as_bool() != Some(true) {
        return Err("non-current holder; ownership unknown".into());
    }
    let instance = text(value, "/instance")?;
    if instance.is_empty() {
        return Err("empty holder instance".into());
    }
    Ok(Holder {
        instance,
        process: process::identity(pid(value, "/pid")?)?,
    })
}
