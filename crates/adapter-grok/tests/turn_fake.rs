//! Drive the adapter turn against the fake ACP agent. Does not spawn Grok.

use samchi_adapter_grok::{run_turn, run_turn_on_admit, AgentCommand, TurnRequest};
use samchi_core::ledger::Ledger;
use samchi_core::source_wire::{
    ApprovalPolicy, ThreadSandbox, TurnStatus, ITEM_AGENT_MESSAGE, ITEM_FILE_CHANGE,
    ITEM_USER_MESSAGE,
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
            follow_up_thread_id: None,
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
    let thread = ledger.read_thread(&outcome.turn.thread_id).expect("thread");
    assert_eq!(thread.acp_session_id, samchi_adapter_grok::STUB_SESSION_ID);
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
            follow_up_thread_id: None,
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

#[test]
fn follow_up_loads_same_session_without_duplicating_history() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let first = run_turn(
        ledger.clone(),
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "one".to_string(),
            approval: ApprovalPolicy::Never,
            sandbox: ThreadSandbox::WorkspaceWrite,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: None,
        },
    )
    .unwrap_or_else(|err| panic!("{err}"));
    let first_items = first.turn.items.len();
    let second = run_turn(
        ledger.clone(),
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "two".to_string(),
            approval: ApprovalPolicy::Never,
            sandbox: ThreadSandbox::WorkspaceWrite,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: Some(first.turn.thread_id.clone()),
        },
    )
    .unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(second.turn.thread_id, first.turn.thread_id);
    assert_ne!(second.turn.id, first.turn.id);
    assert_eq!(second.turn.status, TurnStatus::Completed);
    let thread = ledger.read_thread(&first.turn.thread_id).unwrap();
    assert_eq!(thread.acp_session_id, samchi_adapter_grok::STUB_SESSION_ID);
    assert_eq!(
        second.turn.items.len(),
        first_items,
        "load replay must not append history items onto the new turn"
    );
    assert!(second
        .turn
        .items
        .iter()
        .any(|item| item.item_type == ITEM_USER_MESSAGE && item.text == "two"));
}
