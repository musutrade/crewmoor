//! One explicit local native endpoint. No service discovery, redirects or retries.
use crate::Result;
use reqwest::{
    Url,
    blocking::{Client, Response},
    redirect,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

pub struct Api {
    client: Client,
    url: Url,
    password: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptReceipt {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub delivery: String,
}

impl Api {
    pub fn stream(&self) -> Result<Response> {
        let client = Client::builder()
            .redirect(redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        Ok(client
            .get(self.url.join("/api/event")?)
            .basic_auth("opencode", Some(&self.password))
            .send()?
            .error_for_status()?)
    }
    pub fn new(url: &str, password: &str, timeout: Duration) -> Result<Self> {
        let url = local_url(url)?;
        if password.is_empty() {
            return Err("missing private OpenCode credential".into());
        }
        let client = Client::builder()
            .redirect(redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(timeout)
            .build()?;
        Ok(Self {
            client,
            url,
            password: password.into(),
        })
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        let request = self
            .client
            .get(self.url.join(path)?)
            .basic_auth("opencode", Some(&self.password));
        decode(request.send()?)
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let request = self
            .client
            .post(self.url.join(path)?)
            .basic_auth("opencode", Some(&self.password))
            .json(body);
        decode(request.send()?)
    }

    pub fn session(&self, session: &str) -> Result<Value> {
        let value = self.get(&session_path(session, None)?)?;
        if value["data"]["id"].as_str() != Some(session) {
            return Err("OpenCode returned a different native session".into());
        }
        Ok(value["data"].clone())
    }

    /// The text is the exact CrewMoor envelope, never decoded from content.
    pub fn prompt(&self, session: &str, text: &str) -> Result<PromptReceipt> {
        let value = self.post(
            &session_path(session, Some("prompt"))?,
            &json!({"text":text}),
        )?;
        prompt_receipt(session, value)
    }
}

pub fn prompt_receipt(session: &str, value: Value) -> Result<PromptReceipt> {
    let receipt: PromptReceipt = serde_json::from_value(value["data"].clone())?;
    crate::feedback::model::identifier(&receipt.id)?;
    if receipt.session_id != session || !matches!(receipt.delivery.as_str(), "steer" | "queue") {
        return Err("OpenCode prompt receipt session/delivery differs".into());
    }
    Ok(receipt)
}

pub fn session_path(session: &str, suffix: Option<&str>) -> Result<String> {
    let mut url = Url::parse("http://127.0.0.1/")?;
    {
        let mut parts = url
            .path_segments_mut()
            .map_err(|_| "invalid local API base")?;
        parts.push("api").push("session").push(session);
        if let Some(suffix) = suffix {
            parts.push(suffix);
        }
    }
    Ok(url.path().into())
}

fn local_url(value: &str) -> Result<Url> {
    let url = Url::parse(value)?;
    if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") || url.port().is_none() {
        return Err("OpenCode endpoint must be the returned private loopback HTTP URL".into());
    }
    clean_url(url)
}

fn clean_url(url: Url) -> Result<Url> {
    if !url.username().is_empty() || url.password().is_some() || url.path() != "/" {
        return Err("unexpected OpenCode endpoint credentials/path".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("unexpected OpenCode endpoint query/fragment".into());
    }
    Ok(url)
}

fn decode(response: Response) -> Result<Value> {
    let status = response.status();
    let body: Value = response.json()?;
    if !status.is_success() {
        return Err(format!("OpenCode native HTTP {}: {body}", status.as_u16()).into());
    }
    Ok(body)
}
