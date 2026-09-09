//! Unix-domain app-server listen, HTTP/1.1 WebSocket upgrade, and TASK-016
//! JSON-RPC handshake (initialize / initialized / account/read / model/list).

use crate::ops::open_home;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use samchi_core::source_wire::{MAX_HTTP_UPGRADE_BYTES, MAX_WEBSOCKET_FRAME_BYTES};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::thread;

/// Honest initialize identity. Kept out of `samchi-core`.
pub(crate) const USER_AGENT: &str = "samchi-for-grok/app-server-v1";
const METHOD_NOT_FOUND: i64 = -32602;

/// RFC 6455 magic string used with `Sec-WebSocket-Key`.
const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Inverse of Dolgorae's client upgrade request in `src/app_server.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeReject {
    MethodNotAllowed,
    NotFound,
    BadRequest,
    UpgradeRequired,
    HeadersTooLarge,
}

impl UpgradeReject {
    pub fn status_line(self) -> Option<&'static str> {
        match self {
            Self::MethodNotAllowed => Some("HTTP/1.1 405 Method Not Allowed"),
            Self::NotFound => Some("HTTP/1.1 404 Not Found"),
            Self::BadRequest => Some("HTTP/1.1 400 Bad Request"),
            Self::UpgradeRequired => Some("HTTP/1.1 426 Upgrade Required"),
            Self::HeadersTooLarge => None,
        }
    }
}

struct ListenConfig {
    socket_path: PathBuf,
    home: Option<PathBuf>,
}

pub fn run(args: &[&str], stderr: &mut dyn Write) -> u8 {
    let cfg = match parse_args(args) {
        Ok(c) => c,
        Err(msg) => {
            let _ = writeln!(stderr, "SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG");
            let _ = writeln!(stderr, "{msg}");
            return 1;
        }
    };
    let home = match open_home(cfg.home.as_deref()) {
        Ok(h) => h,
        Err(reason) => {
            let _ = writeln!(stderr, "SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG");
            let _ = writeln!(stderr, "{reason}");
            return 1;
        }
    };
    let listener = match bind_listen(&cfg.socket_path) {
        Ok(l) => l,
        Err(BindError::Occupied) => {
            let _ = writeln!(stderr, "SAMCHI_FOR_GROK_STARTUP_ERROR OCCUPIED_PATH");
            return 1;
        }
        Err(BindError::Other(msg)) => {
            let _ = writeln!(stderr, "SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG");
            let _ = writeln!(stderr, "{msg}");
            return 1;
        }
    };
    accept_loop(listener, home);
    0
}

fn parse_args(args: &[&str]) -> Result<ListenConfig, String> {
    let mut i = 0;
    let mut listen = None;
    let mut home = None;
    while i < args.len() {
        match args[i] {
            "--listen" => {
                i += 1;
                let raw = *args
                    .get(i)
                    .ok_or_else(|| "--listen needs a value".to_string())?;
                listen = Some(parse_listen(raw)?);
            }
            "--home" => {
                i += 1;
                let raw = *args
                    .get(i)
                    .ok_or_else(|| "--home needs a value".to_string())?;
                home = Some(PathBuf::from(raw));
            }
            other => return Err(format!("unknown argument {other}")),
        }
        i += 1;
    }
    let socket_path =
        listen.ok_or_else(|| "--listen unix://<absolute-path> is required".to_string())?;
    Ok(ListenConfig { socket_path, home })
}

fn parse_listen(raw: &str) -> Result<PathBuf, String> {
    let path = raw
        .strip_prefix("unix://")
        .ok_or_else(|| "--listen must be unix://<absolute-path>".to_string())?;
    if path.is_empty() || !path.starts_with('/') {
        return Err("--listen path must be absolute".to_string());
    }
    Ok(PathBuf::from(path))
}

#[derive(Debug)]
enum BindError {
    Occupied,
    Other(String),
}

fn bind_listen(path: &Path) -> Result<UnixListener, BindError> {
    if path.exists() {
        return Err(BindError::Occupied);
    }
    UnixListener::bind(path).map_err(|err| {
        if err.kind() == io::ErrorKind::AddrInUse {
            BindError::Occupied
        } else {
            BindError::Other(err.to_string())
        }
    })
}

fn accept_loop(listener: UnixListener, home: PathBuf) {
    for incoming in listener.incoming() {
        let Ok(stream) = incoming else {
            continue;
        };
        let home = home.clone();
        thread::spawn(move || {
            let _ = serve_connection(stream, &home);
        });
    }
}

fn serve_connection(mut stream: UnixStream, home: &Path) -> io::Result<()> {
    match upgrade(&mut stream) {
        Ok(()) => rpc_loop(&mut stream, home),
        Err(UpgradeReject::HeadersTooLarge) => Ok(()),
        Err(reject) => write_reject(&mut stream, reject),
    }
}

fn write_reject(stream: &mut UnixStream, reject: UpgradeReject) -> io::Result<()> {
    let Some(status) = reject.status_line() else {
        return Ok(());
    };
    if reject == UpgradeReject::UpgradeRequired {
        write!(
            stream,
            "{status}\r\nSec-WebSocket-Version: 13\r\nConnection: close\r\n\r\n"
        )?;
    } else {
        write!(stream, "{status}\r\nConnection: close\r\n\r\n")?;
    }
    stream.flush()
}

fn upgrade(stream: &mut UnixStream) -> Result<(), UpgradeReject> {
    let req = read_http_headers(stream)?;
    let (method, target, version) = parse_request_line(&req)?;
    if method != "GET" {
        return Err(UpgradeReject::MethodNotAllowed);
    }
    if version != "HTTP/1.1" {
        return Err(UpgradeReject::BadRequest);
    }
    if target != "/" {
        return Err(UpgradeReject::NotFound);
    }
    let headers = HeaderMap::parse(&req)?;
    if !headers.eq_ignore("upgrade", "websocket") {
        return Err(UpgradeReject::BadRequest);
    }
    if !headers.connection_has_upgrade() {
        return Err(UpgradeReject::BadRequest);
    }
    let key = headers
        .get("sec-websocket-key")
        .ok_or(UpgradeReject::BadRequest)?;
    if !valid_websocket_key(key) {
        return Err(UpgradeReject::BadRequest);
    }
    match headers.get("sec-websocket-version") {
        Some("13") => {}
        _ => return Err(UpgradeReject::UpgradeRequired),
    }
    let accept = websocket_accept(key);
    write!(
        stream,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )
    .and_then(|_| stream.flush())
    .map_err(|_| UpgradeReject::BadRequest)
}

fn read_http_headers(stream: &mut UnixStream) -> Result<Vec<u8>, UpgradeReject> {
    let mut buf = Vec::new();
    let mut byte = [0_u8; 1];
    while buf.len() < MAX_HTTP_UPGRADE_BYTES {
        match stream.read_exact(&mut byte) {
            Ok(()) => buf.push(byte[0]),
            Err(_) => return Err(UpgradeReject::BadRequest),
        }
        if buf.ends_with(b"\r\n\r\n") {
            return Ok(buf);
        }
    }
    Err(UpgradeReject::HeadersTooLarge)
}

fn parse_request_line(req: &[u8]) -> Result<(&str, &str, &str), UpgradeReject> {
    let text = std::str::from_utf8(req).map_err(|_| UpgradeReject::BadRequest)?;
    let line = text.split("\r\n").next().ok_or(UpgradeReject::BadRequest)?;
    let mut parts = line.split(' ');
    let method = parts.next().ok_or(UpgradeReject::BadRequest)?;
    let target = parts.next().ok_or(UpgradeReject::BadRequest)?;
    let version = parts.next().ok_or(UpgradeReject::BadRequest)?;
    if parts.next().is_some() || method.is_empty() || target.is_empty() || version.is_empty() {
        return Err(UpgradeReject::BadRequest);
    }
    Ok((method, target, version))
}

struct HeaderMap<'a> {
    pairs: Vec<(&'a str, &'a str)>,
}

impl<'a> HeaderMap<'a> {
    fn parse(req: &'a [u8]) -> Result<Self, UpgradeReject> {
        let text = std::str::from_utf8(req).map_err(|_| UpgradeReject::BadRequest)?;
        let mut lines = text.split("\r\n");
        let _request_line = lines.next();
        let mut pairs = Vec::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            let (name, value) = line.split_once(':').ok_or(UpgradeReject::BadRequest)?;
            pairs.push((name.trim(), value.trim()));
        }
        Ok(Self { pairs })
    }

    fn get(&self, name: &str) -> Option<&'a str> {
        self.pairs
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
    }

    fn eq_ignore(&self, name: &str, want: &str) -> bool {
        self.get(name).is_some_and(|v| v.eq_ignore_ascii_case(want))
    }

    fn connection_has_upgrade(&self) -> bool {
        self.get("connection").is_some_and(|v| {
            v.split(',')
                .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
        })
    }
}

fn valid_websocket_key(key: &str) -> bool {
    match STANDARD.decode(key) {
        Ok(bytes) => bytes.len() == 16,
        Err(_) => false,
    }
}

fn websocket_accept(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WEBSOCKET_GUID.as_bytes());
    STANDARD.encode(hasher.finalize())
}

fn rpc_loop(stream: &mut UnixStream, _home: &Path) -> io::Result<()> {
    loop {
        match read_ws_text(stream) {
            Ok(None) => return Ok(()),
            Ok(Some(bytes)) => {
                let Ok(msg) = serde_json::from_slice::<Value>(&bytes) else {
                    continue;
                };
                if let Some(reply) = handle_rpc(&msg) {
                    write_ws_text(stream, &serde_json::to_vec(&reply)?)?;
                }
            }
            Err(_) => return Ok(()),
        }
    }
}

fn handle_rpc(msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?;
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let id = msg.get("id").cloned()?;
    if method == "capabilities.ccas"
        || params
            .get("capabilities")
            .and_then(|c| c.get("ccas"))
            .is_some()
    {
        return Some(rpc_error(id, METHOD_NOT_FOUND, "capabilities.ccas"));
    }
    match method {
        "initialize" => Some(json!({"id": id, "result": initialize_result()})),
        "account/read" => Some(json!({"id": id, "result": json!({"requiresOpenaiAuth": false})})),
        "model/list" => Some(json!({"id": id, "result": model_list_result()})),
        _ => Some(rpc_error(id, METHOD_NOT_FOUND, method)),
    }
}

fn initialize_result() -> Value {
    json!({
        "userAgent": USER_AGENT,
    })
}

fn model_list_result() -> Value {
    json!({
        "data": [{
            "model": "grok",
            "isDefault": true,
            "supportedReasoningEfforts": []
        }],
        "nextCursor": Value::Null
    })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"id": id, "error": {"code": code, "message": message}})
}

fn read_ws_text(stream: &mut UnixStream) -> io::Result<Option<Vec<u8>>> {
    loop {
        let Some((fin, opcode, payload)) = read_ws_frame(stream)? else {
            return Ok(None);
        };
        match opcode {
            0x1 if fin => return Ok(Some(payload)),
            0x8 => return Ok(None),
            0x9 => write_ws_frame(stream, 0xA, &payload)?,
            0xA => {}
            _ => return Ok(None),
        }
    }
}

fn write_ws_text(stream: &mut UnixStream, payload: &[u8]) -> io::Result<()> {
    write_ws_frame(stream, 0x1, payload)
}

fn read_ws_frame(stream: &mut UnixStream) -> io::Result<Option<(bool, u8, Vec<u8>)>> {
    let mut prefix = [0_u8; 2];
    match stream.read_exact(&mut prefix) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    if prefix[0] & 0x70 != 0 {
        return Err(io::Error::other("reserved bits"));
    }
    let masked = prefix[1] & 0x80 != 0;
    if !masked {
        return Err(io::Error::other("client frame must be masked"));
    }
    let fin = prefix[0] & 0x80 != 0;
    let opcode = prefix[0] & 0x0f;
    let marker = prefix[1] & 0x7f;
    let length = match marker {
        0..=125 => u64::from(marker),
        126 => {
            let mut bytes = [0_u8; 2];
            stream.read_exact(&mut bytes)?;
            u64::from(u16::from_be_bytes(bytes))
        }
        127 => {
            let mut bytes = [0_u8; 8];
            stream.read_exact(&mut bytes)?;
            u64::from_be_bytes(bytes)
        }
        _ => return Err(io::Error::other("invalid length")),
    };
    if length > MAX_WEBSOCKET_FRAME_BYTES as u64 {
        return Err(io::Error::other("frame too large"));
    }
    let mut mask = [0_u8; 4];
    stream.read_exact(&mut mask)?;
    let mut payload = vec![0_u8; length as usize];
    stream.read_exact(&mut payload)?;
    for (i, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[i % 4];
    }
    Ok(Some((fin, opcode, payload)))
}

fn write_ws_frame(stream: &mut UnixStream, opcode: u8, payload: &[u8]) -> io::Result<()> {
    let mut header = vec![0x80 | opcode];
    let len = payload.len();
    if len <= 125 {
        header.push(len as u8);
    } else if len <= 65535 {
        header.push(126);
        header.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        header.push(127);
        header.extend_from_slice(&(len as u64).to_be_bytes());
    }
    stream.write_all(&header)?;
    stream.write_all(payload)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_sock() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("samchi-as-{nanos}.sock"))
    }

    fn dolgorae_upgrade(key: &str) -> String {
        format!(
            "GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
    }

    fn sample_key() -> String {
        STANDARD.encode([7_u8; 16])
    }

    fn read_headers(stream: &mut UnixStream) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut byte = [0_u8; 1];
        while buf.len() < MAX_HTTP_UPGRADE_BYTES {
            stream.read_exact(&mut byte).expect("byte");
            buf.push(byte[0]);
            if buf.ends_with(b"\r\n\r\n") {
                return buf;
            }
        }
        panic!("headers too large");
    }

    #[test]
    fn parse_listen_requires_unix_absolute() {
        assert!(parse_listen("unix:///tmp/s.sock").is_ok());
        assert!(parse_listen("unix://tmp/s.sock").is_err());
        assert!(parse_listen("tcp://127.0.0.1:1").is_err());
        assert!(parse_listen("unix://").is_err());
    }

    #[test]
    fn occupied_path_is_fail_closed() {
        let path = unique_sock();
        let _listener = bind_listen(&path).expect("first bind");
        assert!(matches!(bind_listen(&path), Err(BindError::Occupied)));
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn occupied_regular_file_is_not_unlinked() {
        let path = unique_sock();
        std::fs::write(&path, b"occupant").unwrap();
        assert!(matches!(bind_listen(&path), Err(BindError::Occupied)));
        assert_eq!(std::fs::read(&path).unwrap(), b"occupant");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn valid_upgrade_matches_dolgorae_client() {
        let (mut server, mut client) = UnixStream::pair().unwrap();
        let key = sample_key();
        let req = dolgorae_upgrade(&key);
        let worker = thread::spawn(move || upgrade(&mut server));
        client.write_all(req.as_bytes()).unwrap();
        client.flush().unwrap();
        let resp = read_headers(&mut client);
        let text = std::str::from_utf8(&resp).unwrap();
        assert!(text.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
        assert!(text.to_ascii_lowercase().contains("upgrade: websocket"));
        let accept = websocket_accept(&key);
        assert!(text.contains(&format!("Sec-WebSocket-Accept: {accept}")));
        worker.join().unwrap().expect("upgrade");
    }

    #[test]
    fn rejection_table() {
        let cases: &[(&str, UpgradeReject)] = &[
            (
                "POST / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: k\r\nSec-WebSocket-Version: 13\r\n\r\n",
                UpgradeReject::MethodNotAllowed,
            ),
            (
                "GET /nope HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: k\r\nSec-WebSocket-Version: 13\r\n\r\n",
                UpgradeReject::NotFound,
            ),
            (
                "GET / HTTP/1.0\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: k\r\nSec-WebSocket-Version: 13\r\n\r\n",
                UpgradeReject::BadRequest,
            ),
            (
                "GET / HTTP/1.1\r\nConnection: Upgrade\r\nSec-WebSocket-Key: k\r\nSec-WebSocket-Version: 13\r\n\r\n",
                UpgradeReject::BadRequest,
            ),
            (
                "GET / HTTP/1.1\r\nUpgrade: websocket\r\nSec-WebSocket-Key: k\r\nSec-WebSocket-Version: 13\r\n\r\n",
                UpgradeReject::BadRequest,
            ),
            (
                "GET / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\n\r\n",
                UpgradeReject::BadRequest,
            ),
        ];
        for (req, want) in cases {
            let (mut server, mut client) = UnixStream::pair().unwrap();
            let worker = thread::spawn(move || upgrade(&mut server));
            client.write_all(req.as_bytes()).unwrap();
            let got = worker.join().unwrap().expect_err("reject");
            assert_eq!(got, *want, "{req}");
        }
        let key = sample_key();
        let req = format!(
            "GET / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 8\r\n\r\n"
        );
        let (mut server, mut client) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || upgrade(&mut server));
        client.write_all(req.as_bytes()).unwrap();
        assert_eq!(worker.join().unwrap(), Err(UpgradeReject::UpgradeRequired));
    }

    #[test]
    fn headers_over_bound_close_without_status() {
        let (mut server, mut client) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || upgrade(&mut server));
        let mut huge = b"GET / HTTP/1.1\r\nX-Pad: ".to_vec();
        huge.extend(std::iter::repeat_n(b'a', MAX_HTTP_UPGRADE_BYTES));
        huge.extend_from_slice(b"\r\n\r\n");
        let _ = client.write_all(&huge);
        assert_eq!(worker.join().unwrap(), Err(UpgradeReject::HeadersTooLarge));
    }

    #[test]
    fn missing_listen_is_invalid() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--listen"]).is_err());
        assert!(parse_args(&["--home", "/tmp"]).is_err());
    }

    #[test]
    fn initialize_is_honest_and_omits_jsonrpc() {
        let req = json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {"name": "test", "version": "0"},
                "capabilities": {"optOutNotificationMethods": []}
            }
        });
        let reply = handle_rpc(&req).expect("reply");
        assert!(reply.get("jsonrpc").is_none());
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["result"]["userAgent"], USER_AGENT);
        assert!(reply["result"]
            .get("capabilities")
            .and_then(|c| c.get("ccas"))
            .is_none());
    }

    #[test]
    fn capabilities_ccas_is_method_not_found() {
        let req = json!({"id": 2, "method": "capabilities.ccas", "params": {}});
        let reply = handle_rpc(&req).expect("reply");
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
        let nested = json!({
            "id": 3,
            "method": "initialize",
            "params": {"capabilities": {"ccas": true}}
        });
        let reply = handle_rpc(&nested).expect("reply");
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn account_and_models_are_grok() {
        let account =
            handle_rpc(&json!({"id": 4, "method": "account/read", "params": {}})).unwrap();
        assert_eq!(account["result"]["requiresOpenaiAuth"], false);
        let models = handle_rpc(&json!({"id": 5, "method": "model/list", "params": {}})).unwrap();
        assert_eq!(models["result"]["data"][0]["model"], "grok");
        assert_eq!(models["result"]["data"][0]["isDefault"], true);
        assert!(models["result"]["nextCursor"].is_null());
    }

    #[test]
    fn initialized_notification_has_no_reply() {
        assert!(handle_rpc(&json!({"method": "initialized", "params": {}})).is_none());
    }
}
