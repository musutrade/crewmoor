//! Synthetic protocol fixture. Actual OpenCode acceptance is recorded separately.
use crewmoor::Result;
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::OpenOptionsExt,
    sync::{Arc, Condvar, Mutex},
    thread,
};

type End = Arc<(Mutex<bool>, Condvar)>;
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("opencode v2.2.5");
            Ok(())
        }
        Some("serve") => serve(),
        Some("--server") => {
            trace("TUI", "argv", &json!(args))?;
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input)?;
            Ok(())
        }
        _ => Err("unsupported synthetic OpenCode invocation".into()),
    }
}
fn serve() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let url = format!("http://{address}");
    let end: End = Arc::new((Mutex::new(false), Condvar::new()));
    let ended = Arc::clone(&end);
    thread::spawn(move || {
        let _ = std::io::stdin().read_to_end(&mut Vec::new());
        if let Ok(mut stopped) = ended.0.lock() {
            *stopped = true;
            ended.1.notify_all();
        }
        if let Ok(mut socket) = TcpStream::connect(address) {
            let _ = socket.write_all(b"GET /fixture-exit HTTP/1.1\r\nHost: localhost\r\n\r\n");
        }
    });
    println!("{}", json!({"url":url}));
    std::io::stdout().flush()?;
    connections(listener, &url, &end)
}
fn connections(listener: TcpListener, url: &str, end: &End) -> Result<()> {
    let mut created = json!({});
    for connection in listener.incoming() {
        let mut connection = connection?;
        let (method, path, body) = request(&connection)?;
        if path == "/fixture-exit" {
            break;
        }
        trace(&method, &path, &body)?;
        if path == "/api/event" {
            sse(connection, Arc::clone(end));
            continue;
        }
        let (status, value) = route(url, &method, &path, body, &mut created);
        respond(&mut connection, status, &value)?;
    }
    Ok(())
}
fn request(stream: &TcpStream) -> Result<(String, String, Value)> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let mut fields = first.split_whitespace();
    let method = fields.next().ok_or("missing method")?.to_owned();
    let path = fields.next().ok_or("missing path")?.to_owned();
    let length = headers(&mut reader)?;
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((method, path, body))
}
fn headers(reader: &mut impl BufRead) -> Result<usize> {
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" {
            return Ok(length);
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse()?;
        }
    }
}
fn respond(stream: &mut TcpStream, status: u16, value: &Value) -> Result<()> {
    let text = value.to_string();
    write!(
        stream,
        "HTTP/1.1 {status} fixture\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{text}",
        text.len()
    )?;
    Ok(())
}
fn route(url: &str, method: &str, path: &str, body: Value, created: &mut Value) -> (u16, Value) {
    match (method, path) {
        ("GET", "/api/info") => (
            200,
            json!({"pid":std::process::id(),"version":"2.2.5","urls":[url],"paths":{"tmp":"/fixture"}}),
        ),
        ("POST", "/api/session") => {
            *created = body;
            (200, json!({"data":session("ses_A",created)}))
        }
        _ => session_route(method, path, body, created),
    }
}
fn session_route(method: &str, path: &str, body: Value, created: &Value) -> (u16, Value) {
    let Some(tail) = path.strip_prefix("/api/session/") else {
        return (404, json!({"_tag":"SessionNotFoundError"}));
    };
    if tail.contains("missing") {
        return (404, json!({"_tag":"SessionNotFoundError"}));
    }
    if method == "POST"
        && let Some(id) = tail.strip_suffix("/prompt")
    {
        return (
            200,
            json!({"data":{"id":"msg_fixture","sessionID":id,"time":{"created":1},"type":"user","payload":{"text":body["text"]},"delivery":"queue"}}),
        );
    }
    if method == "GET" {
        return (200, json!({"data":session(tail,created)}));
    }
    (400, json!({"_tag":"InvalidRequestError"}))
}
fn session(id: &str, created: &Value) -> Value {
    // Shape captured from actual 2.2.5 session.get in the batch's product-core-001.
    let mut value = json!({"id":id,"projectID":"fixture-project","cost":0,"tokens":{"input":0,"output":0,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1,"updated":1},"location":{"directory":"/fixture"}});
    for key in ["agent", "model", "location", "title"] {
        if let Some(field) = created.get(key) {
            value[key] = field.clone();
        }
    }
    value
}
fn trace(method: &str, path: &str, body: &Value) -> Result<()> {
    let Some(pathname) = std::env::var_os("CM_OPENCODE_TEST_TRACE") else {
        return Ok(());
    };
    let row = json!({"method":method,"path":path,"body":body});
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(pathname)?;
    writeln!(file, "{row}")?;
    Ok(())
}
fn sse(mut stream: TcpStream, end: End) {
    thread::spawn(move || {
        let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\nevent: message\ndata: {\"id\":\"evt_fixture\",\"type\":\"server.connected\",\"data\":{}}\n\n");
        if let Ok(mut stopped) = end.0.lock() {
            while !*stopped {
                let Ok(next) = end.1.wait(stopped) else {
                    return;
                };
                stopped = next;
            }
        }
    });
}
