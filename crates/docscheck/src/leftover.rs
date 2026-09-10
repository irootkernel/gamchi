//! Allowlisted leftover-name search (TASK-028).

use crate::{Violation, ROADMAP_PATH};
use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

const NEEDLES: &[&str] = &["samchi-for-grok", "SAMCHI_FOR_GROK", "Samchi for Grok"];
const CAPTURE_MARKER: &str = "samchi-for-grok-task-004-live-capture";
const CAPTURE_PREFIX: &str = "crates/adapter-grok/captures/task-004/";
const SCANNER_PATH: &str = "crates/docscheck/src/leftover.rs";
const GITHUB_SPANS: &[&str] = &["samchi-for-grok.git", "irootkernel/samchi-for-grok"];

static COMPLETED_TASK_ROW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\| \[TASK-[0-9]{3}\]\([^)]*\) \| [^|]+ \| `Completed` \|").unwrap()
});

/// Scan tracked files under `root` for old product-identity tokens.
pub fn check_leftover_names(root: impl AsRef<Path>) -> Vec<Violation> {
    let root = root.as_ref();
    let files = match git_ls_files(root) {
        Ok(files) => files,
        Err(message) => {
            return vec![Violation {
                check: "leftover-names".to_string(),
                message,
            }]
        }
    };
    let mut vs = Vec::new();
    for rel in files {
        if rel.starts_with(CAPTURE_PREFIX) || rel == SCANNER_PATH {
            continue;
        }
        let path = root.join(&rel);
        let body = match fs::read_to_string(&path) {
            Ok(body) => body,
            Err(_) => continue,
        };
        for hit in leftovers_in(&rel, &body) {
            vs.push(Violation {
                check: "leftover-names".to_string(),
                message: format!("{rel}:{} leftover {:?}", hit.line, hit.needle),
            });
        }
    }
    vs
}

struct Hit {
    line: usize,
    needle: &'static str,
}

fn leftovers_in(rel: &str, body: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (i, line) in body.lines().enumerate() {
        for needle in NEEDLES {
            let mut start = 0;
            while let Some(at) = line[start..].find(needle) {
                let idx = start + at;
                if !allowed(rel, line, idx, needle) {
                    hits.push(Hit {
                        line: i + 1,
                        needle,
                    });
                    break;
                }
                start = idx + needle.len();
            }
        }
    }
    hits
}

fn allowed(rel: &str, line: &str, idx: usize, needle: &str) -> bool {
    if covered_by(line, idx, needle.len(), CAPTURE_MARKER) {
        return true;
    }
    for span in GITHUB_SPANS {
        if covered_by(line, idx, needle.len(), span) {
            return true;
        }
    }
    if rel != ROADMAP_PATH {
        return false;
    }
    if COMPLETED_TASK_ROW_RE.is_match(line) {
        return true;
    }
    if line.contains("SAMCHI_FOR_GROK_*") {
        return true;
    }
    if line.contains("~/.samchi-for-grok")
        && (line.contains("migrate") || line.contains("auto-discover") || line.contains("copy"))
    {
        return true;
    }
    line.contains("Old-name allowlist")
}

fn covered_by(line: &str, idx: usize, len: usize, span: &str) -> bool {
    let end = idx + len;
    line.match_indices(span)
        .any(|(at, s)| idx >= at && end <= at + s.len())
}

fn git_ls_files(root: &Path) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(["-C", root.to_str().unwrap_or("."), "ls-files", "-z"])
        .output()
        .map_err(|err| format!("git ls-files: {err}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_marker_is_allowlisted() {
        let hits = leftovers_in(
            "crates/adapter-grok/src/live_capture.rs",
            "marker samchi-for-grok-task-004-live-capture\n",
        );
        assert!(
            hits.is_empty(),
            "{:?}",
            hits.iter().map(|h| h.needle).collect::<Vec<_>>()
        );
    }

    #[test]
    fn live_file_leftover_is_flagged() {
        let hits = leftovers_in("README.md", "command samchi-for-grok version\n");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].needle, "samchi-for-grok");
    }

    #[test]
    fn completed_roadmap_row_is_allowlisted() {
        let line = "| [TASK-022](#epic-001-foundation) | Rename product identity to samchi-for-grok | `Completed` | TASK-003 | docs use `samchi-for-grok` |\n";
        let hits = leftovers_in(ROADMAP_PATH, line);
        assert!(hits.is_empty(), "hits {}", hits.len());
    }

    #[test]
    fn planned_roadmap_row_is_flagged() {
        let line = "| [TASK-099](#epic-001-foundation) | leftover | `Planned` | — | uses samchi-for-grok |\n";
        let hits = leftovers_in(ROADMAP_PATH, line);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn hard_cutover_lines_are_allowlisted() {
        let body = "Hard cutover: no `SAMCHI_FOR_GROK_*` env fallback; no auto-discover, copy,\nor migrate of `~/.samchi-for-grok`. Default is `GAMCHI_HOME`.\n";
        let hits = leftovers_in(ROADMAP_PATH, body);
        assert!(hits.is_empty(), "hits {}", hits.len());
    }

    #[test]
    fn github_remote_span_is_allowlisted() {
        let hits = leftovers_in(
            "README.md",
            "clone git@github.com:irootkernel/samchi-for-grok.git\n",
        );
        assert!(hits.is_empty(), "hits {}", hits.len());
    }
}
