# Testing

## Contract

The root `Makefile` owns test orchestration. The executable handlers in the
`Makefile` are authoritative; disagreement between this document and the
`Makefile` is a blocking contract defect.

The tree is a Cargo workspace (`crates/core`, `crates/docscheck`, `crates/samchi-for-grok`, `crates/adapter-grok`). `make test`
runs `cargo fmt --check` (must not rewrite), clippy, docscheck, and
`cargo test`. Subset digest and vocabulary tests live in the core crate.
The TASK-006 disk ledger (home resolution, admit/lock, atomic terminal
publish, waiter wakeup, generation liveness, `client_request_id` dedup) is
tested in `samchi-core`, including two-process lock and owner-death helpers.
The adapter crate's fake ACP agent, stdio harness, launch planner, and
`session/update` mapper live in `crates/adapter-grok`. TASK-007's live
`grok agent stdio` file-edit turn is an ignored test (`live_edit`); `make test`
still does not spawn live Grok. TASK-005's go ADR is checked against that
capture parser.

## Canonical commands

| Stage | Command |
| --- | --- |
| Aggregate | `make test` — runs `test-prepare` then `test-unit`, stopping on the first failure |
| Prepare | `make test-prepare` — `fmt-check`, `test-docs`, `clippy` in order |
| Unit | `make test-unit` |

Focused lane: `cargo test -p samchi-core`, `cargo test -p docscheck`, `cargo test -p samchi-for-grok`, or `cargo test -p samchi-adapter-grok` for one
package. CLI worker and MCP stdio tests in `samchi-for-grok` drive the fake
ACP agent via `SAMCHI_FOR_GROK_ACP_PROGRAM`. There is no aggregate integration or e2e target
yet; later Tasks add those atomically with the behavior they prove. Live Grok
is not spawned here.
Ledger process helpers in `crates/core/tests/ledger_processes.rs` spawn the
test binary and dummy `sleep` children only.

## Stage mapping

| Stage | Concrete checks |
| --- | --- |
| prepare | `cargo fmt --all -- --check` (fails on unformatted files and never rewrites), `cargo run -p docscheck --bin docscheck -q` (roadmap identity, statuses, one active task), `cargo clippy --workspace --all-targets -- -D warnings` |
| unit | `cargo test --workspace` |

`make test` does not spawn live Grok. TASK-004's checked-in `grok agent stdio`
capture is parsed offline in `crates/adapter-grok`. TASK-007 launch fail-closed
and mapping tests run in `samchi-adapter-grok`; the live file-edit is
`cargo test -p samchi-adapter-grok --test live_edit -- --ignored`.
TASK-010 host packaging is checked offline in `samchi-for-grok` (`host_packaging`).
The live MCP/Claude/Codex spawn→await file-edit is
`cargo test -p samchi-for-grok --test live_host -- --ignored`.
That live turn uses an isolated disposable workspace and does not edit user
host config. TASK-011 live cancel is
`cargo test -p samchi-for-grok --test live_cancel -- --ignored`.
An in-flight prompt dies and the turn is `interrupted`. Host observer timeout
of await is not cancel. TASK-012 live follow-up is
`cargo test -p samchi-for-grok --test live_followup -- --ignored`.
The second turn uses the same ACP session id.
