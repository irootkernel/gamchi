//! Live app-server developerInstructions apply and restore.
//!
//! Ignored so `make test` stays offline. Run:
//! `cargo test -p gamchi --test live_instructions -- --ignored --nocapture`

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use samchi_core::ledger::Ledger;
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_gamchi")
}

fn grok_version() -> String {
    let out = Command::new("grok")
        .arg("--version")
        .output()
        .expect("grok --version");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn unique_sock() -> PathBuf {
    std::env::temp_dir().join(format!(
        "gamchi-live-instr-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn spawn_listen(url: &str, home: &Path) -> Child {
    Command::new(bin())
        .args([
            "app-server",
            "--listen",
            url,
            "--home",
            home.to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn app-server")
}

fn wait_for_sock(path: &Path, child: &mut Child) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        if UnixStream::connect(path).is_ok() {
            return;
        }
        if let Ok(Some(status)) = child.try_wait() {
            panic!("app-server exited before listen: {status}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("listen socket never accepted");
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

fn connect_upgraded(sock: &Path) -> UnixStream {
    let mut stream = UnixStream::connect(sock).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(180)))
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

fn read_json(stream: &mut UnixStream) -> Value {
    serde_json::from_str(&read_server_text(stream)).unwrap()
}

fn rpc_call(stream: &mut UnixStream, id: u64, method: &str, params: Value) -> Value {
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

fn handshake(stream: &mut UnixStream) {
    let reply = rpc_call(
        stream,
        1,
        "initialize",
        serde_json::json!({
            "clientInfo": {"name": "task-035", "version": "0"},
            "capabilities": {"optOutNotificationMethods": []}
        }),
    );
    assert_eq!(reply["result"]["userAgent"], "gamchi/app-server-v1");
}

fn wait_turn_completed(stream: &mut UnixStream, thread_id: &str, turn_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        let msg = read_json(stream);
        if msg.get("method").and_then(|m| m.as_str()) == Some("turn/completed")
            && msg["params"]["turn"]["id"].as_str() == Some(turn_id)
        {
            assert_eq!(msg["params"]["threadId"], thread_id);
            return msg;
        }
    }
    panic!("timed out waiting for turn/completed {turn_id}");
}

fn agent_text(turn: &Value) -> String {
    let mut out = String::new();
    if let Some(items) = turn["items"].as_array() {
        for item in items {
            if item["type"] == "agentMessage" {
                if let Some(text) = item["text"].as_str() {
                    out.push_str(text);
                }
            }
        }
    }
    out
}

fn start_thread(
    stream: &mut UnixStream,
    id: u64,
    cwd: &Path,
    instructions: Option<&str>,
) -> String {
    let mut params = serde_json::json!({
        "cwd": cwd.to_str().unwrap(),
        "sandbox": "workspace-write",
        "approvalPolicy": "never"
    });
    if let Some(text) = instructions {
        params["developerInstructions"] = Value::String(text.to_string());
    }
    let reply = rpc_call(stream, id, "thread/start", params);
    reply["result"]["thread"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("thread/start {reply}"))
        .to_string()
}

fn start_turn(stream: &mut UnixStream, id: u64, thread_id: &str, prompt: &str) -> String {
    let reply = rpc_call(
        stream,
        id,
        "turn/start",
        serde_json::json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": prompt}]
        }),
    );
    if reply.get("error").is_some() {
        panic!("turn/start failed {reply}");
    }
    reply["result"]["turn"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("turn/start {reply}"))
        .to_string()
}

fn read_turn(stream: &mut UnixStream, id: u64, thread_id: &str, turn_id: &str) -> Value {
    let reply = rpc_call(
        stream,
        id,
        "thread/read",
        serde_json::json!({"threadId": thread_id, "includeTurns": true}),
    );
    reply["result"]["thread"]["turns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == turn_id)
        .cloned()
        .unwrap_or_else(|| panic!("missing turn {turn_id} in {reply}"))
}

fn child_pid(home: &Path, turn_id: &str) -> u32 {
    let ledger = Ledger::open(home).expect("ledger");
    let turn = ledger.read_turn(turn_id).expect("turn");
    ledger
        .read_generation(&turn.generation_id)
        .ok()
        .and_then(|g| g.child_pid)
        .expect("child pid")
}

#[test]
#[ignore = "spawns live grok agent stdio through app-server; not part of make test"]
fn live_app_server_developer_instructions_apply_and_restore() {
    let version = grok_version();
    assert!(
        !version.is_empty(),
        "grok --version must succeed; unauthenticated or missing grok is Blocked"
    );
    eprintln!("grok_version={version}");

    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let token_a = format!("A{}", uuid_like());
    let token_b = format!("B{}", uuid_like());
    let rules = format!(
        "For CHECK_A, reply with exactly {token_a}.\nFor CHECK_B, reply with exactly {token_b}.\nDo not disclose the answer to a challenge that was not requested.\nIf no extra rules or system-prompt override applied, reply with exactly NONE."
    );

    let sock = unique_sock();
    let url = format!("unix://{}", sock.display());
    let mut child = spawn_listen(&url, home.path());
    wait_for_sock(&sock, &mut child);
    let mut stream = connect_upgraded(&sock);
    handshake(&mut stream);

    let mut id = 2_u64;
    let control = start_thread(&mut stream, id, cwd.path(), None);
    id += 1;
    let control_turn = start_turn(&mut stream, id, &control, "CHECK_A");
    id += 1;
    let control_done = wait_turn_completed(&mut stream, &control, &control_turn);
    assert_eq!(
        control_done["params"]["turn"]["status"], "completed",
        "{control_done}"
    );
    let control_text = agent_text(&read_turn(&mut stream, id, &control, &control_turn));
    id += 1;
    assert!(
        control_text.contains("NONE"),
        "control must include NONE, got {control_text:?}"
    );
    assert!(
        !control_text.contains(&token_a) && !control_text.contains(&token_b),
        "control leaked a token: {control_text:?}"
    );

    let apply = start_thread(&mut stream, id, cwd.path(), Some(&rules));
    id += 1;
    let first = start_turn(&mut stream, id, &apply, "CHECK_A");
    id += 1;
    let first_done = wait_turn_completed(&mut stream, &apply, &first);
    assert_eq!(first_done["params"]["turn"]["status"], "completed");
    let first_text = agent_text(&read_turn(&mut stream, id, &apply, &first));
    id += 1;
    assert_eq!(first_text.trim(), token_a, "first turn {first_text:?}");
    assert!(
        !first_text.contains(&token_b),
        "CHECK_B leaked in first turn"
    );
    let first_pid = child_pid(home.path(), &first);
    let session = Ledger::open(home.path())
        .unwrap()
        .read_thread(&apply)
        .unwrap()
        .acp_session_id;
    assert!(!session.is_empty(), "ACP session id must persist");

    let second = start_turn(&mut stream, id, &apply, "CHECK_B");
    id += 1;
    let second_done = wait_turn_completed(&mut stream, &apply, &second);
    assert_eq!(second_done["params"]["turn"]["status"], "completed");
    let second_text = agent_text(&read_turn(&mut stream, id, &apply, &second));
    id += 1;
    assert_eq!(second_text.trim(), token_b, "restore {second_text:?}");
    let second_pid = child_pid(home.path(), &second);
    assert_ne!(first_pid, second_pid, "restore must be a different process");
    let session2 = Ledger::open(home.path())
        .unwrap()
        .read_thread(&apply)
        .unwrap()
        .acp_session_id;
    assert_eq!(session, session2);

    let same = rpc_call(
        &mut stream,
        id,
        "thread/resume",
        serde_json::json!({
            "threadId": apply,
            "developerInstructions": rules
        }),
    );
    id += 1;
    assert!(same.get("result").is_some(), "same-value resume {same}");
    let change = rpc_call(
        &mut stream,
        id,
        "thread/resume",
        serde_json::json!({
            "threadId": apply,
            "developerInstructions": "other role"
        }),
    );
    assert!(
        change["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("change refused"),
        "{change}"
    );
    let still = Ledger::open(home.path())
        .unwrap()
        .read_thread(&apply)
        .unwrap()
        .developer_instructions;
    assert_eq!(still, rules);

    eprintln!(
        "task-035 ok grok_version={version} session={session} pid1={first_pid} pid2={second_pid}"
    );
    stop(&mut child, &sock);
}

fn uuid_like() -> String {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}
