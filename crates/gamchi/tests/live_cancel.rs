//! Live MCP spawn then grok_cancel. An in-flight prompt dies; the turn is interrupted.
//!
//! Ignored so `make test` stays offline. Run:
//! `cargo test -p gamchi --test live_cancel -- --ignored --nocapture`
//!
//! Disposable git cwd and home. Does not edit user host config.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

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
            "user.email=task011@example.test",
            "-c",
            "user.name=task011",
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

struct Rpc {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
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
    }
}

#[test]
#[ignore = "spawns live grok agent stdio; not part of make test"]
fn live_in_flight_prompt_dies_and_turn_is_interrupted() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    init_git(cwd.path());
    let mut rpc = Rpc::start(home.path().to_str().unwrap());
    let _ = rpc.call(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}),
    );
    let spawn = rpc.tool(
        "grok_spawn",
        json!({
            "prompt": "Count slowly from 1 to 100000 in the reply. Do not stop until you reach 100000. Do not use tools.",
            "cwd": cwd.path().to_str().unwrap()
        }),
    );
    let turn_id = spawn["turn_id"]
        .as_str()
        .unwrap_or_else(|| panic!("spawn {spawn}"))
        .to_string();
    thread::sleep(Duration::from_millis(400));
    let cancelled = rpc.tool("grok_cancel", json!({"turn_id": turn_id}));
    assert_eq!(
        cancelled["status"], "interrupted",
        "cancel {cancelled} failure_reason={}",
        cancelled["failure_reason"]
    );
    let done = rpc.tool("grok_await", json!({"turn_id": turn_id}));
    assert_eq!(
        done["status"], "interrupted",
        "await {done} failure_reason={}",
        done["failure_reason"]
    );
}
