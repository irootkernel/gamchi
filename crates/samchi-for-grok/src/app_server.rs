//! Unix-domain app-server listen, HTTP/1.1 WebSocket upgrade, handshake,
//! thread/turn methods, and TASK-018 notifications plus approval requests.

use crate::ops::{acp_command, cancel_turn, open_home, open_ledger};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use samchi_adapter_grok::{
    run_turn_on_admit, ExtraSpawnFields, TurnRequest, UNENFORCEABLE_EXTRA_FIELD_NAMES,
};
use samchi_core::ledger::{Ledger, NewThread};
use samchi_core::source_wire::{
    parse_approval_decision, parse_approval_policy, parse_thread_sandbox, parse_turn_sandbox_type,
    Item, Turn, APP_SERVER_DEFAULT_APPROVAL_POLICY, APP_SERVER_DEFAULT_THREAD_SANDBOX,
    ITEM_COMMAND_EXECUTION, ITEM_FILE_CHANGE, MAX_HTTP_UPGRADE_BYTES, MAX_WEBSOCKET_FRAME_BYTES,
    NOTIFY_ITEM_COMPLETED, NOTIFY_ITEM_STARTED, NOTIFY_THREAD_STARTED, NOTIFY_TURN_COMPLETED,
    REQUEST_COMMAND_APPROVAL, REQUEST_FILE_CHANGE_APPROVAL,
};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

#[derive(Clone)]
struct ConnIo {
    writer: Arc<Mutex<UnixStream>>,
    pending: Arc<Mutex<HashMap<String, mpsc::SyncSender<Value>>>>,
    next_id: Arc<AtomicI64>,
}

fn rpc_loop(stream: &mut UnixStream, home: &Path) -> io::Result<()> {
    let io = ConnIo {
        writer: Arc::new(Mutex::new(stream.try_clone()?)),
        pending: Arc::new(Mutex::new(HashMap::new())),
        next_id: Arc::new(AtomicI64::new(1000)),
    };
    loop {
        match read_ws_text(stream) {
            Ok(None) => return Ok(()),
            Ok(Some(bytes)) => {
                let Ok(msg) = serde_json::from_slice::<Value>(&bytes) else {
                    continue;
                };
                if msg.get("method").and_then(Value::as_str).is_none() {
                    deliver_pending(&io, &msg);
                    continue;
                }
                if let Some(reply) = handle_rpc(&msg, home) {
                    write_locked(&io.writer, &serde_json::to_vec(&reply)?)?;
                    after_rpc(&msg, &reply, home, &io);
                }
            }
            Err(_) => return Ok(()),
        }
    }
}

fn write_locked(writer: &Mutex<UnixStream>, payload: &[u8]) -> io::Result<()> {
    let mut stream = writer.lock().unwrap_or_else(|e| e.into_inner());
    write_ws_text(&mut stream, payload)
}

fn deliver_pending(io: &ConnIo, msg: &Value) {
    let Some(id) = msg.get("id") else {
        return;
    };
    let key = id.to_string();
    if let Some(tx) = io
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&key)
    {
        let _ = tx.send(msg.clone());
    }
}

fn after_rpc(req: &Value, reply: &Value, home: &Path, io: &ConnIo) {
    if reply.get("error").is_some() {
        return;
    }
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "thread/start" => {
            if let Some(id) = reply.pointer("/result/thread/id").and_then(Value::as_str) {
                let note = json!({
                    "method": NOTIFY_THREAD_STARTED,
                    "params": {"thread": {"id": id}}
                });
                let _ = write_locked(&io.writer, &serde_json::to_vec(&note).unwrap_or_default());
            }
        }
        "turn/start" => {
            let Some(turn_id) = reply
                .pointer("/result/turn/id")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return;
            };
            let Some(thread_id) = req
                .pointer("/params/threadId")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return;
            };
            let home = home.to_path_buf();
            let io = io.clone();
            thread::spawn(move || {
                let _ = watch_turn(&home, &thread_id, &turn_id, &io);
            });
        }
        _ => {}
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct PendingApproval {
    request_id: String,
    rpc_id: String,
    rx: mpsc::Receiver<Value>,
}

fn watch_turn(home: &Path, thread_id: &str, turn_id: &str, io: &ConnIo) -> Result<(), String> {
    let ledger = open_ledger(Some(home))?;
    let mut started = HashSet::new();
    let mut completed = HashSet::new();
    let mut last_request = String::new();
    let mut pending: Option<PendingApproval> = None;
    loop {
        let turn = match ledger.observe(turn_id) {
            Ok(turn) => turn,
            Err(_) => {
                thread::sleep(Duration::from_millis(20));
                continue;
            }
        };
        let _ = emit_item_notifications(&turn, thread_id, io, &mut started, &mut completed);
        if turn.status.is_terminal() {
            let _ = emit_item_notifications(&turn, thread_id, io, &mut started, &mut completed);
            let note = json!({
                "method": NOTIFY_TURN_COMPLETED,
                "params": {
                    "threadId": thread_id,
                    "turn": {
                        "id": turn.id,
                        "items": items_wire(&turn),
                        "status": turn.status.as_str(),
                    }
                }
            });
            write_locked(&io.writer, &serde_json::to_vec(&note).unwrap_or_default())
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        if let Some(wait) = pending.take() {
            match wait.rx.try_recv() {
                Ok(reply) => {
                    if let Some(decision) = reply
                        .get("result")
                        .and_then(|result| result.get("decision"))
                        .and_then(Value::as_str)
                    {
                        if let Ok(decision) = parse_approval_decision(decision) {
                            let _ = ledger.respond(&wait.request_id, decision);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => pending = Some(wait),
                Err(mpsc::TryRecvError::Disconnected) => {
                    io.pending
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&wait.rpc_id);
                }
            }
        }
        if turn.pending_approval() && pending.is_none() && turn.pending_request_id != last_request {
            if let Some(wait) = start_socket_approval(&turn, thread_id, io) {
                last_request = wait.request_id.clone();
                pending = Some(wait);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn emit_item_notifications(
    turn: &Turn,
    thread_id: &str,
    io: &ConnIo,
    started: &mut HashSet<String>,
    completed: &mut HashSet<String>,
) -> Result<(), String> {
    let now = now_ms();
    for item in &turn.items {
        if started.insert(item.id.clone()) {
            let note = json!({
                "method": NOTIFY_ITEM_STARTED,
                "params": {
                    "threadId": thread_id,
                    "turnId": turn.id,
                    "startedAtMs": now,
                    "item": item_obj(item),
                }
            });
            write_locked(&io.writer, &serde_json::to_vec(&note).unwrap_or_default())
                .map_err(|e| e.to_string())?;
        }
        let terminal = item.status == "completed" || item.status == "failed";
        if terminal && completed.insert(item.id.clone()) {
            let note = json!({
                "method": NOTIFY_ITEM_COMPLETED,
                "params": {
                    "threadId": thread_id,
                    "turnId": turn.id,
                    "completedAtMs": now,
                    "item": item_obj(item),
                }
            });
            write_locked(&io.writer, &serde_json::to_vec(&note).unwrap_or_default())
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn start_socket_approval(turn: &Turn, thread_id: &str, io: &ConnIo) -> Option<PendingApproval> {
    let item = turn.items.iter().rev().find(|item| {
        item.item_type == ITEM_COMMAND_EXECUTION || item.item_type == ITEM_FILE_CHANGE
    });
    let method = match item.map(|item| item.item_type.as_str()) {
        Some(ITEM_FILE_CHANGE) => REQUEST_FILE_CHANGE_APPROVAL,
        _ => REQUEST_COMMAND_APPROVAL,
    };
    let item_id = item
        .map(|item| item.id.clone())
        .or_else(|| turn.items.last().map(|item| item.id.clone()))
        .unwrap_or_else(|| turn.id.clone());
    let id = io.next_id.fetch_add(1, Ordering::Relaxed);
    let rpc_id = id.to_string();
    let (tx, rx) = mpsc::sync_channel(1);
    io.pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(rpc_id.clone(), tx);
    let req = json!({
        "id": id,
        "method": method,
        "params": {
            "itemId": item_id,
            "startedAtMs": now_ms(),
            "threadId": thread_id,
            "turnId": turn.id,
        }
    });
    if write_locked(&io.writer, &serde_json::to_vec(&req).unwrap_or_default()).is_err() {
        io.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&rpc_id);
        return None;
    }
    Some(PendingApproval {
        request_id: turn.pending_request_id.clone(),
        rpc_id,
        rx,
    })
}

fn handle_rpc(msg: &Value, home: &Path) -> Option<Value> {
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
        "initialize" => {
            if opt_out_is_nonempty(&params) {
                return Some(rpc_error(
                    id,
                    METHOD_NOT_FOUND,
                    "optOutNotificationMethods must be empty",
                ));
            }
            Some(json!({"id": id, "result": initialize_result()}))
        }
        "account/read" => Some(json!({"id": id, "result": json!({"requiresOpenaiAuth": false})})),
        "model/list" => Some(json!({"id": id, "result": model_list_result()})),
        "thread/fork" => Some(rpc_error(id, METHOD_NOT_FOUND, "thread/fork")),
        "thread/start" => Some(rpc_result(id, thread_start(&params, home))),
        "thread/resume" => Some(rpc_result(id, thread_resume(&params, home))),
        "thread/read" => Some(rpc_result(id, thread_read(&params, home))),
        "turn/start" => Some(rpc_result(id, turn_start(&params, home))),
        "turn/interrupt" => Some(rpc_result(id, turn_interrupt(&params, home))),
        _ => Some(rpc_error(id, METHOD_NOT_FOUND, method)),
    }
}

fn rpc_result(id: Value, result: Result<Value, String>) -> Value {
    match result {
        Ok(result) => json!({"id": id, "result": result}),
        Err(message) => rpc_error(id, METHOD_NOT_FOUND, &message),
    }
}

fn opt_out_is_nonempty(params: &Value) -> bool {
    let top = params.get("optOutNotificationMethods");
    let nested = params
        .get("capabilities")
        .and_then(|c| c.get("optOutNotificationMethods"));
    [top, nested]
        .into_iter()
        .flatten()
        .any(|v| v.as_array().map(|a| !a.is_empty()).unwrap_or(true))
}

fn reject_unenforceable_extras(v: &Value) -> Result<(), String> {
    for field in UNENFORCEABLE_EXTRA_FIELD_NAMES {
        if v.get(*field).is_some() {
            return Err(format!("unenforceable extra spawn field {field}"));
        }
    }
    Ok(())
}

fn thread_start(params: &Value, home: &Path) -> Result<Value, String> {
    reject_unenforceable_extras(params)?;
    let cwd = match params.get("cwd").and_then(Value::as_str) {
        Some(p) => {
            let path = PathBuf::from(p);
            if !path.is_absolute() {
                return Err("cwd must be absolute".to_string());
            }
            path.display().to_string()
        }
        None => {
            let path = std::env::current_dir().map_err(|e| e.to_string())?;
            if !path.is_absolute() {
                return Err("cwd must be absolute".to_string());
            }
            path.display().to_string()
        }
    };
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("grok")
        .to_string();
    let sandbox = match params.get("sandbox").and_then(Value::as_str) {
        Some(v) => parse_thread_sandbox(v).map_err(|e| e.to_string())?,
        None => APP_SERVER_DEFAULT_THREAD_SANDBOX,
    };
    let approval_policy = match params.get("approvalPolicy").and_then(Value::as_str) {
        Some(v) => parse_approval_policy(v).map_err(|e| e.to_string())?,
        None => APP_SERVER_DEFAULT_APPROVAL_POLICY,
    };
    let developer_instructions = params
        .get("developerInstructions")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let ledger = open_ledger(Some(home))?;
    let stored = ledger
        .create_thread(&NewThread {
            cwd,
            model,
            sandbox,
            approval_policy,
            developer_instructions,
            acp_session_id: String::new(),
        })
        .map_err(|e| e.to_string())?;
    Ok(json!({"thread": {"id": stored.id, "turns": []}}))
}

fn thread_resume(params: &Value, home: &Path) -> Result<Value, String> {
    reject_unenforceable_extras(params)?;
    let thread_id = params
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or_else(|| "threadId required".to_string())?;
    let ledger = open_ledger(Some(home))?;
    let stored = ledger.read_thread(thread_id).map_err(|e| e.to_string())?;
    Ok(json!({
        "thread": {
            "id": stored.id,
            "turns": turns_for_thread(&ledger, &stored.id)?,
        }
    }))
}

fn thread_read(params: &Value, home: &Path) -> Result<Value, String> {
    reject_unenforceable_extras(params)?;
    let thread_id = params
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or_else(|| "threadId required".to_string())?;
    let ledger = open_ledger(Some(home))?;
    let stored = ledger.read_thread(thread_id).map_err(|e| e.to_string())?;
    let turns = if params.get("includeTurns").and_then(Value::as_bool) == Some(false) {
        Vec::new()
    } else {
        turns_for_thread(&ledger, &stored.id)?
    };
    Ok(json!({"thread": {"id": stored.id, "turns": turns}}))
}

fn turn_start(params: &Value, home: &Path) -> Result<Value, String> {
    reject_unenforceable_extras(params)?;
    if let Some(policy) = params.get("sandboxPolicy") {
        reject_unenforceable_extras(policy)?;
    }
    let thread_id = params
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or_else(|| "threadId required".to_string())?
        .to_string();
    let prompt = prompt_from_input(params.get("input").unwrap_or(&Value::Null))?;
    let ledger = Arc::new(open_ledger(Some(home))?);
    let stored = ledger.read_thread(&thread_id).map_err(|e| e.to_string())?;
    let sandbox = match params.get("sandboxPolicy") {
        Some(policy) => {
            let ty = policy
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| "sandboxPolicy.type required".to_string())?;
            parse_turn_sandbox_type(ty)
                .map_err(|e| e.to_string())?
                .to_thread_sandbox()
        }
        None => stored.sandbox,
    };
    let approval = match params.get("approvalPolicy").and_then(Value::as_str) {
        Some(v) => parse_approval_policy(v).map_err(|e| e.to_string())?,
        None => stored.approval_policy,
    };
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(&stored.model)
        .to_string();
    let (follow_up_thread_id, reuse_thread_id) = if stored.acp_session_id.is_empty() {
        (None, Some(thread_id.clone()))
    } else {
        (Some(thread_id.clone()), None)
    };
    let req = TurnRequest {
        cwd: PathBuf::from(&stored.cwd),
        prompt,
        approval,
        sandbox,
        extra: ExtraSpawnFields::default(),
        model,
        command: acp_command(),
        client_request_id: None,
        follow_up_thread_id,
        reuse_thread_id,
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let ledger_bg = ledger.clone();
    thread::spawn(move || {
        let result = run_turn_on_admit(ledger_bg, &req, |turn| {
            let _ = tx.send(Ok(turn.clone()));
        });
        if let Err(err) = result {
            let _ = tx.try_send(Err(err.to_string()));
        }
    });
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(turn)) => Ok(turn_result(&turn)),
        Ok(Err(err)) => Err(err),
        Err(_) => Err("turn/start did not admit a turn".to_string()),
    }
}

fn turn_interrupt(params: &Value, home: &Path) -> Result<Value, String> {
    reject_unenforceable_extras(params)?;
    let thread_id = params
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or_else(|| "threadId required".to_string())?;
    let turn_id = params
        .get("turnId")
        .and_then(Value::as_str)
        .ok_or_else(|| "turnId required".to_string())?;
    let ledger = open_ledger(Some(home))?;
    let turn = ledger.read_turn(turn_id).map_err(|e| e.to_string())?;
    if turn.thread_id != thread_id {
        return Err("turn does not belong to thread".to_string());
    }
    Ok(turn_result(&cancel_turn(&ledger, turn_id)?))
}

fn prompt_from_input(input: &Value) -> Result<String, String> {
    let arr = input
        .as_array()
        .ok_or_else(|| "input required".to_string())?;
    if arr.is_empty() {
        return Err("input required".to_string());
    }
    let mut parts = Vec::new();
    for el in arr {
        let ty = el.get("type").and_then(Value::as_str).unwrap_or("");
        if ty != "text" {
            return Err(format!("unsupported input type {ty}"));
        }
        let text = el
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| "text required".to_string())?;
        parts.push(text.to_string());
    }
    Ok(parts.join("\n"))
}

fn turns_for_thread(ledger: &Ledger, thread_id: &str) -> Result<Vec<Value>, String> {
    let turns = ledger.list_turns(None).map_err(|e| e.to_string())?;
    Ok(turns
        .into_iter()
        .filter(|turn| turn.thread_id == thread_id)
        .map(|turn| {
            json!({
                "id": turn.id,
                "items": items_wire(&turn),
                "status": turn.status.as_str(),
            })
        })
        .collect())
}

fn item_obj(item: &Item) -> Value {
    json!({
        "id": item.id,
        "type": item.item_type,
        "text": item.text,
        "status": item.status,
    })
}

fn items_wire(turn: &Turn) -> Vec<Value> {
    turn.items.iter().map(item_obj).collect()
}

fn turn_result(turn: &Turn) -> Value {
    json!({
        "turn": {
            "id": turn.id,
            "items": items_wire(turn),
            "status": turn.status.as_str(),
        }
    })
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

    fn rpc(msg: &Value) -> Option<Value> {
        let home = tempfile::tempdir().expect("home");
        handle_rpc(msg, home.path())
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
        let reply = rpc(&req).expect("reply");
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
        let reply = rpc(&req).expect("reply");
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
        let nested = json!({
            "id": 3,
            "method": "initialize",
            "params": {"capabilities": {"ccas": true}}
        });
        let reply = rpc(&nested).expect("reply");
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn account_and_models_are_grok() {
        let account = rpc(&json!({"id": 4, "method": "account/read", "params": {}})).unwrap();
        assert_eq!(account["result"]["requiresOpenaiAuth"], false);
        let models = rpc(&json!({"id": 5, "method": "model/list", "params": {}})).unwrap();
        assert_eq!(models["result"]["data"][0]["model"], "grok");
        assert_eq!(models["result"]["data"][0]["isDefault"], true);
        assert!(models["result"]["nextCursor"].is_null());
    }

    #[test]
    fn initialized_notification_has_no_reply() {
        assert!(rpc(&json!({"method": "initialized", "params": {}})).is_none());
    }

    #[test]
    fn opt_out_notification_methods_must_stay_empty() {
        let ok = rpc(&json!({
            "id": 6,
            "method": "initialize",
            "params": {"capabilities": {"optOutNotificationMethods": []}}
        }))
        .unwrap();
        assert!(ok.get("result").is_some(), "{ok}");
        let blocked = rpc(&json!({
            "id": 7,
            "method": "initialize",
            "params": {"capabilities": {"optOutNotificationMethods": ["item/started"]}}
        }))
        .unwrap();
        assert_eq!(blocked["error"]["code"], METHOD_NOT_FOUND);
        assert!(
            blocked["error"]["message"]
                .as_str()
                .unwrap()
                .contains("optOutNotificationMethods"),
            "{blocked}"
        );
    }

    #[test]
    fn thread_start_omitted_defaults_are_untrusted_readonly() {
        let home = tempfile::tempdir().expect("home");
        let cwd = tempfile::tempdir().expect("cwd");
        let reply = handle_rpc(
            &json!({
                "id": 10,
                "method": "thread/start",
                "params": {"cwd": cwd.path().to_str().unwrap()}
            }),
            home.path(),
        )
        .expect("reply");
        let id = reply["result"]["thread"]["id"].as_str().expect("id");
        let ledger = samchi_core::ledger::Ledger::open(home.path()).unwrap();
        let stored = ledger.read_thread(id).unwrap();
        assert_eq!(
            stored.sandbox,
            samchi_core::source_wire::ThreadSandbox::ReadOnly
        );
        assert_eq!(
            stored.approval_policy,
            samchi_core::source_wire::ApprovalPolicy::Untrusted
        );
        assert!(reply["result"]["thread"]["turns"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn thread_fork_is_recognized_and_fail_closed() {
        let reply = rpc(&json!({
            "id": 11,
            "method": "thread/fork",
            "params": {"threadId": "th_x"}
        }))
        .unwrap();
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(reply["error"]["message"], "thread/fork");
    }

    #[test]
    fn extra_spawn_fields_fail_closed() {
        let home = tempfile::tempdir().expect("home");
        let cwd = tempfile::tempdir().expect("cwd");
        for field in UNENFORCEABLE_EXTRA_FIELD_NAMES {
            let mut params = json!({"cwd": cwd.path().to_str().unwrap()});
            params[*field] = json!(true);
            let reply = handle_rpc(
                &json!({"id": 12, "method": "thread/start", "params": params}),
                home.path(),
            )
            .unwrap();
            assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND, "{field}");
            assert!(
                reply["error"]["message"].as_str().unwrap().contains(field),
                "{reply}"
            );
        }
        let nested = handle_rpc(
            &json!({
                "id": 13,
                "method": "turn/start",
                "params": {
                    "threadId": "th_missing",
                    "input": [{"type": "text", "text": "x"}],
                    "sandboxPolicy": {
                        "type": "workspaceWrite",
                        "writableRoots": ["/tmp"]
                    }
                }
            }),
            home.path(),
        )
        .unwrap();
        assert!(
            nested["error"]["message"]
                .as_str()
                .unwrap()
                .contains("writableRoots"),
            "{nested}"
        );
        let interrupt = handle_rpc(
            &json!({
                "id": 17,
                "method": "turn/interrupt",
                "params": {
                    "threadId": "th_missing",
                    "turnId": "tu_missing",
                    "writableRoots": ["/tmp"]
                }
            }),
            home.path(),
        )
        .unwrap();
        assert!(
            interrupt["error"]["message"]
                .as_str()
                .unwrap()
                .contains("writableRoots"),
            "{interrupt}"
        );
    }

    #[test]
    fn thread_resume_reads_without_mutating() {
        let home = tempfile::tempdir().expect("home");
        let cwd = tempfile::tempdir().expect("cwd");
        let started = handle_rpc(
            &json!({
                "id": 14,
                "method": "thread/start",
                "params": {
                    "cwd": cwd.path().to_str().unwrap(),
                    "sandbox": "workspace-write",
                    "approvalPolicy": "never"
                }
            }),
            home.path(),
        )
        .unwrap();
        let id = started["result"]["thread"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let other = tempfile::tempdir().expect("other-cwd");
        let resumed = handle_rpc(
            &json!({
                "id": 15,
                "method": "thread/resume",
                "params": {
                    "threadId": id,
                    "cwd": other.path().to_str().unwrap()
                }
            }),
            home.path(),
        )
        .unwrap();
        assert_eq!(resumed["result"]["thread"]["id"], id);
        let ledger = samchi_core::ledger::Ledger::open(home.path()).unwrap();
        let stored = ledger.read_thread(&id).unwrap();
        assert_eq!(stored.cwd, cwd.path().display().to_string());
        assert_eq!(
            stored.sandbox,
            samchi_core::source_wire::ThreadSandbox::WorkspaceWrite
        );
        let read = handle_rpc(
            &json!({
                "id": 16,
                "method": "thread/read",
                "params": {"threadId": id, "includeTurns": true}
            }),
            home.path(),
        )
        .unwrap();
        assert_eq!(read["result"]["thread"]["id"], id);
        assert!(read["result"]["thread"]["turns"]
            .as_array()
            .unwrap()
            .is_empty());
        let compact = handle_rpc(
            &json!({
                "id": 18,
                "method": "thread/read",
                "params": {"threadId": id, "includeTurns": false}
            }),
            home.path(),
        )
        .unwrap();
        assert!(compact["result"]["thread"]["turns"]
            .as_array()
            .unwrap()
            .is_empty());
        let extras = handle_rpc(
            &json!({
                "id": 19,
                "method": "thread/resume",
                "params": {"threadId": id, "writableRoots": ["/tmp"]}
            }),
            home.path(),
        )
        .unwrap();
        assert!(
            extras["error"]["message"]
                .as_str()
                .unwrap()
                .contains("writableRoots"),
            "{extras}"
        );
        let turn = ledger
            .admit_turn(&samchi_core::ledger::NewTurn {
                thread_id: id.clone(),
                input: vec![samchi_core::source_wire::UserInput {
                    kind: "text".to_string(),
                    text: "x".to_string(),
                }],
                model: "grok".to_string(),
                effort: String::new(),
                client_request_id: None,
            })
            .unwrap();
        let mismatch = handle_rpc(
            &json!({
                "id": 20,
                "method": "turn/interrupt",
                "params": {"threadId": "th_other", "turnId": turn.id}
            }),
            home.path(),
        )
        .unwrap();
        assert!(
            mismatch["error"]["message"]
                .as_str()
                .unwrap()
                .contains("does not belong"),
            "{mismatch}"
        );
    }
}
