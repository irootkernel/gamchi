//! Command grokgrok is the local Grok worker, MCP host surface, and later
//! app-server. TASK-021 only advertises the command grammar; worker, MCP, and
//! app-server are not implemented yet.

use std::io::{self, Write};
use std::process::ExitCode;

/// Honest initialize identity. It is not Codex or CCAS.
/// "grokgrok" is a working title. Kept out of `grokgrok-core`.
/// Consumed by TASK-016 initialize; unused in this stub.
#[allow(dead_code)]
const USER_AGENT: &str = "grokgrok/app-server-v1";

const USAGE: &str = "\
Usage:
  grokgrok version
  grokgrok worker <start|wait|status|result|cancel|list> ...
  grokgrok mcp
  grokgrok app-server --listen unix://<absolute-path> [--home <absolute-path>]
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
    match args[0] {
        "version" | "--version" => {
            write_all(stdout, "grokgrok/dev\n");
            0
        }
        "worker" | "mcp" | "app-server" => {
            write_all(
                stderr,
                &format!("GROKGROK_STARTUP_ERROR NOT_IMPLEMENTED {}\n", args[0]),
            );
            1
        }
        other => {
            write_all(stderr, "GROKGROK_STARTUP_ERROR INVALID_CONFIG\n");
            if !other.starts_with('-') {
                write_all(stderr, USAGE);
            }
            1
        }
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
        assert_eq!(USER_AGENT, "grokgrok/app-server-v1");
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
        assert_eq!(String::from_utf8_lossy(&stdout).trim(), "grokgrok/dev");
        assert!(
            stderr.is_empty(),
            "stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
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
            String::from_utf8_lossy(&stderr).contains("grokgrok version"),
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
            String::from_utf8_lossy(&stderr).contains("GROKGROK_STARTUP_ERROR INVALID_CONFIG"),
            "stderr {:?}",
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn unimplemented_surfaces() {
        for cmd in ["worker", "mcp", "app-server"] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = run(&[cmd], &mut stdout, &mut stderr);
            assert_eq!(code, 1, "{cmd}: exit {code}");
            let err = String::from_utf8_lossy(&stderr);
            assert!(
                err.contains(&format!("GROKGROK_STARTUP_ERROR NOT_IMPLEMENTED {cmd}")),
                "{cmd}: stderr {err:?}"
            );
        }
    }
}
