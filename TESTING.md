# Testing

## Contract

The root `Makefile` owns test orchestration. The executable handlers in the
`Makefile` are authoritative; disagreement between this document and the
`Makefile` is a blocking contract defect.

The tree is a Cargo workspace (`crates/core`, `crates/docscheck`, `crates/grokgrok`, `crates/adapter-grok`). `make test`
runs `cargo fmt --check` (must not rewrite), clippy, docscheck, and
`cargo test`. Subset digest and vocabulary tests live in the core crate.
The adapter crate's fake ACP agent and stdio harness live in `crates/adapter-grok`.

## Canonical commands

| Stage | Command |
| --- | --- |
| Aggregate | `make test` — runs `test-prepare` then `test-unit`, stopping on the first failure |
| Prepare | `make test-prepare` — `fmt-check`, `test-docs`, `clippy` in order |
| Unit | `make test-unit` |

Focused lane: `cargo test -p grokgrok-core`, `cargo test -p docscheck`, `cargo test -p grokgrok`, or `cargo test -p grokgrok-adapter-grok` for one
package. There is no aggregate integration or e2e target yet; later Tasks add
those atomically with the behavior they prove. Live Grok is TASK-004.

## Stage mapping

| Stage | Concrete checks |
| --- | --- |
| prepare | `cargo fmt --all -- --check` (fails on unformatted files and never rewrites), `cargo run -p docscheck --bin docscheck -q` (roadmap identity, statuses, one active task), `cargo clippy --workspace --all-targets -- -D warnings` |
| unit | `cargo test --workspace` |

Live Grok capture belongs to TASK-004 and is not part of this gate.
