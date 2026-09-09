//! CLI worker facade. TASK-013 publishes respond with start/wait/status/result/list/cancel/followup.

use crate::ops::{acp_command, bounded_turn_json, cancel_turn, turn_json};
use samchi_adapter_grok::{run_turn_on_admit, ExtraSpawnFields, TurnRequest};
use samchi_core::home::{resolve_home_from_os, HomeError};
use samchi_core::ledger::{Ledger, LedgerError};
use samchi_core::source_wire::{
    parse_approval_decision, parse_approval_policy, ApprovalPolicy, ThreadSandbox,
};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub const WORKER_USAGE: &str = "\
Usage:
  samchi-for-grok worker start --json [--home <absolute-path>] [--cwd <absolute-path>] [--approval-policy <never|untrusted|on-request>] <prompt>
  samchi-for-grok worker wait --json --turn-id <id> [--home <absolute-path>] [--timeout-ms <1-50000>]
  samchi-for-grok worker status --json --turn-id <id> [--home <absolute-path>]
  samchi-for-grok worker result --json --turn-id <id> [--home <absolute-path>]
  samchi-for-grok worker list --json [--home <absolute-path>] [--cwd <absolute-path>]
  samchi-for-grok worker cancel --json --turn-id <id> [--home <absolute-path>]
  samchi-for-grok worker followup --json --thread-id <id> [--home <absolute-path>] [--cwd <absolute-path>] <prompt>
  samchi-for-grok worker respond --json --request-id <id> --decision <accept|acceptForSession|decline|cancel> [--home <absolute-path>]
";

const MAX_WAIT_MS: u64 = 50_000;

pub fn run_worker(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    if args.is_empty() {
        write_err(stderr, "INVALID_CONFIG");
        write_all(stderr, WORKER_USAGE);
        return 2;
    }
    match args[0] {
        "start" => cmd_start(&args[1..], stdout, stderr),
        "wait" => cmd_wait(&args[1..], stdout, stderr),
        "status" => cmd_status(&args[1..], stdout, stderr),
        "result" => cmd_result(&args[1..], stdout, stderr),
        "list" => cmd_list(&args[1..], stdout, stderr),
        "cancel" => cmd_cancel(&args[1..], stdout, stderr),
        "followup" => cmd_followup(&args[1..], stdout, stderr),
        "respond" => cmd_respond(&args[1..], stdout, stderr),
        "await" => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            1
        }
        _ => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            1
        }
    }
}

fn cmd_start(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let parsed = match parse_flags(args, true) {
        Ok(p) => p,
        Err(code) => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            return code;
        }
    };
    if !parsed.json || parsed.prompt.is_empty() {
        write_err(stderr, "INVALID_CONFIG");
        write_all(stderr, WORKER_USAGE);
        return 1;
    }
    let home = match open_home(parsed.home.as_deref(), stderr) {
        Ok(h) => h,
        Err(code) => return code,
    };
    let cwd = match parsed.cwd {
        Some(p) => p,
        None => match std::env::current_dir() {
            Ok(p) if p.is_absolute() => p,
            _ => {
                write_err(stderr, "INVALID_CONFIG");
                return 1;
            }
        },
    };
    let ledger = match Ledger::open(&home) {
        Ok(l) => Arc::new(l),
        Err(err) => return ledger_err(stderr, err),
    };
    let req = TurnRequest {
        cwd,
        prompt: parsed.prompt,
        approval: parsed.approval,
        sandbox: ThreadSandbox::WorkspaceWrite,
        extra: ExtraSpawnFields::default(),
        model: "grok".to_string(),
        effort: String::new(),
        command: acp_command(),
        client_request_id: None,
        follow_up_thread_id: None,
        reuse_thread_id: None,
    };
    let printed = std::sync::Mutex::new(false);
    let result = run_turn_on_admit(ledger, &req, |turn| {
        let ids = json!({"thread_id": turn.thread_id, "turn_id": turn.id});
        let mut out = io::stdout();
        write_json(&mut out, &ids);
        let _ = out.flush();
        *printed.lock().expect("print") = true;
    });
    match result {
        Ok(outcome) => {
            if !*printed.lock().expect("print") {
                let ids = json!({
                    "thread_id": outcome.turn.thread_id,
                    "turn_id": outcome.turn.id
                });
                write_json(stdout, &ids);
            }
            0
        }
        Err(err) => {
            write_all(
                stderr,
                &format!("SAMCHI_FOR_GROK_STARTUP_ERROR ACP {err}\n"),
            );
            1
        }
    }
}

fn cmd_wait(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let parsed = match parse_flags(args, false) {
        Ok(p) => p,
        Err(_) => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            return 1;
        }
    };
    if !parsed.json || parsed.turn_id.is_empty() {
        write_err(stderr, "INVALID_CONFIG");
        write_all(stderr, WORKER_USAGE);
        return 1;
    }
    let timeout = parsed.timeout_ms.unwrap_or(MAX_WAIT_MS);
    if timeout == 0 || timeout > MAX_WAIT_MS {
        write_err(stderr, "INVALID_CONFIG");
        return 1;
    }
    let ledger = match open_ledger(parsed.home.as_deref(), stderr) {
        Ok(l) => l,
        Err(code) => return code,
    };
    match ledger.wait(&parsed.turn_id, Some(Duration::from_millis(timeout))) {
        Ok(turn) => {
            write_json(stdout, &turn_json(&turn));
            0
        }
        Err(err) => ledger_err(stderr, err),
    }
}

fn cmd_status(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    snapshot(args, stdout, stderr, false)
}

fn cmd_result(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    snapshot(args, stdout, stderr, true)
}

fn snapshot(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write, result: bool) -> u8 {
    let parsed = match parse_flags(args, false) {
        Ok(p) => p,
        Err(_) => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            return 1;
        }
    };
    if !parsed.json || parsed.turn_id.is_empty() {
        write_err(stderr, "INVALID_CONFIG");
        write_all(stderr, WORKER_USAGE);
        return 1;
    }
    let ledger = match open_ledger(parsed.home.as_deref(), stderr) {
        Ok(l) => l,
        Err(code) => return code,
    };
    match ledger.observe(&parsed.turn_id) {
        Ok(turn) => {
            if result && !turn.status.is_terminal() {
                write_json(
                    stdout,
                    &json!({"ready": false, "not_ready": true, "turn_id": turn.id}),
                );
                return 0;
            }
            write_json(stdout, &bounded_turn_json(&turn, Some(ledger.home())));
            0
        }
        Err(err) => ledger_err(stderr, err),
    }
}

fn cmd_followup(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let parsed = match parse_flags(args, true) {
        Ok(p) => p,
        Err(_) => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            return 1;
        }
    };
    if !parsed.json || parsed.thread_id.is_empty() || parsed.prompt.is_empty() {
        write_err(stderr, "INVALID_CONFIG");
        write_all(stderr, WORKER_USAGE);
        return 1;
    }
    let ledger = match open_ledger(parsed.home.as_deref(), stderr) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let thread = match ledger.read_thread(&parsed.thread_id) {
        Ok(t) => t,
        Err(err) => return ledger_err(stderr, err),
    };
    if thread.acp_session_id.is_empty() {
        write_err(stderr, "ACP");
        write_all(stderr, "follow-up requires a stored ACP session id\n");
        return 1;
    }
    let cwd = match parsed.cwd {
        Some(p) => p,
        None => PathBuf::from(&thread.cwd),
    };
    let req = TurnRequest {
        cwd,
        prompt: parsed.prompt,
        approval: thread.approval_policy,
        sandbox: thread.sandbox,
        extra: ExtraSpawnFields::default(),
        model: thread.model,
        effort: String::new(),
        command: acp_command(),
        client_request_id: None,
        follow_up_thread_id: Some(thread.id),
        reuse_thread_id: None,
    };
    let printed = std::sync::Mutex::new(false);
    let result = run_turn_on_admit(Arc::new(ledger), &req, |turn| {
        let ids = json!({"thread_id": turn.thread_id, "turn_id": turn.id});
        let mut out = io::stdout();
        write_json(&mut out, &ids);
        let _ = out.flush();
        *printed.lock().expect("print") = true;
    });
    match result {
        Ok(outcome) => {
            if !*printed.lock().expect("print") {
                let ids = json!({
                    "thread_id": outcome.turn.thread_id,
                    "turn_id": outcome.turn.id
                });
                write_json(stdout, &ids);
            }
            0
        }
        Err(err) => {
            write_all(
                stderr,
                &format!("SAMCHI_FOR_GROK_STARTUP_ERROR ACP {err}\n"),
            );
            1
        }
    }
}

fn cmd_respond(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let parsed = match parse_flags(args, false) {
        Ok(p) => p,
        Err(_) => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            return 1;
        }
    };
    if !parsed.json || parsed.request_id.is_empty() || parsed.decision.is_empty() {
        write_err(stderr, "INVALID_CONFIG");
        write_all(stderr, WORKER_USAGE);
        return 1;
    }
    let decision = match parse_approval_decision(&parsed.decision) {
        Ok(d) => d,
        Err(_) => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            return 1;
        }
    };
    let ledger = match open_ledger(parsed.home.as_deref(), stderr) {
        Ok(l) => l,
        Err(code) => return code,
    };
    match ledger.respond(&parsed.request_id, decision) {
        Ok(turn) => {
            write_json(stdout, &bounded_turn_json(&turn, Some(ledger.home())));
            0
        }
        Err(err) => {
            write_err(stderr, "ACP");
            write_all(stderr, &format!("{err}\n"));
            1
        }
    }
}

fn cmd_cancel(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let parsed = match parse_flags(args, false) {
        Ok(p) => p,
        Err(_) => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            return 1;
        }
    };
    if !parsed.json || parsed.turn_id.is_empty() {
        write_err(stderr, "INVALID_CONFIG");
        write_all(stderr, WORKER_USAGE);
        return 1;
    }
    let ledger = match open_ledger(parsed.home.as_deref(), stderr) {
        Ok(l) => l,
        Err(code) => return code,
    };
    match cancel_turn(&ledger, &parsed.turn_id) {
        Ok(turn) => {
            write_json(stdout, &bounded_turn_json(&turn, Some(ledger.home())));
            0
        }
        Err(err) => {
            write_err(stderr, "ACP");
            write_all(stderr, &format!("{err}\n"));
            1
        }
    }
}

fn cmd_list(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    let parsed = match parse_flags(args, false) {
        Ok(p) => p,
        Err(_) => {
            write_err(stderr, "INVALID_CONFIG");
            write_all(stderr, WORKER_USAGE);
            return 1;
        }
    };
    if !parsed.json {
        write_err(stderr, "INVALID_CONFIG");
        write_all(stderr, WORKER_USAGE);
        return 1;
    }
    let ledger = match open_ledger(parsed.home.as_deref(), stderr) {
        Ok(l) => l,
        Err(code) => return code,
    };
    let cwd = parsed.cwd.as_ref().map(|p| p.display().to_string());
    match ledger.list_turns(cwd.as_deref()) {
        Ok(turns) => {
            let rows: Vec<Value> = turns.iter().map(turn_json).collect();
            write_json(stdout, &json!({"turns": rows}));
            0
        }
        Err(err) => ledger_err(stderr, err),
    }
}

struct Flags {
    json: bool,
    home: Option<PathBuf>,
    cwd: Option<PathBuf>,
    turn_id: String,
    thread_id: String,
    timeout_ms: Option<u64>,
    prompt: String,
    approval: ApprovalPolicy,
    request_id: String,
    decision: String,
}

fn parse_flags(args: &[&str], take_prompt: bool) -> Result<Flags, u8> {
    let mut json = false;
    let mut home = None;
    let mut cwd = None;
    let mut turn_id = String::new();
    let mut thread_id = String::new();
    let mut timeout_ms = None;
    let mut approval = ApprovalPolicy::Never;
    let mut request_id = String::new();
    let mut decision = String::new();
    let mut i = 0;
    let mut prompt_parts = Vec::new();
    while i < args.len() {
        match args[i] {
            "--json" => json = true,
            "--home" => {
                i += 1;
                let p = args.get(i).ok_or(1u8)?;
                home = Some(PathBuf::from(p));
            }
            "--cwd" => {
                i += 1;
                let p = args.get(i).ok_or(1u8)?;
                cwd = Some(PathBuf::from(p));
            }
            "--turn-id" => {
                i += 1;
                turn_id = args.get(i).ok_or(1u8)?.to_string();
            }
            "--thread-id" => {
                i += 1;
                thread_id = args.get(i).ok_or(1u8)?.to_string();
            }
            "--timeout-ms" => {
                i += 1;
                timeout_ms = Some(args.get(i).ok_or(1u8)?.parse().map_err(|_| 1u8)?);
            }
            "--approval-policy" => {
                i += 1;
                approval = parse_approval_policy(args.get(i).ok_or(1u8)?).map_err(|_| 1u8)?;
            }
            "--request-id" => {
                i += 1;
                request_id = args.get(i).ok_or(1u8)?.to_string();
            }
            "--decision" => {
                i += 1;
                decision = args.get(i).ok_or(1u8)?.to_string();
            }
            "--" => {
                prompt_parts.extend(args[i + 1..].iter().map(|s| (*s).to_string()));
                break;
            }
            flag if flag.starts_with('-') => return Err(1),
            other if take_prompt => prompt_parts.push(other.to_string()),
            _ => return Err(1),
        }
        i += 1;
    }
    Ok(Flags {
        json,
        home,
        cwd,
        turn_id,
        thread_id,
        timeout_ms,
        prompt: prompt_parts.join(" "),
        approval,
        request_id,
        decision,
    })
}

fn open_home(explicit: Option<&Path>, stderr: &mut dyn Write) -> Result<PathBuf, u8> {
    match resolve_home_from_os(explicit) {
        Ok(p) => Ok(p),
        Err(HomeError { reason }) => {
            write_all(
                stderr,
                &format!("SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG {reason}\n"),
            );
            Err(1)
        }
    }
}

fn open_ledger(explicit: Option<&Path>, stderr: &mut dyn Write) -> Result<Ledger, u8> {
    let home = open_home(explicit, stderr)?;
    Ledger::open(home).map_err(|err| ledger_err(stderr, err))
}

fn ledger_err(stderr: &mut dyn Write, err: LedgerError) -> u8 {
    write_all(
        stderr,
        &format!("SAMCHI_FOR_GROK_STARTUP_ERROR LEDGER {err}\n"),
    );
    1
}

fn write_json(w: &mut dyn Write, v: &Value) {
    write_all(w, &format!("{v}\n"));
}

fn write_err(w: &mut dyn Write, code: &str) {
    write_all(w, &format!("SAMCHI_FOR_GROK_STARTUP_ERROR {code}\n"));
}

fn write_all(w: &mut dyn Write, s: &str) {
    w.write_all(s.as_bytes()).expect("write");
}
