//! Parse the checked-in TASK-033 `_meta.rules` restore capture.
//!
//! Does not spawn Grok.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn capture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("captures/task-033")
}

#[test]
fn checked_in_task033_metadata_is_go_for_apply() {
    let body = fs::read_to_string(capture_dir().join("metadata.json")).expect("metadata");
    let v: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(v["task"], "TASK-033");
    let version = v["grok_version"].as_str().expect("grok_version");
    assert!(
        version.contains("1.0.30"),
        "pinned grok version, got {version}"
    );
    assert_eq!(v["authenticated"], true);
    assert_eq!(v["verdict"], "go-for-apply");
    assert_eq!(v["spawn_owned_channel"], "session/new _meta.rules");
    assert_eq!(v["control_ok"], true);
    assert_eq!(v["first_turn_ok"], true);
    assert_eq!(v["restore_ok"], true);
    assert_eq!(v["b_leaked_in_first"], false);
    assert_eq!(v["different_process"], true);
    let method = v["restore_method"].as_str().expect("restore_method");
    assert!(
        method.contains("session/load"),
        "restore must name session/load, got {method}"
    );
    assert!(
        method.contains("no rules re-sent") || method.contains("no `_meta.rules`"),
        "restore must not re-send rules, got {method}"
    );

    let argv = v["agent_stdio_argv_shape"]
        .as_array()
        .expect("argv")
        .iter()
        .filter_map(|x| x.as_str())
        .collect::<Vec<_>>();
    assert!(argv.contains(&"stdio"));
    assert!(argv.contains(&"--no-leader"));
    assert!(!argv.contains(&"serve") && !argv.contains(&"leader"));

    let token_a = v["token_a"].as_str().expect("token_a");
    let token_b = v["token_b"].as_str().expect("token_b");
    assert_ne!(token_a, token_b);
    assert!(token_a.starts_with('A') && token_b.starts_with('B'));

    let mut by_id = std::collections::HashMap::new();
    for probe in v["probes"].as_array().expect("probes") {
        by_id.insert(probe["id"].as_str().expect("id").to_string(), probe.clone());
        assert_eq!(probe["authenticated"], true);
        assert!(probe["error"].is_null());
        assert_eq!(probe["load_session_advertised"], true);
    }
    let control = &by_id["control-no-meta-rules"];
    let first = &by_id["first-turn-meta-rules"];
    let restore = &by_id["restore-session-load-new-process"];
    assert!(
        control["reply"]
            .as_str()
            .expect("control reply")
            .contains("NONE"),
        "control must include NONE"
    );
    assert!(!control["reply"].as_str().unwrap().contains(token_a));
    assert_eq!(first["reply"].as_str().expect("first reply"), token_a);
    assert_eq!(restore["reply"].as_str().expect("restore reply"), token_b);
    assert_eq!(first["session_id"], restore["session_id"]);
    assert_ne!(first["pid"], restore["pid"]);
    assert!(!first["reply"].as_str().unwrap().contains(token_b));
}

#[test]
fn checked_in_task033_adr_matches_capture() {
    let adr = fs::read_to_string(
        repo_root()
            .join("docs/architecture-decision-records/0004-developer-instructions-meta-rules.md"),
    )
    .expect("ADR-0004");
    let lower = adr.to_lowercase();
    assert!(
        lower.contains("go for apply"),
        "ADR-0004 must record go for apply"
    );
    assert!(adr.contains("1.0.30"), "ADR-0004 must pin grok 1.0.30");
    assert!(
        adr.contains("_meta.rules") && adr.contains("session/load"),
        "ADR-0004 must name _meta.rules and session/load"
    );
    assert!(
        lower.contains("current code still refuses"),
        "ADR-0004 must state current code still refuses"
    );
    assert!(
        lower.contains("systempromptoverride"),
        "ADR-0004 must reject systemPromptOverride for role text"
    );
    assert!(
        lower.contains("set_acp_session_id") || lower.contains("persisting the acp session id"),
        "ADR-0004 must specify session-id persistence failure"
    );
    assert!(
        lower.contains("fail closed") || lower.contains("fail-closed"),
        "ADR-0004 must specify legacy fail-closed policy"
    );

    let superseded = fs::read_to_string(
        repo_root()
            .join("docs/architecture-decision-records/0003-developer-instructions-refuse.md"),
    )
    .expect("ADR-0003");
    assert!(
        superseded.contains("Superseded"),
        "ADR-0003 status must be Superseded"
    );

    let index =
        fs::read_to_string(repo_root().join("docs/architecture-decision-records/README.md"))
            .expect("ADR index");
    assert!(
        index.contains("0004-developer-instructions-meta-rules.md"),
        "ADR index must list ADR-0004"
    );
}
