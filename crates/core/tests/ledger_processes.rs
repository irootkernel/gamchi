//! Inter-process ledger tests. Same-process threads share flock, so two hosts
//! and owner-pid death must be real OS processes.

use samchi_core::ledger::{Ledger, LedgerError, NewThread, NewTurn};
use samchi_core::source_wire::{
    ApprovalPolicy, ThreadSandbox, TurnStatus, UserInput, FAILURE_WORKER_GONE,
};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn sample_thread() -> NewThread {
    NewThread {
        cwd: "/work".to_string(),
        model: "grok".to_string(),
        sandbox: ThreadSandbox::WorkspaceWrite,
        approval_policy: ApprovalPolicy::Never,
        developer_instructions: String::new(),
        acp_session_id: String::new(),
    }
}

fn sample_turn(thread_id: &str) -> NewTurn {
    NewTurn {
        thread_id: thread_id.to_string(),
        input: vec![UserInput {
            kind: "text".to_string(),
            text: "do the work".to_string(),
        }],
        model: "grok".to_string(),
        effort: "low".to_string(),
        client_request_id: None,
    }
}

fn run_helper_if_requested() {
    let Ok(kind) = env::var("SAMCHI_LEDGER_HELPER") else {
        return;
    };
    let code = match kind.as_str() {
        "owner-die" => helper_owner_die(),
        "admit" => helper_admit(),
        other => {
            eprintln!("unknown helper {other}");
            2
        }
    };
    std::process::exit(code);
}

fn helper_owner_die() -> i32 {
    let home = env::var("SAMCHI_LEDGER_HOME").expect("SAMCHI_LEDGER_HOME");
    let out = env::var("SAMCHI_LEDGER_OUT").expect("SAMCHI_LEDGER_OUT");
    let ledger = Ledger::open(&home).expect("open");
    let thread = ledger.create_thread(&sample_thread()).expect("thread");
    let turn = ledger.admit_turn(&sample_turn(&thread.id)).expect("admit");
    fs::write(&out, &turn.id).expect("write");
    0
}

fn helper_admit() -> i32 {
    let home = env::var("SAMCHI_LEDGER_HOME").expect("SAMCHI_LEDGER_HOME");
    let thread_id = env::var("SAMCHI_LEDGER_THREAD").expect("SAMCHI_LEDGER_THREAD");
    let out = env::var("SAMCHI_LEDGER_OUT").expect("SAMCHI_LEDGER_OUT");
    let ledger = Ledger::open(&home).expect("open");
    match ledger.admit_turn(&sample_turn(&thread_id)) {
        Ok(turn) => {
            fs::write(&out, format!("ok:{}", turn.id)).expect("write");
            0
        }
        Err(LedgerError::TurnInProgress { turn_id, .. }) => {
            fs::write(&out, format!("in_progress:{turn_id}")).expect("write");
            1
        }
        Err(err) => {
            fs::write(&out, format!("error:{err}")).expect("write");
            2
        }
    }
}

fn spawn_helper(
    test_name: &str,
    home: &Path,
    helper: &str,
    out: &Path,
    thread_id: Option<&str>,
) -> std::process::Child {
    let mut cmd = Command::new(env::current_exe().expect("current_exe"));
    cmd.env("SAMCHI_LEDGER_HELPER", helper)
        .env("SAMCHI_LEDGER_HOME", home)
        .env("SAMCHI_LEDGER_OUT", out)
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture");
    if let Some(id) = thread_id {
        cmd.env("SAMCHI_LEDGER_THREAD", id);
    }
    cmd.spawn().expect("spawn helper")
}

#[test]
fn owner_death_converges_to_worker_gone() {
    run_helper_if_requested();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    Ledger::open(home).unwrap();
    let out = home.join("owner-die.json");
    let status = spawn_helper(
        "owner_death_converges_to_worker_gone",
        home,
        "owner-die",
        &out,
        None,
    )
    .wait()
    .unwrap();
    assert!(status.success(), "helper exit {status}");
    let turn_id = fs::read_to_string(&out).unwrap();
    let ledger = Ledger::open(home).unwrap();
    let inflight = ledger.read_turn(turn_id.trim()).unwrap();
    assert_eq!(inflight.status, TurnStatus::InProgress);
    let got = ledger
        .wait(turn_id.trim(), Some(Duration::from_secs(2)))
        .unwrap();
    assert_eq!(got.status, TurnStatus::Failed);
    assert_eq!(got.failure_reason, FAILURE_WORKER_GONE);
}

#[test]
fn two_hosts_cannot_double_admit() {
    run_helper_if_requested();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let ledger = Ledger::open(home).unwrap();
    let thread = ledger.create_thread(&sample_thread()).unwrap();
    let first = ledger.admit_turn(&sample_turn(&thread.id)).unwrap();
    let out = home.join("admit-second");
    let status = spawn_helper(
        "two_hosts_cannot_double_admit",
        home,
        "admit",
        &out,
        Some(&thread.id),
    )
    .wait()
    .unwrap();
    assert_eq!(
        status.code(),
        Some(1),
        "second host must hit TurnInProgress while the owner lives, out={:?}",
        fs::read_to_string(&out).ok()
    );
    let body = fs::read_to_string(&out).unwrap();
    assert!(
        body.starts_with("in_progress:") && body.contains(&first.id),
        "{body}"
    );
    let listed = ledger.list_turns(None).unwrap();
    assert_eq!(listed.len(), 1, "turns {listed:?}");
    assert_eq!(listed[0].id, first.id);
    assert_eq!(listed[0].status, TurnStatus::InProgress);
}
