//! Offline app-server listen, upgrade, and occupied-path tests. No live Grok.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static LOCK: Mutex<()> = Mutex::new(());
static SOCK_SEQ: AtomicU64 = AtomicU64::new(0);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_samchi-for-grok")
}

fn unique_sock() -> PathBuf {
    let n = SOCK_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("sc-as-{}-{n}.sock", std::process::id()))
}

fn spawn_listen(url: &str, home: Option<&Path>) -> Child {
    let mut cmd = Command::new(bin());
    cmd.args(["app-server", "--listen", url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(home) = home {
        cmd.args(["--home", home.to_str().unwrap()]);
    }
    cmd.spawn().expect("spawn app-server")
}

fn wait_for_sock(path: &Path, child: &mut Child) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        if path.exists() {
            return;
        }
        if let Ok(Some(status)) = child.try_wait() {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut err);
            }
            panic!("app-server exited before listen: {status}; stderr {err}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let mut err = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut err);
    }
    panic!("listen socket did not appear; stderr {err}");
}

fn stop(child: &mut Child, sock: &Path) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(sock);
}

fn read_headers(stream: &mut UnixStream) -> String {
    let mut buf = Vec::new();
    let mut byte = [0_u8; 1];
    while buf.len() < 16 * 1024 {
        stream.read_exact(&mut byte).expect("byte");
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            return String::from_utf8(buf).expect("utf8");
        }
    }
    panic!("no header terminator");
}

#[test]
fn listen_upgrades_dolgorae_handshake() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let home = tempfile::tempdir().expect("home");
    let mut child = spawn_listen(&url, Some(home.path()));
    wait_for_sock(&sock, &mut child);
    let mut stream = UnixStream::connect(&sock).expect("connect");
    let key = STANDARD.encode([9_u8; 16]);
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    )
    .unwrap();
    stream.flush().unwrap();
    let resp = read_headers(&mut stream);
    assert!(
        resp.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "{resp}"
    );
    assert!(resp.to_ascii_lowercase().contains("upgrade: websocket"));
    assert!(resp.contains("Sec-WebSocket-Accept:"), "{resp}");
    stop(&mut child, &sock);
}

#[test]
fn occupied_path_fails_closed() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let home = tempfile::tempdir().expect("home");
    let mut first = spawn_listen(&url, Some(home.path()));
    wait_for_sock(&sock, &mut first);
    let second = Command::new(bin())
        .args([
            "app-server",
            "--listen",
            &url,
            "--home",
            home.path().to_str().unwrap(),
        ])
        .output()
        .expect("second");
    assert!(!second.status.success());
    let err = String::from_utf8_lossy(&second.stderr);
    assert!(
        err.contains("SAMCHI_FOR_GROK_STARTUP_ERROR OCCUPIED_PATH"),
        "{err}"
    );
    assert!(sock.exists(), "occupant must remain");
    stop(&mut first, &sock);
}

#[test]
fn method_not_allowed_is_405() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let home = tempfile::tempdir().expect("home");
    let mut child = spawn_listen(&url, Some(home.path()));
    wait_for_sock(&sock, &mut child);
    let mut stream = UnixStream::connect(&sock).expect("connect");
    write!(
        stream,
        "POST / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n\r\n",
        STANDARD.encode([1_u8; 16])
    )
    .unwrap();
    let resp = read_headers(&mut stream);
    assert!(
        resp.starts_with("HTTP/1.1 405 Method Not Allowed"),
        "{resp}"
    );
    stop(&mut child, &sock);
}
