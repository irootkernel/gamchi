//! Command gamchi is the local Grok worker, MCP host surface, and
//! app-server listen facade. TASK-015 publishes Unix-domain HTTP/WS upgrade.

mod app_server;
mod mcp;
mod ops;
mod worker;

use std::io::{self, Write};
use std::process::ExitCode;
use worker::run_worker;

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
Usage:
  gamchi version [--json]
  gamchi worker <start|wait|status|result|list|cancel|followup|respond> ...
  gamchi mcp [--home <absolute-path>]
  gamchi app-server --listen unix://<absolute-path> [--home <absolute-path>]
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
            write_all(stderr, "GAMCHI_STARTUP_ERROR INVALID_CONFIG\n");
            write_all(stderr, USAGE);
            1
        }
        _ => match args[0] {
            "worker" => {
                samchi_adapter_grok::install_signal_teardown();
                run_worker(&args[1..], stdout, stderr)
            }
            "mcp" => {
                samchi_adapter_grok::install_signal_teardown();
                let stdin = io::stdin();
                let mut lock = stdin.lock();
                mcp::run_mcp(&args[1..], &mut lock, stdout)
            }
            "app-server" => {
                samchi_adapter_grok::install_signal_teardown();
                app_server::run(&args[1..], stderr)
            }
            other => {
                write_all(stderr, "GAMCHI_STARTUP_ERROR INVALID_CONFIG\n");
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
        assert_eq!(app_server::USER_AGENT, "gamchi/app-server-v1");
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
                String::from_utf8_lossy(&stderr).contains("GAMCHI_STARTUP_ERROR INVALID_CONFIG"),
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
            String::from_utf8_lossy(&stderr).contains("gamchi version"),
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
            String::from_utf8_lossy(&stderr).contains("GAMCHI_STARTUP_ERROR INVALID_CONFIG"),
            "stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn app_server_requires_listen() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&["app-server"], &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        let err = String::from_utf8_lossy(&stderr);
        assert!(
            err.contains("GAMCHI_STARTUP_ERROR INVALID_CONFIG"),
            "stderr {err:?}"
        );
        assert!(
            stdout.is_empty(),
            "stdout {:?}",
            String::from_utf8_lossy(&stdout)
        );
    }

    #[test]
    fn worker_usage_lists_published_verbs() {
        assert!(USAGE.contains("cancel"));
        assert!(worker::WORKER_USAGE.contains("worker cancel --json"));
        assert!(USAGE.contains("followup"));
        assert!(worker::WORKER_USAGE.contains("worker followup --json"));
        assert!(USAGE.contains("respond"));
        assert!(worker::WORKER_USAGE.contains("worker respond --json"));
        assert!(worker::WORKER_USAGE.contains("worker start --json"));
        assert!(worker::WORKER_USAGE.contains("[--model <id>]"));
        assert!(worker::WORKER_USAGE.contains("[--effort <id>]"));
        assert!(worker::WORKER_USAGE.contains("worker wait --json"));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&["worker", "respond"], &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        let err = String::from_utf8_lossy(&stderr);
        assert!(err.contains("INVALID_CONFIG"), "{err}");
        assert!(err.contains("worker respond --json"), "{err}");
    }
}
