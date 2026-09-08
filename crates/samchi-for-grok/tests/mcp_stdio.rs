//! MCP stdio tools against the fake ACP agent. Does not spawn live Grok.

use samchi_core::ledger::{Ledger, NewThread, NewTurn};
use samchi_core::source_wire::{ApprovalPolicy, ThreadSandbox, UserInput};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_samchi-for-grok")
}

fn fake_agent() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_BIN_EXE_samchi-for-grok"));
    p.set_file_name("fake-acp-agent");
    p
}

struct Rpc {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Rpc {
    fn start(home: &str) -> Self {
        Self::start_with(home, &[])
    }

    fn start_hang(home: &str) -> Self {
        Self::start_with(home, &[("SAMCHI_FOR_GROK_FAKE_HANG_SECS", "60")])
    }

    fn start_with(home: &str, extra: &[(&str, &str)]) -> Self {
        let mut cmd = Command::new(bin());
        cmd.args(["mcp", "--home", home])
            .env("SAMCHI_FOR_GROK_ACP_PROGRAM", fake_agent())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("mcp");
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
        serde_json::from_str(&line).unwrap()
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let resp = self.call("tools/call", json!({"name": name, "arguments": arguments}));
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    }
}

impl Drop for Rpc {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn tools_list_includes_cancel_and_spawn_await() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let mut rpc = Rpc::start(home.path().to_str().unwrap());
    let init = rpc.call(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "samchi-for-grok");
    let listed = rpc.call("tools/list", json!({}));
    let names: Vec<_> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "grok_spawn",
            "grok_await",
            "grok_wait",
            "grok_status",
            "grok_result",
            "grok_list",
            "grok_cancel",
            "grok_followup"
        ]
    );
    let spawn = rpc.tool(
        "grok_spawn",
        json!({"prompt":"ping","cwd": cwd.path().to_str().unwrap()}),
    );
    let turn_id = spawn["turn_id"].as_str().unwrap().to_string();
    assert!(!turn_id.is_empty());
    let done = rpc.tool("grok_await", json!({"turn_id": turn_id}));
    assert_eq!(
        done["status"], "completed",
        "await {done} failure_reason={}",
        done["failure_reason"]
    );
}

#[test]
fn untrusted_spawn_rejected() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let mut rpc = Rpc::start(home.path().to_str().unwrap());
    let _ = rpc.call(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}),
    );
    let resp = rpc.call(
        "tools/call",
        json!({
            "name": "grok_spawn",
            "arguments": {
                "prompt": "x",
                "cwd": cwd.path().to_str().unwrap(),
                "approvalPolicy": "untrusted"
            }
        }),
    );
    assert_eq!(resp["result"]["isError"], true);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("untrusted"), "{text}");
    let extra = rpc.call(
        "tools/call",
        json!({
            "name": "grok_spawn",
            "arguments": {
                "prompt": "x",
                "cwd": cwd.path().to_str().unwrap(),
                "writableRoots": ["/tmp"]
            }
        }),
    );
    assert_eq!(extra["result"]["isError"], true);
    let extra_text = extra["result"]["content"][0]["text"].as_str().unwrap();
    assert!(extra_text.contains("unenforceable"), "{extra_text}");
}

#[test]
fn wait_timeout_does_not_cancel() {
    let home = tempfile::tempdir().expect("home");
    let ledger = Ledger::open(home.path().to_path_buf()).unwrap();
    let thread = ledger
        .create_thread(&NewThread {
            cwd: "/tmp".into(),
            model: "t".into(),
            sandbox: ThreadSandbox::WorkspaceWrite,
            approval_policy: ApprovalPolicy::Never,
            developer_instructions: String::new(),
            acp_session_id: String::new(),
        })
        .unwrap();
    let turn = ledger
        .admit_turn(&NewTurn {
            thread_id: thread.id,
            input: vec![UserInput {
                kind: "text".into(),
                text: "hold".into(),
            }],
            model: "t".into(),
            effort: String::new(),
            client_request_id: None,
        })
        .unwrap();
    let mut rpc = Rpc::start(home.path().to_str().unwrap());
    let snap = rpc.tool("grok_wait", json!({"turn_id": turn.id, "timeout_ms": 50}));
    assert_eq!(snap["status"], "inProgress");
    let result = rpc.tool("grok_result", json!({"turn_id": turn.id}));
    assert_eq!(result["ready"], false);
    let still = ledger.observe(&turn.id).unwrap();
    assert!(!still.status.is_terminal());
}

#[test]
fn grok_cancel_interrupts_hanging_fake_agent() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let mut rpc = Rpc::start_hang(home.path().to_str().unwrap());
    let _ = rpc.call(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}),
    );
    let spawn = rpc.tool(
        "grok_spawn",
        json!({"prompt":"hang","cwd": cwd.path().to_str().unwrap()}),
    );
    let turn_id = spawn["turn_id"].as_str().unwrap().to_string();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let cancelled = rpc.tool("grok_cancel", json!({"turn_id": turn_id}));
    assert_eq!(
        cancelled["status"], "interrupted",
        "cancel {cancelled} failure_reason={}",
        cancelled["failure_reason"]
    );
    let again = rpc.tool("grok_cancel", json!({"turn_id": turn_id}));
    assert_eq!(again["status"], "interrupted");
    let done = rpc.tool("grok_await", json!({"turn_id": turn_id}));
    assert_eq!(done["status"], "interrupted");
}

#[test]
fn grok_followup_second_turn_same_session() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let mut rpc = Rpc::start(home.path().to_str().unwrap());
    let _ = rpc.call(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}),
    );
    let spawn = rpc.tool(
        "grok_spawn",
        json!({"prompt":"one","cwd": cwd.path().to_str().unwrap()}),
    );
    let thread_id = spawn["thread_id"].as_str().unwrap().to_string();
    let first = spawn["turn_id"].as_str().unwrap().to_string();
    let done = rpc.tool("grok_await", json!({"turn_id": first}));
    assert_eq!(done["status"], "completed");
    let follow = rpc.tool(
        "grok_followup",
        json!({"thread_id": thread_id, "prompt": "two"}),
    );
    assert_eq!(follow["thread_id"], thread_id);
    let second = follow["turn_id"].as_str().unwrap();
    assert_ne!(second, first);
    let done2 = rpc.tool("grok_await", json!({"turn_id": second}));
    assert_eq!(
        done2["status"], "completed",
        "follow-up {done2} failure_reason={}",
        done2["failure_reason"]
    );
}

#[test]
fn grok_followup_fails_closed_without_load_session() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let mut rpc = Rpc::start_with(
        home.path().to_str().unwrap(),
        &[("SAMCHI_FOR_GROK_FAKE_NO_LOAD", "1")],
    );
    let _ = rpc.call(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}),
    );
    let spawn = rpc.tool(
        "grok_spawn",
        json!({"prompt":"one","cwd": cwd.path().to_str().unwrap()}),
    );
    let thread_id = spawn["thread_id"].as_str().unwrap().to_string();
    let first = spawn["turn_id"].as_str().unwrap().to_string();
    assert_eq!(
        rpc.tool("grok_await", json!({"turn_id": first}))["status"],
        "completed"
    );
    let follow = rpc.tool(
        "grok_followup",
        json!({"thread_id": thread_id, "prompt": "two"}),
    );
    let second = follow["turn_id"].as_str().expect("admitted follow-up");
    let done = rpc.tool("grok_await", json!({"turn_id": second}));
    assert_eq!(done["status"], "failed", "follow-up {done}");
    let reason = done["failure_reason"].as_str().unwrap_or("");
    assert!(
        reason.contains("loadSession"),
        "expected loadSession fail-closed, got {done}"
    );
}
