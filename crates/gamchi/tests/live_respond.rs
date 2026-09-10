//! Live MCP untrusted spawn then grok_respond. A gated shell does not run
//! without respond; the parent responds then awaits again.
//!
//! Ignored so `make test` stays offline. Run:
//! `cargo test -p gamchi --test live_respond -- --ignored --nocapture`
//!
//! Disposable git cwd and home. Does not edit user host config.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

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
            "user.email=task013@example.test",
            "-c",
            "user.name=task013",
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
fn live_untrusted_waits_for_respond_then_completes() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    init_git(cwd.path());
    let marker = cwd.path().join("marker.txt");
    let outside = std::env::temp_dir().join(format!("samchi-task013-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&outside);
    let mut rpc = Rpc::start(home.path().to_str().unwrap());
    let _ = rpc.call(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}),
    );
    let spawn = rpc.tool(
        "grok_spawn",
        json!({
            "prompt": format!(
                "Run exactly this shell command and no other: printf ok > {}. Do not skip the command.",
                outside.display()
            ),
            "cwd": cwd.path().to_str().unwrap(),
            "approvalPolicy": "untrusted"
        }),
    );
    let turn_id = spawn["turn_id"]
        .as_str()
        .unwrap_or_else(|| panic!("spawn {spawn}"))
        .to_string();
    let first = rpc.tool("grok_await", json!({"turn_id": turn_id}));
    assert_eq!(
        first["await_reason"], "pending_approval",
        "untrusted must pause before the gated shell: {first}"
    );
    assert_eq!(first["status"], "inProgress");
    assert!(
        !marker.exists() && !outside.exists(),
        "gated shell must not run before grok_respond"
    );
    let request_id = first["request_id"]
        .as_str()
        .unwrap_or_else(|| panic!("request_id {first}"));
    let _ = rpc.tool(
        "grok_respond",
        json!({"request_id": request_id, "decision": "accept"}),
    );
    let mut last = rpc.tool("grok_await", json!({"turn_id": turn_id}));
    for _ in 0..4 {
        if last["await_reason"] == "pending_approval" {
            let rid = last["request_id"].as_str().expect("request_id");
            let _ = rpc.tool(
                "grok_respond",
                json!({"request_id": rid, "decision": "accept"}),
            );
            last = rpc.tool("grok_await", json!({"turn_id": turn_id}));
            continue;
        }
        break;
    }
    assert!(
        last["status"] == "completed" || last["status"] == "interrupted",
        "respond then await {last} failure_reason={}",
        last["failure_reason"]
    );
    assert_ne!(last["status"].as_str(), Some("inProgress"));
    let _ = std::fs::remove_file(&outside);
}
