//! Live MCP spawn→await that edits a named file. Optional Claude/Codex parents.
//!
//! Ignored so `make test` stays offline. Run:
//! `cargo test -p gamchi --test live_host -- --ignored --nocapture`
//!
//! Does not edit user host config. Each case uses a disposable git cwd and home.

use samchi_core::ledger::Ledger;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

const MARKER: &str = "gamchi-task-010-live-edit";
const MCP_FILE: &str = "TASK010_LIVE.txt";
const CLAUDE_FILE: &str = "TASK010_CLAUDE.txt";
const CODEX_FILE: &str = "TASK010_CODEX.txt";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_gamchi")
}

fn init_git(cwd: &Path) {
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(cwd)
        .status()
        .expect("git init")
        .success());
    assert!(Command::new("git")
        .args([
            "-c",
            "user.email=task010@example.test",
            "-c",
            "user.name=task010",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(cwd)
        .status()
        .expect("git commit")
        .success());
}

fn prompt_for(file: &str) -> String {
    format!(
        "Create a file named {file} in the current workspace. Write exactly this one line and nothing else: {MARKER}"
    )
}

struct Rpc {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
    home: PathBuf,
}

impl Rpc {
    fn start(home: &str) -> Self {
        let mut child = Command::new(bin())
            .args(["mcp", "--home", home])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            home: PathBuf::from(home),
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let req = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        writeln!(self.stdin, "{req}").unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|err| panic!("{err}: {line}"))
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let resp = self.call("tools/call", json!({"name": name, "arguments": arguments}));
        let text = resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("tool text {resp}"));
        serde_json::from_str(text).unwrap_or_else(|err| panic!("{err}: {text}"))
    }
}

impl Drop for Rpc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Ok(ledger) = Ledger::open(self.home.clone()) {
            if let Ok(turns) = ledger.list_turns(None) {
                for turn in turns {
                    if let Ok(generation) = ledger.read_generation(&turn.generation_id) {
                        if let Some(pid) = generation.child_pid {
                            samchi_adapter_grok::teardown_process_group(pid);
                        }
                    }
                }
            }
        }
    }
}

fn assert_marker(path: &Path) {
    let written =
        fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    assert!(
        written.contains(MARKER),
        "{} missing marker, got {written:?}",
        path.display()
    );
}

fn assert_host_used_spawn(home: &Path, cwd: &Path) {
    let ledger = Ledger::open(home.to_path_buf()).expect("ledger");
    let cwd_s = cwd.to_str().unwrap();
    let turns = ledger.list_turns(Some(cwd_s)).expect("list");
    assert!(
        turns.iter().any(|t| t.status.is_terminal()),
        "host did not spawn a gamchi turn for {cwd_s}: {turns:?}"
    );
}

#[test]
#[ignore = "spawns live grok via MCP; not part of make test"]
fn live_mcp_spawn_await_edits_named_file() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    init_git(cwd.path());
    let mut rpc = Rpc::start(home.path().to_str().unwrap());
    let _ = rpc.call(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"task010","version":"0"}}),
    );
    let spawn = rpc.tool(
        "grok_spawn",
        json!({
            "prompt": prompt_for(MCP_FILE),
            "cwd": cwd.path().to_str().unwrap()
        }),
    );
    let turn_id = spawn["turn_id"].as_str().expect("turn_id").to_string();
    assert!(!turn_id.is_empty(), "{spawn}");
    let started = Instant::now();
    let done = rpc.tool("grok_await", json!({"turn_id": turn_id}));
    assert_eq!(
        done["status"],
        "completed",
        "await {done} after {:?}",
        started.elapsed()
    );
    assert_marker(&cwd.path().join(MCP_FILE));
}

fn which(name: &str) -> Option<PathBuf> {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
}

fn write_mcp_config(path: &Path, home: &Path) {
    let cfg = json!({
        "mcpServers": {
            "gamchi": {
                "command": bin(),
                "args": ["mcp", "--home", home.to_str().unwrap()]
            }
        }
    });
    fs::write(path, serde_json::to_vec_pretty(&cfg).unwrap()).expect("mcp json");
}

fn skill_text() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/use-gamchi/SKILL.md");
    fs::read_to_string(p).expect("skill")
}

fn host_prompt(file: &str, cwd: &Path) -> String {
    format!(
        "You are supervising Grok through gamchi MCP. \
Call grok_spawn exactly once with prompt={:?} and cwd={}. \
Then call grok_await with the returned turn_id and wait until terminal. \
Do not write {file} yourself. Do not poll grok_status. \
If grok_await returns because of a host timeout, call grok_await again on the same turn_id. \
Do not treat that timeout as grok_cancel.",
        prompt_for(file),
        cwd.display()
    )
}

#[test]
#[ignore = "drives Claude Code as MCP host against live grok; not part of make test"]
fn live_claude_spawn_await_edits_named_file() {
    let claude = which("claude").expect("claude CLI available");
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    init_git(cwd.path());
    let cfg = cwd.path().join("mcp-config.json");
    write_mcp_config(&cfg, home.path());
    let out = Command::new(claude)
        .current_dir(cwd.path())
        .args([
            "-p",
            "--strict-mcp-config",
            "--mcp-config",
            cfg.to_str().unwrap(),
            "--dangerously-skip-permissions",
            "--append-system-prompt",
            &skill_text(),
            "--output-format",
            "text",
            &host_prompt(CLAUDE_FILE, cwd.path()),
        ])
        .output()
        .expect("claude");
    assert!(
        out.status.success(),
        "claude exit {} stderr {} stdout {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    assert_host_used_spawn(home.path(), cwd.path());
    assert_marker(&cwd.path().join(CLAUDE_FILE));
}

#[test]
#[ignore = "drives Codex as MCP host against live grok; not part of make test"]
fn live_codex_spawn_await_edits_named_file() {
    let codex = which("codex").expect("codex CLI available");
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    init_git(cwd.path());
    let bin_s = bin();
    let home_s = home.path().to_str().unwrap();
    let args = format!("[\"mcp\", \"--home\", \"{home_s}\"]");
    let out = Command::new(codex)
        .current_dir(cwd.path())
        .args([
            "exec",
            "--skip-git-repo-check",
            "--dangerously-bypass-approvals-and-sandbox",
            "-C",
            cwd.path().to_str().unwrap(),
            "-c",
            &format!("mcp_servers.gamchi.command={bin_s:?}"),
            "-c",
            &format!("mcp_servers.gamchi.args={args}"),
            "-c",
            "mcp_servers.gamchi.tool_timeout_sec=3600",
            "-c",
            "mcp_servers.gamchi.required=true",
            &host_prompt(CODEX_FILE, cwd.path()),
        ])
        .output()
        .expect("codex");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "codex exit {} stderr {stderr} stdout {stdout}",
        out.status
    );
    let ledger = Ledger::open(home.path().to_path_buf()).expect("ledger");
    let cwd_s = cwd.path().to_str().unwrap();
    let turns = ledger.list_turns(Some(cwd_s)).expect("list");
    assert!(
        turns.iter().any(|t| t.status.is_terminal()),
        "host did not spawn a gamchi turn for {cwd_s}: {turns:?}\nstdout {stdout}\nstderr {stderr}"
    );
    assert_marker(&cwd.path().join(CODEX_FILE));
}
