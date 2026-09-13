//! Parse the checked-in TASK-035 live app-server capture.
//!
//! Does not spawn Grok.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn capture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("captures/task-035")
}

#[test]
fn checked_in_task035_metadata_is_go_for_apply() {
    let body = fs::read_to_string(capture_dir().join("metadata.json")).expect("metadata");
    let v: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(v["task"], "TASK-035");
    let version = v["grok_version"].as_str().expect("grok_version");
    assert!(
        version.contains("1.0.30"),
        "pinned grok version, got {version}"
    );
    assert_eq!(v["authenticated"], true);
    assert_eq!(v["verdict"], "go-for-apply");
    assert_eq!(v["control_ok"], true);
    assert_eq!(v["first_turn_ok"], true);
    assert_eq!(v["restore_ok"], true);
    assert_eq!(v["different_process"], true);
    assert_eq!(v["same_value_resume_ok"], true);
    assert_eq!(v["change_refuse_ok"], true);
    assert_ne!(v["first_child_pid"], v["restore_child_pid"]);
    assert!(!v["acp_session_id"].as_str().unwrap().is_empty());
    let argv = v["agent_stdio_argv_shape"]
        .as_array()
        .expect("argv")
        .iter()
        .filter_map(|x| x.as_str())
        .collect::<Vec<_>>();
    assert!(argv.contains(&"stdio"));
    assert!(argv.contains(&"--no-leader"));
}
