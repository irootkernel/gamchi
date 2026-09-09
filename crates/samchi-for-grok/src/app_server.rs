//! Unix-domain app-server listen and HTTP/1.1 WebSocket upgrade (TASK-015).
//! JSON-RPC handshake stays TASK-016.

use crate::ops::open_home;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use samchi_core::source_wire::MAX_HTTP_UPGRADE_BYTES;
use sha1::{Digest, Sha1};
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::thread;

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
    if let Err(reason) = open_home(cfg.home.as_deref()) {
        let _ = writeln!(stderr, "SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG");
        let _ = writeln!(stderr, "{reason}");
        return 1;
    }
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
    accept_loop(listener);
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

fn accept_loop(listener: UnixListener) {
    for incoming in listener.incoming() {
        let Ok(stream) = incoming else {
            continue;
        };
        thread::spawn(move || {
            let _ = serve_connection(stream);
        });
    }
}

fn serve_connection(mut stream: UnixStream) -> io::Result<()> {
    match upgrade(&mut stream) {
        Ok(()) => park(stream),
        Err(UpgradeReject::HeadersTooLarge) => Ok(()),
        Err(reject) => write_reject(&mut stream, reject),
    }
}

fn park(mut stream: UnixStream) -> io::Result<()> {
    let mut buf = [0_u8; 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return Ok(()),
            Ok(_) => {}
        }
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
}
