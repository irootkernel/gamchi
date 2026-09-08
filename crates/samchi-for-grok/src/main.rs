//! Command samchi-for-grok is the local Grok worker, MCP host surface, and later
//! app-server. TASK-008 publishes CLI worker start/wait/status/result/list.

mod worker;

use std::io::{self, Write};
use std::process::ExitCode;
use worker::run_worker;

/// Honest initialize identity. It is not Codex or CCAS.
/// Kept out of `samchi-core`.
/// Consumed by TASK-016 initialize; unused in this stub.
#[allow(dead_code)]
const USER_AGENT: &str = "samchi-for-grok/app-server-v1";

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
Usage:
  samchi-for-grok version [--json]
  samchi-for-grok worker <start|wait|status|result|list> ...
  samchi-for-grok mcp
  samchi-for-grok app-server --listen unix://<absolute-path> [--home <absolute-path>]
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    ExitCode::from(run(
        &refs,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    ))
}

fn run(args: &[&str], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
    if args.is_empty() {
        write_all(stderr, USAGE);
        return 2;
    }
    match args {
        ["version"] | ["--version"] => {
            write_all(stdout, &format!("{NAME} v{VERSION}\n"));
            0
        }
        ["version", "--json"] => {
            write_all(
                stdout,
                &format!("{{\"name\":\"{NAME}\",\"version\":\"v{VERSION}\"}}\n"),
            );
            0
        }
        ["version", ..] | ["--version", ..] => {
            write_all(stderr, "SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG\n");
            write_all(stderr, USAGE);
            1
        }
        _ => match args[0] {
            "worker" => run_worker(&args[1..], stdout, stderr),
            "mcp" | "app-server" => {
                write_all(
                    stderr,
                    &format!(
                        "SAMCHI_FOR_GROK_STARTUP_ERROR NOT_IMPLEMENTED {}\n",
                        args[0]
                    ),
                );
                1
            }
            other => {
                write_all(stderr, "SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG\n");
                if !other.starts_with('-') {
                    write_all(stderr, USAGE);
                }
                1
            }
        },
    }
}

fn write_all(w: &mut dyn Write, s: &str) {
    w.write_all(s.as_bytes()).expect("write");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_is_honest_identity() {
        assert_eq!(USER_AGENT, "samchi-for-grok/app-server-v1");
    }

    #[test]
    fn version() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&["version"], &mut stdout, &mut stderr);
        assert_eq!(
            code,
            0,
            "exit {code} stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&stdout),
            format!("{NAME} v{VERSION}\n")
        );
        assert!(
            stderr.is_empty(),
            "stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn version_json() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&["version", "--json"], &mut stdout, &mut stderr);
        assert_eq!(
            code,
            0,
            "exit {code} stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&stdout),
            format!("{{\"name\":\"{NAME}\",\"version\":\"v{VERSION}\"}}\n")
        );
        assert!(
            stderr.is_empty(),
            "stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn version_rejects_unknown_flags() {
        for args in [
            &["version", "--json", "extra"] as &[&str],
            &["version", "--JSON"],
            &["--version", "--json"],
        ] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = run(args, &mut stdout, &mut stderr);
            assert_eq!(code, 1, "{args:?}: exit {code}");
            assert!(
                stdout.is_empty(),
                "{args:?}: stdout {:?}",
                String::from_utf8_lossy(&stdout)
            );
            assert!(
                String::from_utf8_lossy(&stderr)
                    .contains("SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG"),
                "{args:?}: stderr {:?}",
                String::from_utf8_lossy(&stderr)
            );
        }
    }

    #[test]
    fn no_args() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&[], &mut stdout, &mut stderr);
        assert_eq!(code, 2);
        assert!(
            stdout.is_empty(),
            "stdout {:?}",
            String::from_utf8_lossy(&stdout)
        );
        assert!(
            String::from_utf8_lossy(&stderr).contains("samchi-for-grok version"),
            "stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn unknown_command() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&["nope"], &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(
            String::from_utf8_lossy(&stderr)
                .contains("SAMCHI_FOR_GROK_STARTUP_ERROR INVALID_CONFIG"),
            "stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn unimplemented_surfaces() {
        for cmd in ["mcp", "app-server"] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = run(&[cmd], &mut stdout, &mut stderr);
            assert_eq!(code, 1, "{cmd}: exit {code}");
            let err = String::from_utf8_lossy(&stderr);
            assert!(
                err.contains(&format!(
                    "SAMCHI_FOR_GROK_STARTUP_ERROR NOT_IMPLEMENTED {cmd}"
                )),
                "{cmd}: stderr {err:?}"
            );
        }
    }

    #[test]
    fn worker_usage_omits_unpublished_verbs() {
        assert!(!USAGE.contains("cancel"));
        assert!(!worker::WORKER_USAGE.contains("cancel"));
        assert!(worker::WORKER_USAGE.contains("worker start --json"));
        assert!(worker::WORKER_USAGE.contains("worker wait --json"));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&["worker", "cancel"], &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        let err = String::from_utf8_lossy(&stderr);
        assert!(err.contains("INVALID_CONFIG"), "{err}");
        assert!(err.contains("worker start --json"), "{err}");
    }
}
