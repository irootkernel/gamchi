//! Validates that docs/roadmap/README.md is the sole lifecycle authority
//! for grokgrok epics and tasks (TASK-001).

use regex::Regex;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

pub const ROADMAP_PATH: &str = "docs/roadmap/README.md";

const MAX_TASKS_PER_EPIC: usize = 7;

const STATUS_PLANNED: &str = "Planned";
const STATUS_IN_PROGRESS: &str = "In Progress";
const STATUS_IN_REVIEW: &str = "In Review";
const STATUS_COMPLETED: &str = "Completed";
const STATUS_BLOCKED: &str = "Blocked";
const STATUS_DEFERRED: &str = "Deferred";

static VALID_STATUSES: &[&str] = &[
    STATUS_PLANNED,
    STATUS_IN_PROGRESS,
    STATUS_IN_REVIEW,
    STATUS_COMPLETED,
    STATUS_BLOCKED,
    STATUS_DEFERRED,
];

static TASK_ROW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\| \[TASK-([0-9]{3})\]\([^)]*\) \| ([^|]+) \| `([^`]+)` \| ([^|]+) \|")
        .unwrap()
});
static EPIC_HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^## (EPIC-[0-9]{3}): ").unwrap());
static EPIC_STATUS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Status: `([^`]*)`$").unwrap());
static CURRENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Current Task: (`TASK-[0-9]{3}`|none)").unwrap());
static NEXT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^Next eligible Task: (`TASK-[0-9]{3}`|none)").unwrap());
static EPIC_SPLIT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^## EPIC-[0-9]{3}: ").unwrap());

/// One roadmap integrity failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub check: String,
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.check, self.message)
    }
}

struct TaskRow {
    id: String,
    status: String,
}

/// Loads the repository roadmap and checks it.
pub fn check_file(path: impl AsRef<Path>) -> Vec<Violation> {
    match fs::read_to_string(path) {
        Ok(body) => check(&body),
        Err(err) => vec![Violation {
            check: "roadmap-readable".to_string(),
            message: err.to_string(),
        }],
    }
}

/// Validates roadmap markdown.
pub fn check(body: &str) -> Vec<Violation> {
    let mut vs = Vec::new();
    let rows = parse_tasks(body);
    if rows.is_empty() {
        vs.push(Violation {
            check: "task-count".to_string(),
            message: "roadmap has no TASK rows".to_string(),
        });
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut active = Vec::new();
    for row in &rows {
        *seen.entry(row.id.clone()).or_insert(0) += 1;
        if !VALID_STATUSES.contains(&row.status.as_str()) {
            vs.push(Violation {
                check: "task-status".to_string(),
                message: format!("TASK-{} has invalid status {:?}", row.id, row.status),
            });
        }
        if row.status == STATUS_IN_PROGRESS || row.status == STATUS_IN_REVIEW {
            active.push(format!("TASK-{}", row.id));
        }
    }
    for (id, n) in &seen {
        if *n > 1 {
            vs.push(Violation {
                check: "task-set".to_string(),
                message: format!("TASK-{id} appears {n} times"),
            });
        }
    }

    if active.len() > 1 {
        vs.push(Violation {
            check: "one-active-task".to_string(),
            message: format!(
                "at most one In Progress or In Review task, got {}",
                active.join(", ")
            ),
        });
    }

    let epics: Vec<_> = EPIC_HEADER_RE.captures_iter(body).collect();
    if epics.is_empty() {
        vs.push(Violation {
            check: "epic-count".to_string(),
            message: "roadmap has no EPIC headers".to_string(),
        });
    }
    for (i, m) in epics.iter().enumerate() {
        let want = format!("EPIC-{:03}", i + 1);
        let got = m.get(1).map(|c| c.as_str()).unwrap_or("");
        if got != want {
            vs.push(Violation {
                check: "epic-order".to_string(),
                message: format!("epic {} is {got}, want {want}", i + 1),
            });
        }
    }
    let status_count = EPIC_STATUS_RE.captures_iter(body).count();
    if status_count != epics.len() {
        vs.push(Violation {
            check: "epic-status".to_string(),
            message: format!("want {} epic Status lines, got {status_count}", epics.len()),
        });
    }

    for (i, section) in epic_task_sections(body).iter().enumerate() {
        let n = parse_tasks(section).len();
        if n > MAX_TASKS_PER_EPIC {
            vs.push(Violation {
                check: "epic-task-cap".to_string(),
                message: format!("EPIC-{:03} has {n} tasks, max {MAX_TASKS_PER_EPIC}", i + 1),
            });
        }
    }

    let cur = CURRENT_RE.captures(body);
    let nxt = NEXT_RE.captures(body);
    if cur.is_none() {
        vs.push(Violation {
            check: "current-task".to_string(),
            message: "missing Current Task line".to_string(),
        });
    }
    if nxt.is_none() {
        vs.push(Violation {
            check: "next-task".to_string(),
            message: "missing Next eligible Task line".to_string(),
        });
    }
    if let Some(cur) = cur {
        let got = cur
            .get(1)
            .map(|c| c.as_str().trim_matches('`'))
            .unwrap_or("");
        match active.len() {
            0 if got != "none" => {
                vs.push(Violation {
                    check: "current-task".to_string(),
                    message: format!(
                        "Current Task must be none when no task is In Progress or In Review, got {got}"
                    ),
                });
            }
            1 if got != active[0] => {
                vs.push(Violation {
                    check: "current-task".to_string(),
                    message: format!("Current Task must be {}, got {got}", active[0]),
                });
            }
            _ => {}
        }
    }
    if let Some(nxt) = nxt {
        let got = nxt
            .get(1)
            .map(|c| c.as_str().trim_matches('`'))
            .unwrap_or("");
        if got != "none" {
            let num = got.strip_prefix("TASK-").unwrap_or(got);
            if num.parse::<u32>().is_err() {
                vs.push(Violation {
                    check: "next-task".to_string(),
                    message: format!("malformed Next eligible Task {got}"),
                });
            }
        }
    }
    vs
}

fn epic_task_sections(body: &str) -> Vec<&str> {
    let idxs: Vec<usize> = EPIC_SPLIT_RE.find_iter(body).map(|m| m.start()).collect();
    if idxs.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(idxs.len());
    for (i, start) in idxs.iter().enumerate() {
        let end = if i + 1 < idxs.len() {
            idxs[i + 1]
        } else {
            body.len()
        };
        out.push(&body[*start..end]);
    }
    out
}

fn parse_tasks(body: &str) -> Vec<TaskRow> {
    TASK_ROW_RE
        .captures_iter(body)
        .map(|m| TaskRow {
            id: m.get(1).unwrap().as_str().to_string(),
            status: m.get(3).unwrap().as_str().trim().to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn find_repo_root() -> PathBuf {
        let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut dir = start.clone();
        loop {
            if dir.join("docs/roadmap/README.md").is_file() {
                return dir;
            }
            if !dir.pop() {
                panic!("repo root not found from {}", start.display());
            }
        }
    }

    fn checked_in() -> String {
        fs::read_to_string(find_repo_root().join(ROADMAP_PATH)).unwrap()
    }

    fn has_check(vs: &[Violation], check: &str) -> bool {
        vs.iter().any(|v| v.check == check)
    }

    #[test]
    fn checked_in_roadmap() {
        let vs = check_file(find_repo_root().join(ROADMAP_PATH));
        assert!(vs.is_empty(), "roadmap violations: {vs:?}");
    }

    #[test]
    fn rejects_wrong_task_count() {
        let vs = check("# grokgrok\n\nCurrent Task: none.\nNext eligible Task: none.\n");
        assert!(
            has_check(&vs, "task-count"),
            "expected task-count, got {vs:?}"
        );
    }

    #[test]
    fn allows_appended_task() {
        let mut body = checked_in();
        body.push_str(
            "\n| [TASK-099](#epic-001-foundation) | extra | `Planned` | TASK-003 | extra |\n",
        );
        let vs = check(&body);
        assert!(
            !has_check(&vs, "task-count"),
            "appended task must not trip a fixed count, got {vs:?}"
        );
    }

    #[test]
    fn rejects_two_active_tasks() {
        let body = checked_in();
        const NEEDLE: &str = "| `Planned` | TASK-";
        assert!(body.contains(NEEDLE), "need a Planned task row to promote");
        let body = body.replacen(NEEDLE, "| `In Progress` | TASK-", 1);
        let vs = check(&body);
        assert!(
            has_check(&vs, "one-active-task") || has_check(&vs, "current-task"),
            "expected one-active-task or current-task, got {vs:?}"
        );
    }
}
