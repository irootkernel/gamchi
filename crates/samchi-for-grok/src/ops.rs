//! Shared ledger/JSON helpers for CLI and MCP facades.

use samchi_adapter_grok::{teardown_process_group, AgentCommand};
use samchi_core::home::resolve_home_from_os;
use samchi_core::ledger::Ledger;
use samchi_core::source_wire::Turn;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const MAX_WAIT_MS: u64 = 50_000;
pub const RESULT_CAP: usize = 64 * 1024;

pub fn acp_command() -> AgentCommand {
    match std::env::var_os("SAMCHI_FOR_GROK_ACP_PROGRAM") {
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
pub fn cancel_turn(ledger: &Ledger, turn_id: &str) -> Result<Turn, String> {
    let turn = ledger.cancel(turn_id).map_err(|e| e.to_string())?;
    if let Ok(generation) = ledger.read_generation(&turn.generation_id) {
        if let Some(pid) = generation.child_pid {
            teardown_process_group(pid);
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
