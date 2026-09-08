//! Live MCP spawn→await then grok_followup on the same ACP session.
//!
//! Ignored so `make test` stays offline. Run:
//! `cargo test -p samchi-for-grok --test live_followup -- --ignored --nocapture`
//!
//! Disposable git cwd and home. Does not edit user host config.

use samchi_core::ledger::Ledger;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_samchi-for-grok")
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
            "user.email=task012@example.test",
            "-c",
            "user.name=task012",
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
fn live_second_turn_reuses_acp_session() {
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
            "prompt": "Reply with exactly the word ping and do not use tools.",
            "cwd": cwd.path().to_str().unwrap()
        }),
    );
    let thread_id = spawn["thread_id"]
        .as_str()
        .unwrap_or_else(|| panic!("spawn {spawn}"))
        .to_string();
    let first = spawn["turn_id"].as_str().unwrap().to_string();
    let done = rpc.tool("grok_await", json!({"turn_id": first}));
    assert_eq!(
        done["status"], "completed",
        "first turn {done} failure_reason={}",
        done["failure_reason"]
    );
    let follow = rpc.tool(
        "grok_followup",
        json!({
            "thread_id": thread_id,
            "prompt": "Reply with exactly the word pong and do not use tools."
        }),
    );
    assert_eq!(follow["thread_id"], thread_id);
    let second = follow["turn_id"].as_str().unwrap().to_string();
    assert_ne!(second, first);
    let done2 = rpc.tool("grok_await", json!({"turn_id": second}));
    assert_eq!(
        done2["status"], "completed",
        "follow-up {done2} failure_reason={}",
        done2["failure_reason"]
    );
    let ledger = Ledger::open(home.path().to_path_buf()).expect("ledger");
    let thread = ledger.read_thread(&thread_id).expect("thread");
    assert!(
        !thread.acp_session_id.is_empty(),
        "follow-up must keep the stored ACP session id"
    );
}
