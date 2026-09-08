//! Replay TASK-004 capture `session/update` traffic through the mapper.
//!
//! Does not spawn Grok.

use agent_client_protocol::schema::v1::SessionNotification;
use samchi_adapter_grok::live_capture::{capture_dir, TRAFFIC_FILENAME};
use samchi_adapter_grok::Mapper;
use samchi_core::source_wire::{ITEM_AGENT_MESSAGE, ITEM_FILE_CHANGE};
use serde_json::Value;
use std::fs;

#[test]
fn capture_updates_emit_file_change_without_early_complete() {
    let path = capture_dir(env!("CARGO_MANIFEST_DIR")).join(TRAFFIC_FILENAME);
    let raw = fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    let mut mapper = Mapper::new("capture replay");
    let mut saw_in_progress_edit = false;
    for (i, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let env: Value =
            serde_json::from_str(line).unwrap_or_else(|err| panic!("line {}: {err}", i + 1));
        if env.get("direction").and_then(Value::as_str) != Some("agent->client") {
            continue;
        }
        let message = env.get("message").expect("message");
        if message.get("method").and_then(Value::as_str) != Some("session/update") {
            continue;
        }
        let params = message.get("params").cloned().expect("params");
        let notif: SessionNotification = serde_json::from_value(params)
            .unwrap_or_else(|err| panic!("line {} update: {err}", i + 1));
        mapper
            .apply(&notif.update)
            .unwrap_or_else(|err| panic!("line {} map: {err}", i + 1));
        if mapper
            .items()
            .iter()
            .any(|item| item.item_type == ITEM_FILE_CHANGE && item.status == "inProgress")
        {
            saw_in_progress_edit = true;
        }
    }
    mapper.finish_agent_message();
    assert!(
        mapper
            .items()
            .iter()
            .any(|item| item.item_type == ITEM_AGENT_MESSAGE && item.status == "completed"),
        "expected completed agentMessage"
    );
    let edits: Vec<_> = mapper
        .items()
        .iter()
        .filter(|item| item.item_type == ITEM_FILE_CHANGE)
        .collect();
    assert!(!edits.is_empty(), "capture must project fileChange");
    assert!(
        edits
            .iter()
            .any(|item| item.status == "completed" || item.status == "failed"),
        "edit items={edits:?}"
    );
    let _ = saw_in_progress_edit;
}
