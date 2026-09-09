//! Live proof that requested model/effort land on the Grok child argv.
//!
//! Ignored so `make test` stays offline. Run:
//! `cargo test -p samchi-adapter-grok --test live_model -- --ignored --nocapture`

use samchi_adapter_grok::{run_turn, AgentCommand, TurnRequest};
use samchi_core::ledger::{Ledger, NewTurn};
use samchi_core::source_wire::{ApprovalPolicy, ThreadSandbox, TurnStatus, UserInput};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

#[test]
#[ignore = "spawns live grok agent stdio; not part of make test"]
fn live_requested_pair_is_on_child_argv() {
    let grok = which_grok();
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    init_git(cwd.path());
    let argv_log = home.path().join("argv.log");
    let wrapper = write_wrapper(home.path(), &grok, &argv_log);
    let ledger = Arc::new(Ledger::open(home.path().to_path_buf()).expect("ledger"));

    let first = run_live(
        ledger.clone(),
        cwd.path(),
        &wrapper,
        "Reply with exactly one word: pong. Do not edit files.",
        "grok-4.6",
        "low",
        None,
    );
    assert_eq!(first.turn.status, TurnStatus::Completed, "{first:?}");
    assert_argv(&argv_log, "grok-4.6", "low");
    assert_eq!(first.turn.model, "grok-4.6");
    assert_eq!(first.turn.effort, "low");

    let second = run_live(
        ledger.clone(),
        cwd.path(),
        &wrapper,
        "Reply with exactly one word: pong.",
        "",
        "high",
        Some(first.turn.thread_id.clone()),
    );
    assert_eq!(second.turn.status, TurnStatus::Completed, "{second:?}");
    assert_argv(&argv_log, "grok-4.6", "high");
    assert_eq!(second.turn.effort, "high");

    let mismatch = run_turn(
        ledger.clone(),
        &req(
            cwd.path(),
            &wrapper,
            "should not start",
            "other",
            "",
            Some(first.turn.thread_id.clone()),
        ),
    );
    let err = mismatch.expect_err("model lock");
    let msg = err.to_string();
    assert!(msg.contains("INVALID_CONFIG"), "{msg}");

    let empty_effort = ledger
        .admit_turn(&NewTurn {
            thread_id: first.turn.thread_id.clone(),
            input: vec![UserInput {
                kind: "text".to_string(),
                text: "history".to_string(),
            }],
            model: "grok-4.6".to_string(),
            effort: String::new(),
            client_request_id: None,
        })
        .expect("empty effort turn");
    ledger
        .publish_terminal(&empty_effort.id, TurnStatus::Completed, "end_turn", "")
        .expect("publish empty");
    let third = run_live(
        ledger,
        cwd.path(),
        &wrapper,
        "Reply with exactly one word: pong.",
        "",
        "",
        Some(first.turn.thread_id),
    );
    assert_eq!(third.turn.status, TurnStatus::Completed, "{third:?}");
    assert_argv(&argv_log, "grok-4.6", "high");
    assert_eq!(third.turn.effort, "high");
}

fn run_live(
    ledger: Arc<Ledger>,
    cwd: &Path,
    wrapper: &Path,
    prompt: &str,
    model: &str,
    effort: &str,
    follow_up: Option<String>,
) -> samchi_adapter_grok::TurnOutcome {
    run_turn(ledger, &req(cwd, wrapper, prompt, model, effort, follow_up))
        .unwrap_or_else(|err| panic!("live turn failed: {err}"))
}

fn req(
    cwd: &Path,
    wrapper: &Path,
    prompt: &str,
    model: &str,
    effort: &str,
    follow_up: Option<String>,
) -> TurnRequest {
    TurnRequest {
        cwd: cwd.to_path_buf(),
        prompt: prompt.to_string(),
        approval: ApprovalPolicy::Never,
        sandbox: ThreadSandbox::WorkspaceWrite,
        extra: samchi_adapter_grok::ExtraSpawnFields::default(),
        model: model.to_string(),
        effort: effort.to_string(),
        command: AgentCommand::Grok {
            program: wrapper.to_path_buf(),
        },
        client_request_id: None,
        follow_up_thread_id: follow_up,
        reuse_thread_id: None,
    }
}

fn which_grok() -> PathBuf {
    let out = Command::new("which")
        .arg("grok")
        .output()
        .expect("which grok");
    assert!(
        out.status.success(),
        "live grok is required; unauthenticated or missing grok is Blocked"
    );
    PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
}

fn write_wrapper(home: &Path, grok: &Path, argv_log: &Path) -> PathBuf {
    let path = home.join("grok-argv-wrapper");
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{log}\"\nexec \"{grok}\" \"$@\"\n",
        log = argv_log.display(),
        grok = grok.display()
    );
    fs::write(&path, body).expect("wrapper");
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&path, perm).unwrap();
    path
}

fn assert_argv(log: &Path, model: &str, effort: &str) {
    let body = fs::read_to_string(log).unwrap_or_else(|err| panic!("argv log: {err}"));
    let lines: Vec<&str> = body.lines().collect();
    assert!(
        lines.windows(2).any(|w| w == ["-m", model]),
        "missing -m {model} in {body:?}"
    );
    assert!(
        lines
            .windows(2)
            .any(|w| w == ["--reasoning-effort", effort]),
        "missing --reasoning-effort {effort} in {body:?}"
    );
}

fn init_git(cwd: &Path) {
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(cwd)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args([
            "-c",
            "user.email=task027@example.test",
            "-c",
            "user.name=task027",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(cwd)
        .status()
        .unwrap()
        .success());
}
