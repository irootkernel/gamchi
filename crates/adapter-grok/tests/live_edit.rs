//! Live `grok agent stdio` turn that edits a named file.
//!
//! Ignored so `make test` stays offline. Run:
//! `cargo test -p samchi-adapter-grok --test live_edit -- --ignored --nocapture`

use samchi_adapter_grok::{run_turn, AgentCommand, TurnRequest};
use samchi_core::ledger::Ledger;
use samchi_core::source_wire::{ApprovalPolicy, ThreadSandbox, TurnStatus};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

const MARKER: &str = "samchi-for-grok-task-007-live-edit";
const FILE_NAME: &str = "TASK007_LIVE.txt";

#[test]
#[ignore = "spawns live grok agent stdio; not part of make test"]
fn live_turn_edits_named_file() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    init_git(cwd.path());
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));
    let prompt = format!(
        "Create a file named {FILE_NAME} in the current workspace. Write exactly this one line and nothing else: {MARKER}"
    );
    let outcome = run_turn(
        ledger,
        &TurnRequest {
            cwd: cwd.path().to_path_buf(),
            prompt,
            approval: ApprovalPolicy::Never,
            sandbox: ThreadSandbox::WorkspaceWrite,
            extra: samchi_adapter_grok::ExtraSpawnFields::default(),
            model: "grok-4.6".to_string(),
            command: AgentCommand::Grok {
                program: Path::new("grok").to_path_buf(),
            },
            client_request_id: None,
            follow_up_thread_id: None,
        },
    )
    .unwrap_or_else(|err| panic!("live turn failed: {err}"));
    assert_eq!(
        outcome.turn.status,
        TurnStatus::Completed,
        "stop={} failure={}",
        outcome.turn.stop_reason,
        outcome.turn.failure_reason
    );
    let written = fs::read_to_string(cwd.path().join(FILE_NAME))
        .unwrap_or_else(|err| panic!("expected {FILE_NAME}: {err}"));
    assert!(
        written.contains(MARKER),
        "live file missing marker, got {written:?}"
    );
}

fn init_git(cwd: &Path) {
    let status = Command::new("git")
        .args(["init"])
        .current_dir(cwd)
        .status()
        .expect("git init");
    assert!(status.success());
    let status = Command::new("git")
        .args([
            "-c",
            "user.email=task007@example.test",
            "-c",
            "user.name=task007",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(cwd)
        .status()
        .expect("git commit");
    assert!(status.success());
}
