# Testing

## Contract

The root `Makefile` owns test orchestration. The executable handlers in the
`Makefile` are authoritative; disagreement between this document and the
`Makefile` is a blocking contract defect.

The tree is a Cargo workspace (`crates/core`, `crates/docscheck`, `crates/samchi-for-grok`, `crates/adapter-grok`). `make test`
runs `cargo fmt --check` (must not rewrite), clippy, docscheck, and
`cargo test`. Subset digest and vocabulary tests live in the core crate.
The adapter crate's fake ACP agent and stdio harness live in `crates/adapter-grok`.
TASK-005's go ADR is checked against that capture parser; `make test` still
does not spawn live Grok.

## Canonical commands

| Stage | Command |
| --- | --- |
| Aggregate | `make test` — runs `test-prepare` then `test-unit`, stopping on the first failure |
| Prepare | `make test-prepare` — `fmt-check`, `test-docs`, `clippy` in order |
| Unit | `make test-unit` |

Focused lane: `cargo test -p samchi-core`, `cargo test -p docscheck`, `cargo test -p samchi-for-grok`, or `cargo test -p samchi-adapter-grok` for one
package. There is no aggregate integration or e2e target yet; later Tasks add
those atomically with the behavior they prove. Live Grok is not spawned here.

## Stage mapping

| Stage | Concrete checks |
| --- | --- |
| prepare | `cargo fmt --all -- --check` (fails on unformatted files and never rewrites), `cargo run -p docscheck --bin docscheck -q` (roadmap identity, statuses, one active task), `cargo clippy --workspace --all-targets -- -D warnings` |
| unit | `cargo test --workspace` |

`make test` does not spawn live Grok. TASK-004's checked-in `grok agent stdio`
capture is parsed offline in `crates/adapter-grok`.
