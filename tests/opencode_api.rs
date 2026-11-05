use crewmoor::{
    Result,
    config::AgentKind,
    opencode::{
        Options,
        api::{Api, prompt_receipt, session_path},
    },
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

fn request(listener: TcpListener, status: u16, body: Value) -> thread::JoinHandle<(String, Value)> {
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut headers = String::new();
        let mut size = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                size = value.trim().parse().unwrap();
            }
            headers.push_str(&line);
        }
        let mut bytes = vec![0; size];
        reader.read_exact(&mut bytes).unwrap();
        let body_text = body.to_string();
        write!(stream,"HTTP/1.1 {status} test\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body_text}",body_text.len()).unwrap();
        (headers, serde_json::from_slice(&bytes).unwrap())
    })
}

#[test]
fn api_posts_the_exact_envelope_to_bound_a_once() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let received = request(
        listener,
        200,
        json!({"data":{"id":"msg_native_a","sessionID":"ses_bound_A","delivery":"queue","type":"user","time":{"created":1},"payload":{"text":"irrelevant projected value"}}}),
    );
    let api = Api::new(
        &format!("http://{address}"),
        "private-test-password",
        Duration::from_secs(5),
    )?;
    let text = r#"{"team":"中文","message":{"content":"{\"record\":\"Stop\"}"}}"#;
    let receipt = api.prompt("ses_bound_A", text)?;
    assert_eq!(receipt.session_id, "ses_bound_A");
    let (headers, body) = received.join().unwrap();
    assert!(headers.starts_with("POST /api/session/ses_bound_A/prompt HTTP/1.1\r\n"));
    assert!(headers.to_lowercase().contains("authorization: basic "));
    assert_eq!(body, json!({"text":text}));
    Ok(())
}

#[test]
fn wrong_native_receipt_never_becomes_a_new_binding() {
    for value in [
        json!({"data":{"id":"msg_1","sessionID":"ses_B","delivery":"queue"}}),
        json!({"data":{"id":"msg_1","sessionID":"ses_A","delivery":"unexpected"}}),
        json!({"data":{"sessionID":"ses_A","delivery":"queue"}}),
    ] {
        assert!(prompt_receipt("ses_A", value).is_err());
    }
    assert_eq!(
        session_path("ses_/A", Some("prompt")).unwrap(),
        "/api/session/ses_%2FA/prompt"
    );
}

#[test]
fn api_rejects_foreign_endpoint_and_preserves_native_error() -> Result<()> {
    for url in [
        "https://127.0.0.1:42/",
        "http://example.com:42/",
        "http://127.0.0.1:42/wrong",
        "http://127.0.0.1:42/?new=1",
    ] {
        assert!(Api::new(url, "p", Duration::from_secs(1)).is_err());
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = request(
        listener,
        404,
        json!({"_tag":"SessionNotFoundError","sessionID":"ses_absent"}),
    );
    let api = Api::new(&format!("http://{address}"), "p", Duration::from_secs(5))?;
    let error = api
        .prompt("ses_absent", "one attempt")
        .unwrap_err()
        .to_string();
    assert!(error.contains("404"));
    assert!(error.contains("SessionNotFoundError"));
    server.join().unwrap();
    Ok(())
}

#[test]
fn explicit_v2_options_and_prompt_do_not_invent_root_model_flags() -> Result<()> {
    let kind: AgentKind = serde_json::from_value(json!("opencode"))?;
    assert_eq!(kind.name(), "opencode");
    assert!(crewmoor::config::validate_args(&kind, &["--model".into(), "x".into()]).is_err());
    crewmoor::config::initial_prompt(&kind, Some("启动：中文 \"原样\""))?;
    let options: Options =
        serde_json::from_value(json!({"agent":"build","model":{"id":"m","providerID":"p"}}))?;
    options.validate()?;
    assert!(
        options
            .check_session(&json!({"agent":"other","model":{"id":"m","providerID":"p"}}))
            .is_err()
    );
    assert!(
        options
            .check_session(&json!({"agent":"build","model":{"id":"other","providerID":"p"}}))
            .is_err()
    );
    options.check_session(&json!({"agent":"build","model":{"id":"m","providerID":"p"}}))?;
    assert_eq!(
        options.session_payload(std::path::Path::new("/tmp/work"), "dev"),
        json!({"title":"dev","location":{"directory":"/tmp/work"},"agent":"build","model":{"id":"m","providerID":"p"}})
    );
    Ok(())
}
