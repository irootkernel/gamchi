//! Offline TASK-010 packaging checks. Does not spawn Grok or edit host config.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn read(rel: &str) -> String {
    fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|err| panic!("{rel}: {err}"))
}

#[test]
fn skill_states_async_host_contract() {
    let skill = read("skills/use-samchi-for-grok/SKILL.md");
    for needle in [
        "Call `grok_spawn` exactly once",
        "Call `grok_await` with that `turn_id`",
        "Do not poll `grok_status`",
        "If `grok_await` ends because of **host** timeout, call `grok_await` again",
        "Do not treat host cancel of the await tool as",
        "`grok_cancel`",
        "tool_timeout_sec = 3600",
        "Timeout does not cancel the turn",
        "Do not silently edit user config",
    ] {
        assert!(skill.contains(needle), "skill missing {needle:?}\n{skill}");
    }
}

#[test]
fn codex_snippet_sets_hour_timeout() {
    let toml = read("docs/ops/codex-mcp.toml");
    assert!(toml.contains("[mcp_servers.samchi-for-grok]"));
    assert!(toml.contains("command = \"samchi-for-grok\""));
    assert!(toml.contains("args = [\"mcp\"]"));
    assert!(toml.contains("tool_timeout_sec = 3600"));
    assert!(toml.contains("Do not silently edit user config"));
}

#[test]
fn claude_snippet_is_project_mcp_json() {
    let json = read("docs/ops/claude-mcp.json");
    let v: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(
        v["mcpServers"]["samchi-for-grok"]["command"],
        "samchi-for-grok"
    );
    assert_eq!(
        v["mcpServers"]["samchi-for-grok"]["args"],
        serde_json::json!(["mcp"])
    );
}

#[test]
fn ops_index_says_observer_timeout_is_not_cancel() {
    let ops = read("docs/ops/README.md");
    assert!(ops.contains("Host observer timeout of await is not `grok_cancel`"));
    assert!(ops.contains("Do not silently edit user host config"));
}

#[test]
fn host_exercise_records_available_parents() {
    let rec = read("docs/ops/host-exercise.md");
    assert!(rec.contains("User host config was not modified"));
    assert!(rec.contains("Claude Code"));
    assert!(rec.contains("Codex"));
    assert!(rec.contains("Observer timeout of await is not `grok_cancel`"));
}
