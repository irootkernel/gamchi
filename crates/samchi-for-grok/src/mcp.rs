//! MCP stdio facade. TASK-013 publishes grok_respond; spawn still returns immediately.

use crate::ops::{
    acp_command, bounded_turn_json, cancel_turn, open_home, open_ledger, turn_json, MAX_WAIT_MS,
};
use samchi_adapter_grok::{
    run_turn_on_admit, ExtraSpawnFields, TurnRequest, UNENFORCEABLE_EXTRA_FIELD_NAMES,
};
use samchi_core::source_wire::{
    parse_approval_decision, parse_approval_policy, parse_thread_sandbox, ApprovalPolicy,
    ThreadSandbox,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const PROTOCOL: &str = "2024-11-05";

pub fn run_mcp(args: &[&str], stdin: &mut dyn BufRead, stdout: &mut dyn Write) -> u8 {
    let home = match parse_home(args) {
        Ok(h) => h,
        Err(_) => {
            let _ = writeln!(io::stderr(), "SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG");
            return 1;
        }
    };
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) => return 0,
            Ok(_) => {}
            Err(_) => return 1,
        }
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if msg.get("method").and_then(Value::as_str) == Some("notifications/initialized") {
            continue;
        }
        let Some(id) = msg.get("id").cloned() else {
            continue;
        };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let result = match method {
            "initialize" => initialize_result(),
            "tools/list" => tools_list(),
            "ping" => json!({}),
            "tools/call" => match call_tool(&params, home.as_deref()) {
                Ok(v) => tool_text(v, false),
                Err(err) => tool_text(json!({"error": err}), true),
            },
            _ => {
                write_rpc(
                    stdout,
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":method}}),
                );
                continue;
            }
        };
        write_rpc(stdout, json!({"jsonrpc":"2.0","id":id,"result":result}));
    }
}

fn parse_home(args: &[&str]) -> Result<Option<PathBuf>, ()> {
    let mut i = 0;
    let mut home = None;
    while i < args.len() {
        match args[i] {
            "--home" => {
                i += 1;
                home = Some(PathBuf::from(*args.get(i).ok_or(())?));
            }
            _ => return Err(()),
        }
        i += 1;
    }
    Ok(home)
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [
            tool("grok_spawn", "Start a Grok turn. Returns ids immediately. Default never + workspace-write.", spawn_schema()),
            tool("grok_await", "Wait until the turn is terminal. Host timeout does not cancel.", id_schema()),
            tool("grok_wait", "Bounded wait. timeout_ms default and max 50000. Does not cancel.", wait_schema()),
            tool("grok_status", "One snapshot of a turn.", id_schema()),
            tool("grok_result", "Terminal envelope, or ready=false.", id_schema()),
            tool("grok_list", "Recent turns from the disk ledger.", list_schema()),
            tool(
                "grok_cancel",
                "Tear down the Grok process group. TurnStatus interrupted. Host timeout is not cancel.",
                id_schema(),
            ),
            tool(
                "grok_followup",
                "New turn on the same ACP session via session/load.",
                followup_schema(),
            ),
            tool(
                "grok_respond",
                "Answer a pending_approval with request_id. Then grok_await again.",
                respond_schema(),
            ),
        ]
    })
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": schema})
}

fn spawn_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "prompt": {"type": "string"},
            "cwd": {"type": "string"},
            "home": {"type": "string"},
            "approvalPolicy": {"type": "string"},
            "sandbox": {"type": "string"},
            "client_request_id": {"type": "string"}
        },
        "required": ["prompt"]
    })
}

fn id_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"turn_id": {"type": "string"}, "home": {"type": "string"}},
        "required": ["turn_id"]
    })
}

fn wait_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "turn_id": {"type": "string"},
            "timeout_ms": {"type": "integer"},
            "home": {"type": "string"}
        },
        "required": ["turn_id"]
    })
}

fn list_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"cwd": {"type": "string"}, "home": {"type": "string"}}
    })
}

fn respond_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "request_id": {"type": "string"},
            "decision": {"type": "string"},
            "home": {"type": "string"}
        },
        "required": ["request_id", "decision"]
    })
}

fn followup_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "thread_id": {"type": "string"},
            "prompt": {"type": "string"},
            "home": {"type": "string"}
        },
        "required": ["thread_id", "prompt"]
    })
}

fn call_tool(params: &Value, cli_home: Option<&Path>) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing tool name".to_string())?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    match name {
        "grok_spawn" => spawn(&args, cli_home),
        "grok_await" => await_turn(&args, cli_home, None),
        "grok_wait" => wait_turn(&args, cli_home),
        "grok_status" => status_turn(&args, cli_home, false),
        "grok_result" => status_turn(&args, cli_home, true),
        "grok_list" => list_turns(&args, cli_home),
        "grok_cancel" => cancel_tool(&args, cli_home),
        "grok_followup" => followup(&args, cli_home),
        "grok_respond" => respond_tool(&args, cli_home),
        other => Err(format!("unknown tool {other}")),
    }
}

fn spawn(args: &Value, cli_home: Option<&Path>) -> Result<Value, String> {
    let prompt = args
        .get("prompt")
        .and_then(Value::as_str)
        .ok_or_else(|| "prompt required".to_string())?
        .to_string();
    let approval = match args.get("approvalPolicy").and_then(Value::as_str) {
        Some(v) => parse_approval_policy(v).map_err(|e| e.to_string())?,
        None => ApprovalPolicy::Never,
    };
    let sandbox = match args.get("sandbox").and_then(Value::as_str) {
        Some(v) => parse_thread_sandbox(v).map_err(|e| e.to_string())?,
        None => ThreadSandbox::WorkspaceWrite,
    };
    for field in UNENFORCEABLE_EXTRA_FIELD_NAMES {
        if args.get(*field).is_some() {
            return Err(format!("unenforceable extra spawn field {field}"));
        }
    }
    let home_arg = args
        .get("home")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| cli_home.map(Path::to_path_buf));
    let home = open_home(home_arg.as_deref())?;
    let cwd = match args.get("cwd").and_then(Value::as_str) {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    if !cwd.is_absolute() {
        return Err("cwd must be absolute".to_string());
    }
    let client_request_id = args
        .get("client_request_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let ledger = Arc::new(open_ledger(Some(home.as_path()))?);
    let req = TurnRequest {
        cwd,
        prompt,
        approval,
        sandbox,
        extra: ExtraSpawnFields::default(),
        model: "grok".to_string(),
        effort: String::new(),
        command: acp_command(),
        client_request_id,
        follow_up_thread_id: None,
        reuse_thread_id: None,
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let ledger_bg = ledger.clone();
    thread::spawn(move || {
        let _ = run_turn_on_admit(ledger_bg, &req, |turn| {
            let _ = tx.send((turn.thread_id.clone(), turn.id.clone()));
        });
    });
    let (thread_id, turn_id) = rx
        .recv_timeout(Duration::from_secs(30))
        .map_err(|_| "spawn did not admit a turn".to_string())?;
    Ok(json!({"thread_id": thread_id, "turn_id": turn_id}))
}

fn await_turn(
    args: &Value,
    cli_home: Option<&Path>,
    timeout: Option<Duration>,
) -> Result<Value, String> {
    let turn_id = turn_id(args)?;
    let ledger = ledger_from(args, cli_home)?;
    let turn = ledger.wait(&turn_id, timeout).map_err(|e| e.to_string())?;
    if timeout.is_none() && !turn.status.is_terminal() && !turn.pending_approval() {
        return Err("await returned non-terminal".to_string());
    }
    Ok(bounded_turn_json(&turn, Some(ledger.home())))
}

fn wait_turn(args: &Value, cli_home: Option<&Path>) -> Result<Value, String> {
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(MAX_WAIT_MS);
    if timeout_ms == 0 || timeout_ms > MAX_WAIT_MS {
        return Err("timeout_ms must be 1..=50000".to_string());
    }
    await_turn(args, cli_home, Some(Duration::from_millis(timeout_ms)))
}

fn status_turn(args: &Value, cli_home: Option<&Path>, result: bool) -> Result<Value, String> {
    let turn_id = turn_id(args)?;
    let ledger = ledger_from(args, cli_home)?;
    let turn = ledger.observe(&turn_id).map_err(|e| e.to_string())?;
    if result && !turn.status.is_terminal() {
        return Ok(json!({"ready": false, "not_ready": true, "turn_id": turn.id}));
    }
    Ok(bounded_turn_json(&turn, Some(ledger.home())))
}

fn list_turns(args: &Value, cli_home: Option<&Path>) -> Result<Value, String> {
    let ledger = ledger_from(args, cli_home)?;
    let cwd = args.get("cwd").and_then(Value::as_str);
    let turns = ledger.list_turns(cwd).map_err(|e| e.to_string())?;
    Ok(json!({"turns": turns.iter().map(turn_json).collect::<Vec<_>>()}))
}

fn respond_tool(args: &Value, cli_home: Option<&Path>) -> Result<Value, String> {
    let request_id = args
        .get("request_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "request_id required".to_string())?;
    let decision = args
        .get("decision")
        .and_then(Value::as_str)
        .ok_or_else(|| "decision required".to_string())?;
    let decision = parse_approval_decision(decision).map_err(|e| e.to_string())?;
    let ledger = ledger_from(args, cli_home)?;
    let turn = ledger
        .respond(request_id, decision)
        .map_err(|e| e.to_string())?;
    Ok(bounded_turn_json(&turn, Some(ledger.home())))
}

fn cancel_tool(args: &Value, cli_home: Option<&Path>) -> Result<Value, String> {
    let turn_id = turn_id(args)?;
    let ledger = ledger_from(args, cli_home)?;
    let turn = cancel_turn(&ledger, &turn_id)?;
    Ok(bounded_turn_json(&turn, Some(ledger.home())))
}

fn followup(args: &Value, cli_home: Option<&Path>) -> Result<Value, String> {
    let thread_id = args
        .get("thread_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "thread_id required".to_string())?
        .to_string();
    let prompt = args
        .get("prompt")
        .and_then(Value::as_str)
        .ok_or_else(|| "prompt required".to_string())?
        .to_string();
    let home_arg = args
        .get("home")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| cli_home.map(Path::to_path_buf));
    let home = open_home(home_arg.as_deref())?;
    let ledger = Arc::new(open_ledger(Some(home.as_path()))?);
    let thread = ledger.read_thread(&thread_id).map_err(|e| e.to_string())?;
    if thread.acp_session_id.is_empty() {
        return Err("follow-up requires a stored ACP session id".to_string());
    }
    let cwd = PathBuf::from(&thread.cwd);
    let req = TurnRequest {
        cwd,
        prompt,
        approval: thread.approval_policy,
        sandbox: thread.sandbox,
        extra: ExtraSpawnFields::default(),
        model: thread.model,
        effort: String::new(),
        command: acp_command(),
        client_request_id: None,
        follow_up_thread_id: Some(thread_id),
        reuse_thread_id: None,
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let ledger_bg = ledger.clone();
    thread::spawn(move || {
        let _ = run_turn_on_admit(ledger_bg, &req, |turn| {
            let _ = tx.send((turn.thread_id.clone(), turn.id.clone()));
        });
    });
    let (thread_id, turn_id) = rx
        .recv_timeout(Duration::from_secs(30))
        .map_err(|_| "follow-up did not admit a turn".to_string())?;
    Ok(json!({"thread_id": thread_id, "turn_id": turn_id}))
}

fn turn_id(args: &Value) -> Result<String, String> {
    args.get("turn_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "turn_id required".to_string())
}

fn ledger_from(
    args: &Value,
    cli_home: Option<&Path>,
) -> Result<samchi_core::ledger::Ledger, String> {
    let home = args
        .get("home")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| cli_home.map(Path::to_path_buf));
    open_ledger(home.as_deref())
}

fn tool_text(v: Value, is_error: bool) -> Value {
    json!({
        "content": [{"type": "text", "text": v.to_string()}],
        "isError": is_error
    })
}

fn write_rpc(stdout: &mut dyn Write, v: Value) {
    let _ = writeln!(stdout, "{v}");
    let _ = stdout.flush();
}
