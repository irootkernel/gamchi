//! CLI worker verbs against the fake ACP agent. Does not spawn live Grok.

use samchi_core::ledger::{Ledger, NewThread, NewTurn};
use samchi_core::source_wire::{ApprovalPolicy, ThreadSandbox, TurnStatus, UserInput};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_samchi-for-grok")
}

fn fake_agent() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_BIN_EXE_samchi-for-grok"));
    p.set_file_name("fake-acp-agent");
    p
}

#[test]
fn start_prints_ids_then_reaches_terminal() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let out = Command::new(bin())
        .env("SAMCHI_FOR_GROK_ACP_PROGRAM", fake_agent())
        .args([
            "worker",
            "start",
            "--json",
            "--home",
            home.path().to_str().unwrap(),
            "--cwd",
            cwd.path().to_str().unwrap(),
            "ping",
        ])
        .output()
        .expect("start");
    assert!(
        out.status.success(),
        "stderr {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().next().expect("ids line");
    let v: Value = serde_json::from_str(line).expect("json");
    let turn_id = v["turn_id"].as_str().expect("turn_id");
    assert!(!turn_id.is_empty());
    let status = Command::new(bin())
        .args([
            "worker",
            "status",
            "--json",
            "--home",
            home.path().to_str().unwrap(),
            "--turn-id",
            turn_id,
        ])
        .output()
        .expect("status");
    assert!(status.status.success());
    let snap: Value = serde_json::from_str(&String::from_utf8_lossy(&status.stdout)).unwrap();
    assert_eq!(snap["status"], "completed");
}

#[test]
fn wait_timeout_does_not_cancel() {
    let home = tempfile::tempdir().expect("home");
    let ledger = Ledger::open(home.path().to_path_buf()).expect("ledger");
    let thread = ledger
        .create_thread(&NewThread {
            cwd: "/tmp".into(),
            model: "test".into(),
            sandbox: ThreadSandbox::WorkspaceWrite,
            approval_policy: ApprovalPolicy::Never,
            developer_instructions: String::new(),
            acp_session_id: String::new(),
        })
        .unwrap();
    let turn = ledger
        .admit_turn(&NewTurn {
            thread_id: thread.id,
            input: vec![UserInput {
                kind: "text".into(),
                text: "hold".into(),
            }],
            model: "test".into(),
            effort: String::new(),
            client_request_id: None,
        })
        .unwrap();
    let started = Instant::now();
    let out = Command::new(bin())
        .args([
            "worker",
            "wait",
            "--json",
            "--home",
            home.path().to_str().unwrap(),
            "--turn-id",
            &turn.id,
            "--timeout-ms",
            "50",
        ])
        .output()
        .expect("wait");
    assert!(
        out.status.success(),
        "stderr {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(started.elapsed().as_millis() < 5_000, "wait should return");
    let snap: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(snap["status"], "inProgress");
    let still = ledger.observe(&turn.id).unwrap();
    assert_eq!(still.status, TurnStatus::InProgress);
}

#[test]
fn list_and_result_json() {
    let home = tempfile::tempdir().expect("home");
    let list = Command::new(bin())
        .args([
            "worker",
            "list",
            "--json",
            "--home",
            home.path().to_str().unwrap(),
        ])
        .output()
        .expect("list");
    assert!(list.status.success());
    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&list.stdout)).unwrap();
    assert!(v["turns"].as_array().unwrap().is_empty());
    let _ = Path::new(home.path());
}
