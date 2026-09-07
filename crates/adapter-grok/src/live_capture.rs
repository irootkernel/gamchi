//! Offline parser for the TASK-004 live `grok agent stdio` capture.
//!
//! `make test` does not spawn Grok. These helpers read checked-in ACP NDJSON.

use crate::{STUB_REPLY, STUB_SESSION_ID, STUB_TOOL_CALL_ID};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Capture directory relative to this crate's manifest dir.
pub const CAPTURE_RELDIR: &str = "captures/task-004";
/// NDJSON envelopes (`direction` + JSON-RPC `message`).
pub const TRAFFIC_FILENAME: &str = "traffic.ndjson";
/// Launch argv, pinned CLI version, and owner note.
pub const METADATA_FILENAME: &str = "metadata.json";
/// Copy of the file written during the captured turn.
pub const WRITTEN_FILENAME: &str = "TASK004_CAPTURE.txt";

/// Protocol events extracted from the checked-in live capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveStdioCapture {
    /// `grok --version` string from the capture machine.
    pub grok_cli_version: String,
    /// ACP `initialize` result `_meta.agentVersion`.
    pub grok_agent_version: String,
    /// Launch argv recorded in metadata.
    pub argv: Vec<String>,
    /// Who owns the Grok child (must mention `--no-leader` or an owner).
    pub owner: String,
    /// Client sent ACP `initialize`.
    pub initialize_request: bool,
    /// Agent returned an `initialize` result.
    pub initialize_result: bool,
    /// Client sent `session/new`.
    pub session_new: bool,
    /// Client sent `session/prompt`.
    pub session_prompt: bool,
    /// Agent sent at least one `session/update`.
    pub session_update: bool,
    /// A tool call (or update) had ACP `kind` `edit`.
    pub edit_kind: bool,
    /// Agent issued `fs/write_text_file` (client wrote the bytes).
    pub fs_write_text_file: bool,
    /// Bytes of the written file copy.
    pub written_content: String,
    /// `session/prompt` result `stopReason`.
    pub stop_reason: String,
    /// Traffic contains TASK-003 fake-agent stub ids.
    pub contains_fake_stub_ids: bool,
}

impl LiveStdioCapture {
    /// True when argv includes `--no-leader` and an owner note is present.
    pub fn launched_with_no_leader(&self) -> bool {
        self.argv.iter().any(|a| a == "--no-leader") && !self.owner.is_empty()
    }
}

/// `$CARGO_MANIFEST_DIR/captures/task-004`.
pub fn capture_dir(manifest_dir: impl AsRef<Path>) -> PathBuf {
    manifest_dir.as_ref().join(CAPTURE_RELDIR)
}

/// Load metadata, traffic, and the written-file copy from `dir`.
pub fn parse_capture_dir(dir: impl AsRef<Path>) -> Result<LiveStdioCapture, String> {
    let dir = dir.as_ref();
    let meta_raw = read_to_string(dir.join(METADATA_FILENAME))?;
    let traffic_raw = read_to_string(dir.join(TRAFFIC_FILENAME))?;
    let written = read_to_string(dir.join(WRITTEN_FILENAME))?;
    parse_capture_parts(&meta_raw, &traffic_raw, &written)
}

/// Parse already-loaded capture bytes. Used by tests and `parse_capture_dir`.
pub fn parse_capture_parts(
    metadata_json: &str,
    traffic_ndjson: &str,
    written_file: &str,
) -> Result<LiveStdioCapture, String> {
    let meta: Value =
        serde_json::from_str(metadata_json).map_err(|err| format!("metadata.json: {err}"))?;
    let grok_cli_version = meta_string(&meta, "grok_version")?;
    let argv = meta_string_array(&meta, "argv")?;
    let owner = meta_string(&meta, "owner")?;
    let meta_agent_version = meta_string(&meta, "agent_version")?;
    let meta_stop = meta_string(&meta, "stop_reason")?;
    let meta_marker = meta_string(&meta, "written_marker")?;

    let mut initialize_request = false;
    let mut initialize_result = false;
    let mut session_new = false;
    let mut session_prompt = false;
    let mut session_update = false;
    let mut edit_kind = false;
    let mut fs_write_text_file = false;
    let mut grok_agent_version = String::new();
    let mut stop_reason = String::new();
    let mut init_ids = Vec::new();
    let mut prompt_ids = Vec::new();

    for (i, line) in traffic_ndjson.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let env: Value = serde_json::from_str(line)
            .map_err(|err| format!("traffic.ndjson line {}: {err}", i + 1))?;
        let direction = env
            .get("direction")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("traffic.ndjson line {}: missing direction", i + 1))?;
        let message = env
            .get("message")
            .ok_or_else(|| format!("traffic.ndjson line {}: missing message", i + 1))?;
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id").cloned();

        match (direction, method) {
            ("client->agent", Some("initialize")) => {
                initialize_request = true;
                if let Some(id) = id {
                    init_ids.push(id);
                }
            }
            ("client->agent", Some("session/new")) => session_new = true,
            ("client->agent", Some("session/prompt")) => {
                session_prompt = true;
                if let Some(id) = id {
                    prompt_ids.push(id);
                }
            }
            ("agent->client", Some("session/update")) => session_update = true,
            ("agent->client", Some("fs/write_text_file")) => fs_write_text_file = true,
            _ => {}
        }

        if direction == "agent->client" && method.is_none() {
            if let Some(result) = message.get("result") {
                if init_ids.iter().any(|want| message.get("id") == Some(want)) {
                    initialize_result = true;
                    if let Some(v) = result
                        .pointer("/_meta/agentVersion")
                        .and_then(Value::as_str)
                    {
                        grok_agent_version = v.to_string();
                    }
                }
                if prompt_ids
                    .iter()
                    .any(|want| message.get("id") == Some(want))
                {
                    if let Some(v) = result.get("stopReason").and_then(Value::as_str) {
                        stop_reason = v.to_string();
                    }
                }
            }
        }

        if json_has_edit_kind(message) {
            edit_kind = true;
        }
    }

    if grok_agent_version.is_empty() {
        grok_agent_version = meta_agent_version.clone();
    }
    if grok_agent_version != meta_agent_version {
        return Err(format!(
            "agentVersion {grok_agent_version:?} != metadata {meta_agent_version:?}"
        ));
    }
    if stop_reason.is_empty() {
        stop_reason = meta_stop.clone();
    }
    if stop_reason != meta_stop {
        return Err(format!(
            "stopReason {stop_reason:?} != metadata {meta_stop:?}"
        ));
    }
    if !written_file.contains(&meta_marker) {
        return Err(format!(
            "written file does not contain marker {meta_marker:?}"
        ));
    }

    let blob = format!("{metadata_json}\n{traffic_ndjson}\n{written_file}");
    let contains_fake_stub_ids = blob.contains(STUB_SESSION_ID)
        || blob.contains(STUB_REPLY)
        || blob.contains(STUB_TOOL_CALL_ID);

    Ok(LiveStdioCapture {
        grok_cli_version,
        grok_agent_version,
        argv,
        owner,
        initialize_request,
        initialize_result,
        session_new,
        session_prompt,
        session_update,
        edit_kind,
        fs_write_text_file,
        written_content: written_file.to_string(),
        stop_reason,
        contains_fake_stub_ids,
    })
}

fn read_to_string(path: PathBuf) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))
}

fn meta_string(meta: &Value, key: &str) -> Result<String, String> {
    meta.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("metadata.json missing string {key}"))
}

fn meta_string_array(meta: &Value, key: &str) -> Result<Vec<String>, String> {
    let arr = meta
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("metadata.json missing array {key}"))?;
    arr.iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("metadata.json {key} entry is not a string"))
        })
        .collect()
}

fn json_has_edit_kind(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("kind").and_then(Value::as_str) == Some("edit")
                || map.values().any(json_has_edit_kind)
        }
        Value::Array(items) => items.iter().any(json_has_edit_kind),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_traffic(agent_version: &str, stop: &str, kind: &str) -> String {
        let init_req = serde_json::json!({
            "direction": "client->agent",
            "message": {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}
        });
        let init_res = serde_json::json!({
            "direction": "agent->client",
            "message": {
                "jsonrpc":"2.0",
                "id":1,
                "result":{"protocolVersion":1,"_meta":{"agentVersion":agent_version}}
            }
        });
        let new_req = serde_json::json!({
            "direction": "client->agent",
            "message": {"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":"/CAPTURE_CWD","mcpServers":[]}}
        });
        let prompt_req = serde_json::json!({
            "direction": "client->agent",
            "message": {"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{"sessionId":"live","prompt":[]}}
        });
        let update = serde_json::json!({
            "direction": "agent->client",
            "message": {
                "jsonrpc":"2.0",
                "method":"session/update",
                "params":{"sessionId":"live","update":{"sessionUpdate":"tool_call","kind":kind}}
            }
        });
        let write = serde_json::json!({
            "direction": "agent->client",
            "message": {
                "jsonrpc":"2.0",
                "id":10,
                "method":"fs/write_text_file",
                "params":{"path":"/CAPTURE_CWD/TASK004_CAPTURE.txt","content":"samchi-for-grok-task-004-live-capture\n"}
            }
        });
        let prompt_res = serde_json::json!({
            "direction": "agent->client",
            "message": {"jsonrpc":"2.0","id":3,"result":{"stopReason":stop}}
        });
        [
            init_req, init_res, new_req, prompt_req, update, write, prompt_res,
        ]
        .into_iter()
        .map(|v| format!("{v}\n"))
        .collect()
    }

    fn sample_meta(version: &str, stop: &str) -> String {
        serde_json::json!({
            "grok_version": format!("grok {version} (deadbeef) [stable]"),
            "agent_version": version,
            "argv": ["grok","agent","--no-leader","stdio"],
            "owner": "capture parent owns the child via --no-leader",
            "stop_reason": stop,
            "written_marker": "samchi-for-grok-task-004-live-capture"
        })
        .to_string()
    }

    #[test]
    fn parse_parts_accepts_live_shaped_turn() {
        let parsed = parse_capture_parts(
            &sample_meta("1.0.13", "end_turn"),
            &sample_traffic("1.0.13", "end_turn", "edit"),
            "samchi-for-grok-task-004-live-capture\n",
        )
        .expect("parse");
        assert!(parsed.initialize_request);
        assert!(parsed.initialize_result);
        assert!(parsed.session_new);
        assert!(parsed.session_prompt);
        assert!(parsed.session_update);
        assert!(parsed.edit_kind);
        assert!(parsed.fs_write_text_file);
        assert_eq!(parsed.stop_reason, "end_turn");
        assert_eq!(parsed.grok_agent_version, "1.0.13");
        assert!(parsed.launched_with_no_leader());
        assert!(!parsed.contains_fake_stub_ids);
    }

    #[test]
    fn parse_parts_rejects_stub_ids() {
        let mut traffic = sample_traffic("1.0.13", "end_turn", "edit");
        let extra = serde_json::json!({
            "direction": "agent->client",
            "message": {
                "jsonrpc": "2.0",
                "method": "session/update",
                "params": {"sessionId": STUB_SESSION_ID}
            }
        });
        traffic.push_str(&format!("{extra}\n"));
        let parsed = parse_capture_parts(
            &sample_meta("1.0.13", "end_turn"),
            &traffic,
            "samchi-for-grok-task-004-live-capture\n",
        )
        .expect("parse");
        assert!(parsed.contains_fake_stub_ids);
    }

    #[test]
    fn parse_parts_requires_matching_agent_version() {
        let err = parse_capture_parts(
            &sample_meta("1.0.13", "end_turn"),
            &sample_traffic("9.9.9", "end_turn", "edit"),
            "samchi-for-grok-task-004-live-capture\n",
        )
        .expect_err("version mismatch");
        assert!(err.contains("agentVersion"), "{err}");
    }
}
