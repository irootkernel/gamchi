//! Shared ledger/JSON helpers for CLI and MCP facades.

use samchi_adapter_grok::{
    run_turn_on_admit, teardown_process_group, AgentCommand, ExtraSpawnFields, TurnRequest,
};
use samchi_core::home::resolve_home_from_os;
use samchi_core::ledger::Ledger;
use samchi_core::source_wire::{ApprovalPolicy, Thread, ThreadSandbox, Turn, TurnStatus};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub const MAX_WAIT_MS: u64 = 50_000;
pub const RESULT_CAP: usize = 64 * 1024;

/// Absent or JSON null is omitted. Present blank/whitespace is `INVALID_CONFIG`.
pub fn optional_text_field(args: &Value, key: &str) -> Result<String, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(v) => {
            let s = v
                .as_str()
                .ok_or_else(|| format!("{key} must be a string"))?;
            if s.trim().is_empty() {
                Err(format!("INVALID_CONFIG: blank {key}"))
            } else {
                Ok(s.to_string())
            }
        }
    }
}

pub fn start_request(
    cwd: PathBuf,
    prompt: String,
    approval: ApprovalPolicy,
    sandbox: ThreadSandbox,
    model: String,
    effort: String,
    client_request_id: Option<String>,
) -> TurnRequest {
    TurnRequest {
        cwd,
        prompt,
        approval,
        sandbox,
        extra: ExtraSpawnFields::default(),
        model,
        effort,
        command: acp_command(),
        client_request_id,
        follow_up_thread_id: None,
        reuse_thread_id: None,
    }
}

pub fn followup_request(
    thread: &Thread,
    prompt: String,
    model: String,
    effort: String,
    cwd: Option<PathBuf>,
) -> TurnRequest {
    TurnRequest {
        cwd: cwd.unwrap_or_else(|| PathBuf::from(&thread.cwd)),
        prompt,
        approval: thread.approval_policy,
        sandbox: thread.sandbox,
        extra: ExtraSpawnFields::default(),
        model,
        effort,
        command: acp_command(),
        client_request_id: None,
        follow_up_thread_id: Some(thread.id.clone()),
        reuse_thread_id: None,
    }
}

/// Admit on a background thread so MCP spawn/followup can return ids.
/// Pre-admit failures (including follow-up model lock) return immediately.
pub fn admit_in_background(
    ledger: Arc<Ledger>,
    req: TurnRequest,
) -> Result<(String, String), String> {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let sent = AtomicBool::new(false);
        let result = run_turn_on_admit(ledger, &req, |turn| {
            sent.store(true, Ordering::SeqCst);
            let _ = tx.send(Ok((turn.thread_id.clone(), turn.id.clone())));
        });
        if let Err(err) = result {
            if !sent.load(Ordering::SeqCst) {
                let _ = tx.send(Err(err.to_string()));
            }
        }
    });
    rx.recv_timeout(Duration::from_secs(30))
        .map_err(|_| "spawn did not admit a turn".to_string())?
}

pub fn acp_command() -> AgentCommand {
    match std::env::var_os("GAMCHI_ACP_PROGRAM") {
        Some(p) => AgentCommand::Override {
            program: PathBuf::from(p),
            args: Vec::new(),
        },
        None => AgentCommand::Grok {
            program: PathBuf::from("grok"),
        },
    }
}

pub fn open_home(explicit: Option<&Path>) -> Result<PathBuf, String> {
    resolve_home_from_os(explicit).map_err(|e| e.reason)
}

pub fn open_ledger(explicit: Option<&Path>) -> Result<Ledger, String> {
    let home = open_home(explicit)?;
    Ledger::open(home).map_err(|e| e.to_string())
}

/// Publish `interrupted` then tear down the Grok child process group.
/// Dead generations converge to `failed`/`worker_gone` first. Teardown runs
/// only when this call publishes interrupted and the recorded child pid still
/// matches that start epoch.
pub fn cancel_turn(ledger: &Ledger, turn_id: &str) -> Result<Turn, String> {
    let turn = ledger.cancel(turn_id).map_err(|e| e.to_string())?;
    if turn.status != TurnStatus::Interrupted {
        return Ok(turn);
    }
    if let Ok(generation) = ledger.read_generation(&turn.generation_id) {
        if let Some(pid) = generation.child_pid {
            if Ledger::recorded_child_is_alive(&generation) {
                teardown_process_group(pid);
            }
        }
    }
    ledger.read_turn(turn_id).map_err(|e| e.to_string())
}

pub fn turn_json(turn: &Turn) -> Value {
    let mut v = json!({
        "turn_id": turn.id,
        "thread_id": turn.thread_id,
        "status": turn.status.as_str(),
        "stop_reason": turn.stop_reason,
        "failure_reason": turn.failure_reason,
        "items": turn.items,
    });
    if turn.pending_approval() {
        if let Some(obj) = v.as_object_mut() {
            obj.insert("await_reason".into(), json!("pending_approval"));
            obj.insert("request_id".into(), json!(turn.pending_request_id));
        }
    }
    v
}

pub fn bounded_turn_json(turn: &Turn, home: Option<&Path>) -> Value {
    let mut v = turn_json(turn);
    let raw = serde_json::to_vec(&v).unwrap_or_default();
    if raw.len() <= RESULT_CAP {
        v.as_object_mut()
            .expect("obj")
            .insert("truncated".into(), Value::Bool(false));
        return v;
    }
    if let Some(home) = home {
        let path = home.join("results").join(format!("{}.json", turn.id));
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, &raw);
        if let Some(obj) = v.as_object_mut() {
            obj.insert("truncated".into(), Value::Bool(true));
            obj.insert("path".into(), json!(path.display().to_string()));
            obj.insert("items".into(), json!([]));
        }
    } else if let Some(obj) = v.as_object_mut() {
        obj.insert("truncated".into(), Value::Bool(true));
        obj.insert("items".into(), json!([]));
    }
    v
}
