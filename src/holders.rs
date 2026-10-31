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
    list.iter().map(parse).collect()
}
fn parse(value: &Value) -> Result<Holder> {
    Ok(Holder {
        instance: text(value, "/instance")?,
        process: process::identity(pid(value, "/pid")?)?,
    })
}
