//! Offline app-server listen, upgrade, handshake, and same-ledger thread/turn tests.
//! No live Grok.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use samchi_core::ledger::Ledger;
use samchi_core::source_wire::{ApprovalPolicy, ThreadSandbox};
use std::io::{BufRead, BufReader, Read, Write};
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

fn fake_agent() -> PathBuf {
    let mut p = PathBuf::from(bin());
    p.set_file_name("fake-acp-agent");
    p
}

fn unique_sock() -> PathBuf {
    let n = SOCK_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("sc-as-{}-{n}.sock", std::process::id()))
}

fn spawn_listen(url: &str, home: Option<&Path>) -> Child {
    spawn_listen_with(url, home, &[])
}

fn spawn_listen_with(url: &str, home: Option<&Path>, extra: &[(&str, &str)]) -> Child {
    let mut cmd = Command::new(bin());
    cmd.args(["app-server", "--listen", url])
        .env("SAMCHI_FOR_GROK_ACP_PROGRAM", fake_agent())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(home) = home {
        cmd.args(["--home", home.to_str().unwrap()]);
    }
    for (k, v) in extra {
        cmd.env(k, v);
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

fn client_text_frame(payload: &[u8]) -> Vec<u8> {
    let mask = [1_u8, 2, 3, 4];
    let mut out = vec![0x81];
    if payload.len() <= 125 {
        out.push(0x80 | payload.len() as u8);
    } else {
        out.push(0x80 | 126);
        out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    }
    out.extend_from_slice(&mask);
    for (i, b) in payload.iter().enumerate() {
        out.push(b ^ mask[i % 4]);
    }
    out
}

fn read_server_text(stream: &mut UnixStream) -> String {
    let mut prefix = [0_u8; 2];
    stream.read_exact(&mut prefix).expect("prefix");
    assert_eq!(prefix[0] & 0x0f, 0x1);
    assert_eq!(prefix[1] & 0x80, 0, "server frames are unmasked");
    let marker = prefix[1] & 0x7f;
    let len = if marker <= 125 {
        marker as usize
    } else {
        assert_eq!(marker, 126);
        let mut ext = [0_u8; 2];
        stream.read_exact(&mut ext).expect("ext");
        u16::from_be_bytes(ext) as usize
    };
    let mut payload = vec![0_u8; len];
    stream.read_exact(&mut payload).expect("payload");
    String::from_utf8(payload).expect("utf8")
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
    let init = serde_json::json!({
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {"name": "shaped", "version": "0"},
            "capabilities": {"optOutNotificationMethods": []}
        }
    });
    stream
        .write_all(&client_text_frame(init.to_string().as_bytes()))
        .unwrap();
    let reply: serde_json::Value = serde_json::from_str(&read_server_text(&mut stream)).unwrap();
    assert!(reply.get("jsonrpc").is_none(), "{reply}");
    assert_eq!(
        reply["result"]["userAgent"],
        "samchi-for-grok/app-server-v1"
    );
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

fn connect_upgraded(sock: &Path) -> UnixStream {
    let mut stream = UnixStream::connect(sock).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(20)))
        .expect("write timeout");
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
    stream
}

fn read_json(stream: &mut UnixStream) -> serde_json::Value {
    serde_json::from_str(&read_server_text(stream)).unwrap()
}

fn rpc_call(
    stream: &mut UnixStream,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let req = serde_json::json!({"id": id, "method": method, "params": params});
    stream
        .write_all(&client_text_frame(req.to_string().as_bytes()))
        .unwrap();
    loop {
        let reply = read_json(stream);
        if reply.get("id").is_some() && reply.get("method").is_none() {
            return reply;
        }
    }
}

fn mcp_spawn(home: &Path, cwd: &Path, prompt: &str) -> serde_json::Value {
    let mut child = Command::new(bin())
        .args(["mcp", "--home", home.to_str().unwrap()])
        .env("SAMCHI_FOR_GROK_ACP_PROGRAM", fake_agent())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("mcp");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}
        }
    });
    writeln!(stdin, "{init}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "grok_spawn",
            "arguments": {
                "prompt": prompt,
                "cwd": cwd.to_str().unwrap()
            }
        }
    });
    writeln!(stdin, "{call}").unwrap();
    stdin.flush().unwrap();
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("mcp spawn {resp}"));
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
    let _ = child.kill();
    let _ = child.wait();
    parsed
}

#[test]
fn socket_thread_turn_shares_mcp_ledger() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let mcp = mcp_spawn(home.path(), cwd.path(), "mcp-ping");
    let mcp_thread = mcp["thread_id"].as_str().expect("mcp thread").to_string();
    let mcp_turn = mcp["turn_id"].as_str().expect("mcp turn").to_string();

    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen(&url, Some(home.path()));
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let init = rpc_call(
        &mut stream,
        1,
        "initialize",
        serde_json::json!({
            "clientInfo": {"name": "shaped", "version": "0"},
            "capabilities": {"optOutNotificationMethods": []}
        }),
    );
    assert_eq!(init["result"]["userAgent"], "samchi-for-grok/app-server-v1");
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({"cwd": cwd.path().to_str().unwrap()}),
    );
    let socket_thread = started["result"]["thread"]["id"]
        .as_str()
        .expect("socket thread")
        .to_string();
    let turn = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": socket_thread,
            "input": [{"type": "text", "text": "socket-ping"}]
        }),
    );
    let socket_turn = turn["result"]["turn"]["id"]
        .as_str()
        .expect("socket turn")
        .to_string();
    assert_eq!(turn["result"]["turn"]["status"], "inProgress");
    let fork = rpc_call(
        &mut stream,
        4,
        "thread/fork",
        serde_json::json!({"threadId": socket_thread}),
    );
    assert_eq!(fork["error"]["code"], -32602);
    assert_eq!(fork["error"]["message"], "thread/fork");

    let ledger = Ledger::open(home.path()).expect("ledger");
    let mcp_stored = ledger.read_thread(&mcp_thread).unwrap();
    let socket_stored = ledger.read_thread(&socket_thread).unwrap();
    assert_eq!(mcp_stored.sandbox, ThreadSandbox::WorkspaceWrite);
    assert_eq!(mcp_stored.approval_policy, ApprovalPolicy::Never);
    assert_eq!(socket_stored.sandbox, ThreadSandbox::ReadOnly);
    assert_eq!(socket_stored.approval_policy, ApprovalPolicy::Untrusted);
    assert_eq!(ledger.read_turn(&mcp_turn).unwrap().thread_id, mcp_thread);
    assert_eq!(
        ledger.read_turn(&socket_turn).unwrap().thread_id,
        socket_thread
    );
    stop(&mut child, &sock);
}

#[test]
fn one_in_progress_turn_per_thread_and_interrupt() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen_with(
        &url,
        Some(home.path()),
        &[("SAMCHI_FOR_GROK_FAKE_HANG_SECS", "60")],
    );
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let _ = rpc_call(
        &mut stream,
        1,
        "initialize",
        serde_json::json!({
            "clientInfo": {"name": "shaped", "version": "0"},
            "capabilities": {"optOutNotificationMethods": []}
        }),
    );
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({"cwd": cwd.path().to_str().unwrap()}),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let first = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "hang"}]
        }),
    );
    let turn_id = first["result"]["turn"]["id"].as_str().unwrap().to_string();
    assert_eq!(first["result"]["turn"]["status"], "inProgress");
    let second = rpc_call(
        &mut stream,
        4,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "again"}]
        }),
    );
    assert_eq!(second["error"]["code"], -32602);
    assert!(
        second["error"]["message"]
            .as_str()
            .unwrap()
            .contains("inProgress"),
        "{second}"
    );
    let interrupted = rpc_call(
        &mut stream,
        5,
        "turn/interrupt",
        serde_json::json!({"threadId": thread_id, "turnId": turn_id}),
    );
    assert_eq!(interrupted["result"]["turn"]["id"], turn_id);
    assert_eq!(interrupted["result"]["turn"]["status"], "interrupted");
    let read = rpc_call(
        &mut stream,
        6,
        "thread/read",
        serde_json::json!({"threadId": thread_id, "includeTurns": true}),
    );
    assert_eq!(read["result"]["thread"]["turns"][0]["id"], turn_id);
    assert_eq!(
        read["result"]["thread"]["turns"][0]["status"],
        "interrupted"
    );
    stop(&mut child, &sock);
}

fn handshake(stream: &mut UnixStream) {
    let _ = rpc_call(
        stream,
        1,
        "initialize",
        serde_json::json!({
            "clientInfo": {"name": "shaped", "version": "0"},
            "capabilities": {"optOutNotificationMethods": []}
        }),
    );
}

#[test]
fn socket_emits_item_and_turn_notifications() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen(&url, Some(home.path()));
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    handshake(&mut stream);
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({"cwd": cwd.path().to_str().unwrap()}),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let thread_started = read_json(&mut stream);
    assert_eq!(thread_started["method"], "thread/started");
    assert_eq!(thread_started["params"]["thread"]["id"], thread_id);
    let _ = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "ping"}]
        }),
    );
    let mut methods = Vec::new();
    let mut completed = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let msg = read_json(&mut stream);
        if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
            methods.push(method.to_string());
            if method == "turn/completed" {
                assert_eq!(msg["params"]["threadId"], thread_id);
                assert_eq!(msg["params"]["turn"]["status"], "completed");
                assert!(msg.get("jsonrpc").is_none());
                completed = true;
                break;
            }
        }
    }
    assert!(completed, "expected turn/completed");
    assert!(methods.iter().any(|m| m == "item/started"), "{methods:?}");
    assert!(methods.iter().any(|m| m == "item/completed"), "{methods:?}");
    assert!(methods.iter().any(|m| m == "turn/completed"), "{methods:?}");
    assert!(
        methods.iter().all(|m| m != "item/fileChange/patchUpdated"),
        "{methods:?}"
    );
    stop(&mut child, &sock);
}

#[test]
fn socket_approval_is_server_request_not_grok_respond() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen_with(
        &url,
        Some(home.path()),
        &[("SAMCHI_FOR_GROK_FAKE_ASK_PERMISSION", "1")],
    );
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    handshake(&mut stream);
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({
            "cwd": cwd.path().to_str().unwrap(),
            "approvalPolicy": "untrusted",
            "sandbox": "workspace-write"
        }),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "edit"}]
        }),
    );
    let mut saw_approval = false;
    let mut completed = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let msg = read_json(&mut stream);
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        if method == "item/commandExecution/requestApproval"
            || method == "item/fileChange/requestApproval"
        {
            assert!(msg.get("id").is_some(), "{msg}");
            assert!(msg.get("jsonrpc").is_none(), "{msg}");
            assert_eq!(msg["params"]["threadId"], thread_id);
            let item_id = msg["params"]["itemId"].as_str().expect("itemId");
            assert!(!item_id.is_empty(), "{msg}");
            assert!(msg["params"]["startedAtMs"].as_u64().is_some());
            let reply = serde_json::json!({
                "id": msg["id"],
                "result": {"decision": "accept"}
            });
            stream
                .write_all(&client_text_frame(reply.to_string().as_bytes()))
                .unwrap();
            saw_approval = true;
        }
        if method == "turn/completed" {
            assert_eq!(msg["params"]["turn"]["status"], "completed");
            completed = true;
            break;
        }
    }
    assert!(saw_approval, "expected socket requestApproval");
    assert!(completed, "expected turn/completed after accept");
    stop(&mut child, &sock);
}

#[test]
fn interrupt_during_approval_emits_turn_completed() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen_with(
        &url,
        Some(home.path()),
        &[("SAMCHI_FOR_GROK_FAKE_ASK_PERMISSION", "1")],
    );
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    handshake(&mut stream);
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({
            "cwd": cwd.path().to_str().unwrap(),
            "approvalPolicy": "untrusted",
            "sandbox": "workspace-write"
        }),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let turn = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "edit"}]
        }),
    );
    let turn_id = turn["result"]["turn"]["id"].as_str().unwrap().to_string();
    let mut interrupted = false;
    let mut sent_interrupt = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let msg = read_json(&mut stream);
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        if !sent_interrupt
            && (method == "item/commandExecution/requestApproval"
                || method == "item/fileChange/requestApproval")
        {
            let bad = serde_json::json!({"id": msg["id"], "result": {}});
            stream
                .write_all(&client_text_frame(bad.to_string().as_bytes()))
                .unwrap();
            let req = serde_json::json!({
                "id": 4,
                "method": "turn/interrupt",
                "params": {"threadId": thread_id, "turnId": turn_id}
            });
            stream
                .write_all(&client_text_frame(req.to_string().as_bytes()))
                .unwrap();
            sent_interrupt = true;
        }
        if method == "turn/completed" {
            assert_eq!(msg["params"]["turn"]["status"], "interrupted");
            interrupted = true;
            break;
        }
    }
    assert!(interrupted, "expected turn/completed after interrupt");
    stop(&mut child, &sock);
}

fn notify(stream: &mut UnixStream, method: &str, params: serde_json::Value) {
    let req = serde_json::json!({"method": method, "params": params});
    stream
        .write_all(&client_text_frame(req.to_string().as_bytes()))
        .unwrap();
}

fn wait_method(stream: &mut UnixStream, want: &str) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let msg = read_json(stream);
        if msg.get("method").and_then(|m| m.as_str()) == Some(want) {
            return msg;
        }
    }
    panic!("timed out waiting for {want}");
}

fn shaped_handshake(stream: &mut UnixStream) -> serde_json::Value {
    rpc_call(
        stream,
        1,
        "initialize",
        serde_json::json!({
            "clientInfo": {"name": "dolgorae-shaped", "version": "0"},
            "capabilities": {"experimentalApi": false, "optOutNotificationMethods": []}
        }),
    )
}

/// TASK-019 scenario `probe`.
#[test]
fn shaped_client_probe() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen(&url, Some(home.path()));
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let init = shaped_handshake(&mut stream);
    assert_eq!(init["result"]["userAgent"], "samchi-for-grok/app-server-v1");
    assert!(init.get("jsonrpc").is_none());
    notify(&mut stream, "initialized", serde_json::json!({}));
    let account = rpc_call(
        &mut stream,
        2,
        "account/read",
        serde_json::json!({"refreshToken": false}),
    );
    assert_eq!(account["result"]["requiresOpenaiAuth"], false);
    let models = rpc_call(
        &mut stream,
        3,
        "model/list",
        serde_json::json!({"cursor": serde_json::Value::Null, "limit": 100}),
    );
    assert_eq!(models["result"]["data"][0]["model"], "grok");
    assert_eq!(models["result"]["data"][0]["isDefault"], true);
    assert!(models["result"]["data"][0]["supportedReasoningEfforts"].is_array());
    let runtime = rpc_call(&mut stream, 4, "grok/runtime/read", serde_json::json!({}));
    assert_eq!(runtime["result"]["runtime"], "grok");
    assert_eq!(
        runtime["result"]["userAgent"],
        "samchi-for-grok/app-server-v1"
    );
    assert_eq!(
        runtime["result"]["home"].as_str().unwrap(),
        home.path().display().to_string()
    );
    let absent = rpc_call(
        &mut stream,
        5,
        "thread/read",
        serde_json::json!({
            "threadId": "01900000-0000-7000-8000-000000000000",
            "includeTurns": true
        }),
    );
    assert_eq!(absent["error"]["code"], -32600);
    stop(&mut child, &sock);
}

#[test]
fn grok_runtime_read_home_follows_env() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen_with(
        &url,
        None,
        &[("SAMCHI_FOR_GROK_HOME", home.path().to_str().unwrap())],
    );
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let _ = shaped_handshake(&mut stream);
    let runtime = rpc_call(&mut stream, 2, "grok/runtime/read", serde_json::json!({}));
    assert_eq!(
        runtime["result"]["home"].as_str().unwrap(),
        home.path().display().to_string()
    );
    stop(&mut child, &sock);
}

/// TASK-019 scenario `first-turn`.
#[test]
fn shaped_client_first_turn() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen(&url, Some(home.path()));
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let _ = shaped_handshake(&mut stream);
    notify(&mut stream, "initialized", serde_json::json!({}));
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({"cwd": cwd.path().to_str().unwrap()}),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let thread_started = wait_method(&mut stream, "thread/started");
    assert_eq!(thread_started["params"]["thread"]["id"], thread_id);
    let turn = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "first-turn"}]
        }),
    );
    assert_eq!(turn["result"]["turn"]["status"], "inProgress");
    let completed = wait_method(&mut stream, "turn/completed");
    assert_eq!(completed["params"]["threadId"], thread_id);
    assert_eq!(completed["params"]["turn"]["status"], "completed");
    let ledger = Ledger::open(home.path()).expect("ledger");
    let stored = ledger.read_thread(&thread_id).unwrap();
    assert_eq!(stored.sandbox, ThreadSandbox::ReadOnly);
    assert_eq!(stored.approval_policy, ApprovalPolicy::Untrusted);
    stop(&mut child, &sock);
}

/// TASK-019 scenario `follow-up`.
#[test]
fn shaped_client_follow_up() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen(&url, Some(home.path()));
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let _ = shaped_handshake(&mut stream);
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({"cwd": cwd.path().to_str().unwrap()}),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = wait_method(&mut stream, "thread/started");
    let first = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "first"}]
        }),
    );
    let first_id = first["result"]["turn"]["id"].as_str().unwrap().to_string();
    let first_done = wait_method(&mut stream, "turn/completed");
    assert_eq!(first_done["params"]["turn"]["id"], first_id);
    assert_eq!(first_done["params"]["turn"]["status"], "completed");
    let resumed = rpc_call(
        &mut stream,
        4,
        "thread/resume",
        serde_json::json!({"threadId": thread_id}),
    );
    assert_eq!(resumed["result"]["thread"]["id"], thread_id);
    let second = rpc_call(
        &mut stream,
        5,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "follow-up"}]
        }),
    );
    let second_id = second["result"]["turn"]["id"].as_str().unwrap().to_string();
    assert_ne!(second_id, first_id);
    assert_eq!(second["result"]["turn"]["status"], "inProgress");
    let second_done = wait_method(&mut stream, "turn/completed");
    assert_eq!(second_done["params"]["turn"]["id"], second_id);
    assert_eq!(second_done["params"]["turn"]["status"], "completed");
    let read = rpc_call(
        &mut stream,
        6,
        "thread/read",
        serde_json::json!({"threadId": thread_id, "includeTurns": true}),
    );
    let turns = read["result"]["thread"]["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 2, "{read}");
    stop(&mut child, &sock);
}

/// TASK-019 scenario `approval`.
#[test]
fn shaped_client_approval() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen_with(
        &url,
        Some(home.path()),
        &[("SAMCHI_FOR_GROK_FAKE_ASK_PERMISSION", "1")],
    );
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let _ = shaped_handshake(&mut stream);
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({
            "cwd": cwd.path().to_str().unwrap(),
            "approvalPolicy": "untrusted",
            "sandbox": "workspace-write"
        }),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "edit"}]
        }),
    );
    let mut saw_approval = false;
    let mut completed = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let msg = read_json(&mut stream);
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        if method == "item/commandExecution/requestApproval"
            || method == "item/fileChange/requestApproval"
        {
            assert!(msg.get("id").is_some(), "{msg}");
            assert!(msg.get("jsonrpc").is_none(), "{msg}");
            let reply = serde_json::json!({
                "id": msg["id"],
                "result": {"decision": "accept"}
            });
            stream
                .write_all(&client_text_frame(reply.to_string().as_bytes()))
                .unwrap();
            saw_approval = true;
        }
        if method == "turn/completed" {
            assert_eq!(msg["params"]["turn"]["status"], "completed");
            completed = true;
            break;
        }
    }
    assert!(saw_approval, "expected socket requestApproval");
    assert!(completed, "expected turn/completed after accept");
    stop(&mut child, &sock);
}

/// TASK-019 scenario `interrupt`.
#[test]
fn shaped_client_interrupt() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen_with(
        &url,
        Some(home.path()),
        &[("SAMCHI_FOR_GROK_FAKE_HANG_SECS", "60")],
    );
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let _ = shaped_handshake(&mut stream);
    let started = rpc_call(
        &mut stream,
        2,
        "thread/start",
        serde_json::json!({"cwd": cwd.path().to_str().unwrap()}),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let first = rpc_call(
        &mut stream,
        3,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "hang"}]
        }),
    );
    let turn_id = first["result"]["turn"]["id"].as_str().unwrap().to_string();
    let req = serde_json::json!({
        "id": 4,
        "method": "turn/interrupt",
        "params": {"threadId": thread_id, "turnId": turn_id}
    });
    stream
        .write_all(&client_text_frame(req.to_string().as_bytes()))
        .unwrap();
    let mut saw_reply = false;
    let mut saw_completed = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let msg = read_json(&mut stream);
        if msg.get("id") == Some(&serde_json::json!(4)) {
            assert_eq!(msg["result"]["turn"]["status"], "interrupted");
            saw_reply = true;
        }
        if msg.get("method").and_then(|m| m.as_str()) == Some("turn/completed") {
            assert_eq!(msg["params"]["turn"]["status"], "interrupted");
            saw_completed = true;
        }
        if saw_reply && saw_completed {
            break;
        }
    }
    assert!(saw_reply, "expected turn/interrupt reply");
    assert!(saw_completed, "expected turn/completed interrupted");
    let fork = rpc_call(
        &mut stream,
        5,
        "thread/fork",
        serde_json::json!({"threadId": thread_id}),
    );
    assert_eq!(fork["error"]["code"], -32602);
    assert_eq!(fork["error"]["message"], "thread/fork");
    stop(&mut child, &sock);
}

#[test]
fn model_list_fixture_and_turn_model_lock() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let fixture = home.path().join("models.json");
    std::fs::write(
        &fixture,
        r#"{"data":[{"model":"grok-4.6","isDefault":true,"supportedReasoningEfforts":["high"]}],"nextCursor":null}"#,
    )
    .unwrap();
    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen_with(
        &url,
        Some(home.path()),
        &[(
            "SAMCHI_FOR_GROK_MODEL_LIST_FIXTURE",
            fixture.to_str().unwrap(),
        )],
    );
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    let _ = shaped_handshake(&mut stream);
    notify(&mut stream, "initialized", serde_json::json!({}));
    let models = rpc_call(
        &mut stream,
        2,
        "model/list",
        serde_json::json!({"cursor": serde_json::Value::Null, "limit": 100}),
    );
    assert_eq!(models["result"]["data"][0]["model"], "grok-4.6");
    let started = rpc_call(
        &mut stream,
        3,
        "thread/start",
        serde_json::json!({"cwd": cwd.path().to_str().unwrap()}),
    );
    let thread_id = started["result"]["thread"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = wait_method(&mut stream, "thread/started");
    let first_mismatch = rpc_call(
        &mut stream,
        4,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "locked"}],
            "model": "other"
        }),
    );
    assert!(
        first_mismatch["error"]["message"]
            .as_str()
            .unwrap()
            .contains("INVALID_CONFIG"),
        "{first_mismatch}"
    );
    let first = rpc_call(
        &mut stream,
        5,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "one"}],
            "effort": "low"
        }),
    );
    assert_eq!(first["result"]["turn"]["status"], "inProgress");
    let _ = wait_method(&mut stream, "turn/completed");
    let mismatch = rpc_call(
        &mut stream,
        6,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "two"}],
            "model": "other"
        }),
    );
    assert!(
        mismatch["error"]["message"]
            .as_str()
            .unwrap()
            .contains("INVALID_CONFIG"),
        "{mismatch}"
    );
    let effort = rpc_call(
        &mut stream,
        7,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": "three"}],
            "effort": "high"
        }),
    );
    assert_eq!(effort["result"]["turn"]["status"], "inProgress");
    let _ = wait_method(&mut stream, "turn/completed");
    let ledger = Ledger::open(home.path()).expect("ledger");
    let stored = ledger.read_thread(&thread_id).unwrap();
    assert_eq!(stored.model, "grok-4.6");
    stop(&mut child, &sock);
}
