//! Parse the checked-in TASK-030 instruction-channel capture.
//!
//! Does not spawn Grok.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn capture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("captures/task-030")
}

#[test]
fn checked_in_task030_metadata_is_refuse() {
    let body = fs::read_to_string(capture_dir().join("metadata.json")).expect("metadata");
    let v: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(v["task"], "TASK-030");
    let version = v["grok_version"].as_str().expect("grok_version");
    assert!(
        version.contains("1.0.25"),
        "pinned grok version, got {version}"
    );
    assert_eq!(v["authenticated"], true);
    assert_eq!(v["verdict"], "no-go-for-apply-go-for-refuse");
    assert!(v["spawn_owned_channel"].is_null());
    let argv = v["agent_stdio_argv_shape"]
        .as_array()
        .expect("argv")
        .iter()
        .filter_map(|x| x.as_str())
        .collect::<Vec<_>>();
    assert!(argv.contains(&"stdio"));
    assert!(argv.contains(&"--no-leader"));
    assert!(!argv.contains(&"serve") && !argv.contains(&"leader"));

    let mut honored_ids = Vec::new();
    let mut refused_unique = Vec::new();
    for probe in v["probes"].as_array().expect("probes") {
        let id = probe["id"].as_str().expect("id");
        let honored = probe["honored"].as_bool().expect("honored");
        let unique = probe["unique_token"].as_bool().expect("unique_token");
        if honored {
            honored_ids.push(id.to_string());
            assert!(
                unique,
                "{id} honored without a unique token (contamination risk)"
            );
            assert!(
                id.starts_with("cwd-"),
                "{id} honored but is not a cwd instruction file"
            );
        } else if unique {
            refused_unique.push(id.to_string());
        }
    }
    assert!(
        honored_ids
            .iter()
            .any(|id| id.contains("extra.rules-no-flag")),
        "cwd extra.rules without --rules must be recorded as honored, got {honored_ids:?}"
    );
    assert!(
        honored_ids.iter().any(|id| id.contains("AGENTS.md")),
        "cwd AGENTS.md must be recorded as honored, got {honored_ids:?}"
    );
    assert!(
        refused_unique.iter().any(|id| id.contains("outside-cwd")),
        "--rules outside cwd must be unique and not honored, got {refused_unique:?}"
    );
    assert!(
        refused_unique
            .iter()
            .any(|id| id.contains("system-prompt-override")),
        "unique --system-prompt-override must not be honored, got {refused_unique:?}"
    );
    assert!(
        refused_unique.iter().any(|id| id.contains("inline-text")),
        "unique inline --rules must not be honored, got {refused_unique:?}"
    );
}

#[test]
fn checked_in_task030_adr_matches_capture() {
    let adr = fs::read_to_string(
        repo_root()
            .join("docs/architecture-decision-records/0003-developer-instructions-refuse.md"),
    )
    .expect("ADR-0003");
    let lower = adr.to_lowercase();
    assert!(
        lower.contains("no-go for apply") && lower.contains("go for refuse"),
        "ADR-0003 must record no-go for apply / go for refuse"
    );
    assert!(adr.contains("1.0.25"), "ADR-0003 must pin grok 1.0.25");
    assert!(
        lower.contains("agents.md") && lower.contains("extra.rules"),
        "ADR-0003 must name cwd instruction files"
    );
    assert!(
        lower.contains("collide") || lower.contains("collides"),
        "ADR-0003 must reject cwd files as an apply path"
    );
    assert!(
        lower.contains("inline text") && lower.contains("not honored"),
        "ADR-0003 must record that --rules inline text was not honored"
    );
    let index =
        fs::read_to_string(repo_root().join("docs/architecture-decision-records/README.md"))
            .expect("ADR index");
    assert!(
        index.contains("0003-developer-instructions-refuse.md"),
        "ADR index must list ADR-0003"
    );
}
