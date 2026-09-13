//! Drive the adapter turn against the fake ACP agent. Does not spawn Grok.

use samchi_adapter_grok::{run_turn, run_turn_on_admit, AgentCommand, TurnRequest};
use samchi_core::ledger::{Ledger, NewThread};
use samchi_core::source_wire::{
    ApprovalPolicy, ThreadSandbox, TurnStatus, ITEM_AGENT_MESSAGE, ITEM_FILE_CHANGE,
    ITEM_USER_MESSAGE,
};
use std::path::PathBuf;
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
            effort: String::new(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: None,
            reuse_thread_id: None,
        },
    )
    .unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(outcome.turn.status, TurnStatus::Completed);
    assert_eq!(outcome.turn.stop_reason, "end_turn");
    assert_eq!(outcome.turn.model, "test");
    assert_eq!(outcome.turn.effort, "high");
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
            effort: String::new(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: None,
            reuse_thread_id: None,
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
            effort: String::new(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: None,
            reuse_thread_id: None,
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
            effort: String::new(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: Some(first.turn.thread_id.clone()),
            reuse_thread_id: None,
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

#[test]
fn reuse_thread_id_admits_session_new_on_existing_thread() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let thread = ledger
        .create_thread(&NewThread {
            cwd: cwd.path().display().to_string(),
            model: "test".to_string(),
            sandbox: ThreadSandbox::ReadOnly,
            approval_policy: ApprovalPolicy::Untrusted,
            developer_instructions: String::new(),
            acp_session_id: String::new(),
        })
        .unwrap();
    let outcome = run_turn(
        ledger.clone(),
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "ping".to_string(),
            approval: ApprovalPolicy::Untrusted,
            sandbox: ThreadSandbox::ReadOnly,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            effort: String::new(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: None,
            reuse_thread_id: Some(thread.id.clone()),
        },
    )
    .unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(outcome.turn.thread_id, thread.id);
    let stored = ledger.read_thread(&thread.id).unwrap();
    assert_eq!(stored.acp_session_id, samchi_adapter_grok::STUB_SESSION_ID);
}

#[test]
fn reuse_thread_id_refuses_model_mismatch_before_first_turn() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let thread = ledger
        .create_thread(&NewThread {
            cwd: cwd.path().display().to_string(),
            model: "grok-4.6".to_string(),
            sandbox: ThreadSandbox::ReadOnly,
            approval_policy: ApprovalPolicy::Untrusted,
            developer_instructions: String::new(),
            acp_session_id: String::new(),
        })
        .unwrap();
    let err = run_turn(
        ledger.clone(),
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "ping".to_string(),
            approval: ApprovalPolicy::Untrusted,
            sandbox: ThreadSandbox::ReadOnly,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "other".to_string(),
            effort: String::new(),
            command: AgentCommand::Override {
                program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: None,
            reuse_thread_id: Some(thread.id.clone()),
        },
    )
    .expect_err("model lock");
    assert!(err.to_string().contains("INVALID_CONFIG"), "{err}");
    assert!(ledger.list_turns(None).unwrap().is_empty());
    let stored = ledger.read_thread(&thread.id).unwrap();
    assert_eq!(stored.model, "grok-4.6");
    assert!(stored.acp_session_id.is_empty());
}

#[test]
fn spawn_failure_after_admit_publishes_failed() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let thread = ledger
        .create_thread(&NewThread {
            cwd: cwd.path().display().to_string(),
            model: "test".to_string(),
            sandbox: ThreadSandbox::ReadOnly,
            approval_policy: ApprovalPolicy::Untrusted,
            developer_instructions: String::new(),
            acp_session_id: String::new(),
        })
        .unwrap();
    let err = run_turn(
        ledger.clone(),
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "ping".to_string(),
            approval: ApprovalPolicy::Untrusted,
            sandbox: ThreadSandbox::ReadOnly,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            effort: String::new(),
            command: AgentCommand::Override {
                program: PathBuf::from("/no/such/samchi-acp-agent"),
                args: Vec::new(),
            },
            client_request_id: None,
            follow_up_thread_id: None,
            reuse_thread_id: Some(thread.id.clone()),
        },
    )
    .expect_err("missing agent");
    let _ = err;
    let turns = ledger.list_turns(None).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].status, TurnStatus::Failed);
    assert_eq!(turns[0].thread_id, thread.id);
}

fn fake_command() -> AgentCommand {
    AgentCommand::Override {
        program: env!("CARGO_BIN_EXE_fake-acp-agent").into(),
        args: Vec::new(),
    }
}

fn thread_with_instructions(ledger: &Ledger, cwd: &std::path::Path, text: &str) -> String {
    ledger
        .create_thread(&NewThread {
            cwd: cwd.display().to_string(),
            model: "test".to_string(),
            sandbox: ThreadSandbox::WorkspaceWrite,
            approval_policy: ApprovalPolicy::Never,
            developer_instructions: text.to_string(),
            acp_session_id: String::new(),
        })
        .unwrap()
        .id
}

fn reuse_turn(
    ledger: Arc<Ledger>,
    cwd: &std::path::Path,
    thread_id: &str,
) -> samchi_adapter_grok::TurnOutcome {
    run_turn(
        ledger,
        &TurnRequest {
            cwd: cwd.to_path_buf(),
            prompt: "ping".to_string(),
            approval: ApprovalPolicy::Never,
            sandbox: ThreadSandbox::WorkspaceWrite,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            effort: String::new(),
            command: fake_command(),
            client_request_id: None,
            follow_up_thread_id: None,
            reuse_thread_id: Some(thread_id.to_string()),
        },
    )
    .unwrap_or_else(|err| panic!("{err}"))
}

fn session_log(cwd: &std::path::Path) -> PathBuf {
    cwd.join(".gamchi-fake-session-new.jsonl")
}

#[test]
fn session_new_installs_meta_rules_and_omits_yolo_mode() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let thread_id = thread_with_instructions(&ledger, cwd.path(), "be a reviewer");
    let outcome = reuse_turn(ledger.clone(), cwd.path(), &thread_id);
    assert_eq!(outcome.turn.status, TurnStatus::Completed);
    let stored = ledger.read_thread(&thread_id).unwrap();
    assert_eq!(stored.acp_session_id, samchi_adapter_grok::STUB_SESSION_ID);
    let body = std::fs::read_to_string(session_log(cwd.path())).expect("session log");
    let line: serde_json::Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert_eq!(line["rules"], "be a reviewer");
    assert!(line.get("yoloMode").is_none());
}

#[test]
fn empty_instructions_omit_meta_rules() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let thread_id = thread_with_instructions(&ledger, cwd.path(), "");
    reuse_turn(ledger, cwd.path(), &thread_id);
    let body = std::fs::read_to_string(session_log(cwd.path())).expect("session log");
    let line: serde_json::Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert!(line["rules"].is_null());
}

#[test]
fn two_threads_same_cwd_do_not_leak_rules() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let a = thread_with_instructions(&ledger, cwd.path(), "role A");
    let b = thread_with_instructions(&ledger, cwd.path(), "role B");
    reuse_turn(ledger.clone(), cwd.path(), &a);
    reuse_turn(ledger, cwd.path(), &b);
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(session_log(cwd.path()))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["rules"], "role A");
    assert_eq!(lines[1]["rules"], "role B");
}

#[test]
fn follow_up_load_does_not_resend_rules() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let thread_id = thread_with_instructions(&ledger, cwd.path(), "frozen");
    reuse_turn(ledger.clone(), cwd.path(), &thread_id);
    run_turn(
        ledger,
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "two".to_string(),
            approval: ApprovalPolicy::Never,
            sandbox: ThreadSandbox::WorkspaceWrite,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            effort: String::new(),
            command: fake_command(),
            client_request_id: None,
            follow_up_thread_id: Some(thread_id),
            reuse_thread_id: None,
        },
    )
    .unwrap_or_else(|err| panic!("{err}"));
    let n = std::fs::read_to_string(session_log(cwd.path()))
        .unwrap()
        .lines()
        .count();
    assert_eq!(n, 1, "session/load must not write another session/new");
}

#[test]
fn session_id_persist_failure_stops_before_prompt() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    std::fs::write(cwd.path().join(".gamchi-fail-set-acp-session-id"), b"").unwrap();
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let thread_id = thread_with_instructions(&ledger, cwd.path(), "frozen");
    let err = run_turn(
        ledger.clone(),
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt: "ping".to_string(),
            approval: ApprovalPolicy::Never,
            sandbox: ThreadSandbox::WorkspaceWrite,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "test".to_string(),
            effort: String::new(),
            command: fake_command(),
            client_request_id: None,
            follow_up_thread_id: None,
            reuse_thread_id: Some(thread_id.clone()),
        },
    )
    .expect_err("persist failure");
    let _ = err;
    let stored = ledger.read_thread(&thread_id).unwrap();
    assert!(stored.acp_session_id.is_empty());
    let turns = ledger.list_turns(None).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].status, TurnStatus::Failed);
    assert!(turns[0].failure_reason.contains("persistence failed"));
}
