//! Drive the adapter turn against the fake ACP agent. Does not spawn Grok.

use samchi_adapter_grok::{run_turn, run_turn_on_admit, AgentCommand, TurnRequest};
use samchi_core::ledger::Ledger;
use samchi_core::source_wire::{
    ApprovalPolicy, ThreadSandbox, TurnStatus, ITEM_AGENT_MESSAGE, ITEM_FILE_CHANGE,
};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn fake_agent_turn_publishes_items() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let outcome = run_turn(
        ledger.clone(),
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "ping".to_string(),
            approval: ApprovalPolicy::Never,
            sandbox: ThreadSandbox::WorkspaceWrite,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
        },
    )
    .unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(outcome.turn.status, TurnStatus::Completed);
    assert_eq!(outcome.turn.stop_reason, "end_turn");
    assert!(outcome
        .turn
        .items
        .iter()
        .any(|item| item.item_type == ITEM_AGENT_MESSAGE && item.status == "completed"));
    assert!(outcome
        .turn
        .items
        .iter()
        .any(|item| item.item_type == ITEM_FILE_CHANGE && item.status == "completed"));
    let stored = ledger.read_turn(&outcome.turn.id).expect("stored");
    assert_eq!(stored.status, TurnStatus::Completed);
}

#[test]
fn concurrent_wait_sees_completed_not_worker_gone() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let waiter_ledger = ledger.clone();
    let waiter = std::thread::spawn(move || {
        let id: String = rx.recv().expect("turn id");
        waiter_ledger
            .wait(&id, Some(Duration::from_secs(10)))
            .expect("wait")
    });
    let outcome = run_turn_on_admit(
        ledger.clone(),
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "ping".to_string(),
            approval: ApprovalPolicy::Never,
            sandbox: ThreadSandbox::WorkspaceWrite,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
        },
        |turn| {
            let _ = tx.send(turn.id.clone());
        },
    )
    .unwrap_or_else(|err| panic!("{err}"));
    let waited = waiter.join().expect("waiter");
    assert_eq!(outcome.turn.status, TurnStatus::Completed);
    assert_eq!(waited.status, TurnStatus::Completed);
    assert_eq!(waited.failure_reason, "");
}
