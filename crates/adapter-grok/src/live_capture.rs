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
/// Repo-relative path of the TASK-005 go/no-go ADR.
pub const GO_ADR_RELPATH: &str = "docs/architecture-decision-records/0002-grok-agent-stdio.md";
/// Repo-relative path of the ADR index.
pub const ADR_INDEX_RELPATH: &str = "docs/architecture-decision-records/README.md";

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
    /// Initialize `agentCapabilities.loadSession` (and metadata, if present).
    pub load_session_advertised: bool,
    /// Traffic contains an ACP `session/load` method.
    pub session_load_invoked: bool,
}

impl LiveStdioCapture {
    /// True when argv includes `--no-leader` and an owner note is present.
    pub fn launched_with_no_leader(&self) -> bool {
        self.argv.iter().any(|a| a == "--no-leader") && !self.owner.is_empty()
    }

    /// True when argv includes `--always-approve`.
    pub fn launched_with_always_approve(&self) -> bool {
        self.argv.iter().any(|a| a == "--always-approve")
    }

    /// True when argv uses `stdio` and not `serve` / `leader`.
    pub fn launched_as_stdio_child(&self) -> bool {
        self.argv.iter().any(|a| a == "stdio")
            && !self.argv.iter().any(|a| a == "serve" || a == "leader")
    }

    /// True when argv includes `--sandbox`.
    pub fn launched_with_sandbox(&self) -> bool {
        self.argv.iter().any(|a| a == "--sandbox")
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
    let mut session_load_invoked = false;
    let mut grok_agent_version = String::new();
    let mut stop_reason = String::new();
    let mut init_ids = Vec::new();
    let mut prompt_ids = Vec::new();
    let mut load_session_from_init: Option<bool> = None;

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
            (_, Some("session/load")) => session_load_invoked = true,
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
                    if let Some(v) = result
                        .pointer("/agentCapabilities/loadSession")
                        .and_then(Value::as_bool)
                    {
                        load_session_from_init = Some(v);
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

    let meta_load = meta.get("load_session_advertised").and_then(Value::as_bool);
    let load_session_advertised = match (load_session_from_init, meta_load) {
        (Some(from_init), Some(from_meta)) if from_init != from_meta => {
            return Err(format!(
                "loadSession {from_init} != metadata load_session_advertised {from_meta}"
            ));
        }
        (Some(from_init), _) => from_init,
        (None, Some(from_meta)) => from_meta,
        (None, None) => false,
    };

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
        load_session_advertised,
        session_load_invoked,
    })
}

/// Read the TASK-005 go ADR from `repo_root`.
pub fn read_go_adr(repo_root: impl AsRef<Path>) -> Result<String, String> {
    read_to_string(repo_root.as_ref().join(GO_ADR_RELPATH))
}

/// Read the ADR index from `repo_root`.
pub fn read_adr_index(repo_root: impl AsRef<Path>) -> Result<String, String> {
    read_to_string(repo_root.as_ref().join(ADR_INDEX_RELPATH))
}

/// Empty iff `adr` is a **go** that matches `capture` (not a re-stated hope).
pub fn check_go_adr(adr: &str, capture: &LiveStdioCapture) -> Vec<String> {
    let mut errs = Vec::new();

    if !capture_supports_go(capture) {
        errs.push("capture does not support a go decision".to_string());
    }

    let decision = markdown_section(adr, "Decision").unwrap_or("");
    let first_decision = decision
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !first_decision.starts_with("**go") {
        errs.push("Decision section must open with **go".to_string());
    }
    if first_decision.starts_with("**no-go") {
        errs.push("Decision section must not be no-go".to_string());
    }

    for cmd in ["grok agent stdio", "grok agent serve", "grok agent leader"] {
        if !adr.contains(cmd) {
            errs.push(format!("ADR must name {cmd}"));
        }
    }
    if capture.launched_with_no_leader() && !adr.contains("--no-leader") {
        errs.push("ADR must record --no-leader as the reason v1 uses grok agent stdio".to_string());
    }

    if capture.load_session_advertised {
        if !adr.contains("loadSession") {
            errs.push("ADR must mention initialize loadSession".to_string());
        }
        if !adr.contains("load_session_advertised") {
            errs.push("ADR must mention capture load_session_advertised".to_string());
        }
        if !adr.to_ascii_lowercase().contains("advertised") {
            errs.push("ADR must record session/load as advertised".to_string());
        }
    } else if adr.contains("loadSession")
        && adr.to_ascii_lowercase().contains("advertised")
        && !adr.to_ascii_lowercase().contains("not advertised")
    {
        errs.push("ADR claims loadSession advertised but capture does not".to_string());
    }

    let adr_lower = adr.to_ascii_lowercase();
    if !capture.session_load_invoked {
        if !adr_lower.contains("not invoked") {
            errs.push("ADR must record that session/load was not invoked".to_string());
        }
        if adr_lower.contains("session/load was invoked") {
            errs.push("ADR must not treat session/load invocation as proven".to_string());
        }
    }

    let fork = markdown_section(adr, "thread/fork").unwrap_or("");
    if fork.is_empty() {
        errs.push("ADR must have a thread/fork section".to_string());
    } else if !fork.to_ascii_lowercase().contains("not decided") {
        errs.push("thread/fork section must say this ADR does not decide it".to_string());
    }

    let inscope = markdown_section(adr, "EPIC-003 in-scope (observed)").unwrap_or("");
    if inscope.is_empty() {
        errs.push("ADR must list EPIC-003 in-scope (observed)".to_string());
    } else {
        for needle in [
            "grok agent stdio",
            "--no-leader",
            "initialize",
            "session/new",
            "session/prompt",
            "session/update",
            "end_turn",
        ] {
            if !inscope.contains(needle) {
                errs.push(format!("in-scope section missing {needle}"));
            }
        }
        if !inscope.contains("edit") || !inscope.contains("write") {
            errs.push("in-scope section must list edit/write file change".to_string());
        }
        if capture.launched_with_always_approve() && !inscope.contains("--always-approve") {
            errs.push("in-scope section must list --always-approve".to_string());
        }
        if !capture.launched_with_always_approve() && inscope.contains("--always-approve") {
            errs.push("in-scope lists --always-approve but capture argv does not".to_string());
        }
        if !capture.session_load_invoked && inscope.contains("session/load") {
            errs.push("in-scope must not list session/load (not invoked)".to_string());
        }
        if inscope.contains("thread/fork") {
            errs.push("in-scope must not list thread/fork".to_string());
        }
        if !capture.launched_with_sandbox() && inscope.contains("--sandbox") {
            errs.push("in-scope must not treat --sandbox as proven".to_string());
        }
    }

    let unproven = markdown_section(adr, "Not proven (unobserved)").unwrap_or("");
    if unproven.is_empty() {
        errs.push("ADR must list Not proven (unobserved)".to_string());
    } else {
        for needle in [
            "session/load",
            "sandbox",
            "grok agent serve",
            "grok agent leader",
            "thread/fork",
        ] {
            if !unproven.contains(needle) {
                errs.push(format!("unproven section missing {needle}"));
            }
        }
    }

    let rejected = markdown_section(adr, "Rejected alternatives").unwrap_or("");
    if !rejected.contains("grok -p") {
        errs.push("Rejected alternatives must include grok -p fallback".to_string());
    }
    if !rejected.to_ascii_lowercase().contains("no-go") {
        errs.push("Rejected alternatives must include no-go".to_string());
    }

    errs
}

/// Empty iff the ADR index lists the go record.
pub fn check_adr_index(index: &str) -> Vec<String> {
    let mut errs = Vec::new();
    if !index.contains("0002-grok-agent-stdio.md") {
        errs.push("ADR index must list 0002-grok-agent-stdio.md".to_string());
    }
    let lower = index.to_ascii_lowercase();
    if !lower.contains("go") {
        errs.push("ADR index must record the go".to_string());
    }
    errs
}

fn capture_supports_go(capture: &LiveStdioCapture) -> bool {
    capture.initialize_request
        && capture.initialize_result
        && capture.session_new
        && capture.session_prompt
        && capture.session_update
        && capture.edit_kind
        && capture.fs_write_text_file
        && capture.stop_reason == "end_turn"
        && capture.launched_with_no_leader()
        && capture.launched_as_stdio_child()
        && capture.launched_with_always_approve()
        && !capture.contains_fake_stub_ids
        && !capture.session_load_invoked
}

fn markdown_section<'a>(body: &'a str, heading: &str) -> Option<&'a str> {
    let marker = format!("## {heading}");
    let start = body.find(&marker)?;
    let after = start + marker.len();
    let end = body[after..]
        .find("\n## ")
        .map(|i| after + i)
        .unwrap_or(body.len());
    Some(body[after..end].trim())
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
                "result":{
                    "protocolVersion":1,
                    "_meta":{"agentVersion":agent_version},
                    "agentCapabilities":{"loadSession":true}
                }
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
            "argv": ["grok","agent","--no-leader","--always-approve","stdio"],
            "owner": "capture parent owns the child via --no-leader",
            "stop_reason": stop,
            "written_marker": "samchi-for-grok-task-004-live-capture",
            "load_session_advertised": true
        })
        .to_string()
    }

    fn sample_go_capture() -> LiveStdioCapture {
        parse_capture_parts(
            &sample_meta("1.0.13", "end_turn"),
            &sample_traffic("1.0.13", "end_turn", "edit"),
            "samchi-for-grok-task-004-live-capture\n",
        )
        .expect("parse")
    }

    fn sample_go_adr() -> String {
        r#"# ADR-0002

## Decision

**go.** v1 uses parent-owned `grok agent stdio`.

## Launch candidates

`grok agent stdio`, `grok agent serve`, and `grok agent leader`.
v1 uses stdio because of parent-owned `--no-leader`.

## session/load

`session/load` is advertised, not invoked. initialize `loadSession` matches
`load_session_advertised`.

## thread/fork

`thread/fork` is **not decided** here.

## EPIC-003 in-scope (observed)

parent-owned `grok agent stdio` with `--no-leader`
`initialize`
`session/new`
`session/prompt`
`session/update`
edit/write file change
turn end (`end_turn`)
`--always-approve`

## Not proven (unobserved)

- `session/load` invocation
- sandbox matrix
- `grok agent serve` as owner
- `grok agent leader` as owner
- `thread/fork`

## Rejected alternatives

- **no-go / stop.**
- **`grok -p` fallback.**
"#
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
        assert!(parsed.launched_with_always_approve());
        assert!(parsed.launched_as_stdio_child());
        assert!(!parsed.launched_with_sandbox());
        assert!(parsed.load_session_advertised);
        assert!(!parsed.session_load_invoked);
        assert!(!parsed.contains_fake_stub_ids);
    }

    #[test]
    fn parse_parts_rejects_load_session_mismatch() {
        let mut meta: serde_json::Value =
            serde_json::from_str(&sample_meta("1.0.13", "end_turn")).unwrap();
        meta["load_session_advertised"] = serde_json::json!(false);
        let err = parse_capture_parts(
            &meta.to_string(),
            &sample_traffic("1.0.13", "end_turn", "edit"),
            "samchi-for-grok-task-004-live-capture\n",
        )
        .expect_err("loadSession mismatch");
        assert!(err.contains("loadSession"), "{err}");
    }

    #[test]
    fn parse_parts_detects_session_load_invoke() {
        let mut traffic = sample_traffic("1.0.13", "end_turn", "edit");
        let extra = serde_json::json!({
            "direction": "client->agent",
            "message": {"jsonrpc":"2.0","id":4,"method":"session/load","params":{"sessionId":"live"}}
        });
        traffic.push_str(&format!("{extra}\n"));
        let parsed = parse_capture_parts(
            &sample_meta("1.0.13", "end_turn"),
            &traffic,
            "samchi-for-grok-task-004-live-capture\n",
        )
        .expect("parse");
        assert!(parsed.session_load_invoked);
        assert!(parsed.load_session_advertised);
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

    #[test]
    fn check_go_adr_accepts_matching_text() {
        let errs = check_go_adr(&sample_go_adr(), &sample_go_capture());
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn check_go_adr_rejects_nogo_decision() {
        let adr = sample_go_adr().replacen("**go.**", "**no-go.**", 1);
        let errs = check_go_adr(&adr, &sample_go_capture());
        assert!(
            errs.iter()
                .any(|e| e.contains("no-go") || e.contains("**go")),
            "{errs:?}"
        );
    }

    #[test]
    fn check_go_adr_rejects_missing_serve() {
        let adr = sample_go_adr().replace("`grok agent serve`", "`grok agent stdio`");
        let errs = check_go_adr(&adr, &sample_go_capture());
        assert!(
            errs.iter().any(|e| e.contains("grok agent serve")),
            "{errs:?}"
        );
    }

    #[test]
    fn check_adr_index_requires_go_record() {
        let errs = check_adr_index(
            "| [0002-grok-agent-stdio.md](0002-grok-agent-stdio.md) | Accepted | go |\n",
        );
        assert!(errs.is_empty(), "{errs:?}");
        let errs = check_adr_index("| [0001-rust-core.md](0001-rust-core.md) |\n");
        assert!(!errs.is_empty(), "expected missing 0002");
    }
}
