# Testing

## Contract

The root `Makefile` owns test orchestration. The executable handlers in the
`Makefile` are authoritative; disagreement between this document and the
`Makefile` is a blocking contract defect.

Today the tree is a Go prototype. **TASK-021** must rewrite this file and the
Makefile together: `cargo fmt --check` (must not rewrite in the gate),
clippy, `cargo test`, and docscheck. Subset digest and vocabulary tests
move with the core crate. Do not leave TESTING.md describing Go after the
Makefile runs Cargo.

## Canonical commands

| Stage | Command |
| --- | --- |
| Aggregate | `make test` — runs `test-prepare` then `test-unit`, stopping on the first failure |
| Prepare | `make test-prepare` — `fmt-check`, `test-docs`, `vet` in order |
| Unit | `make test-unit` |

Focused lane: `go test ./internal/<pkg>/...` for one package. There is no
aggregate integration or e2e target yet; later Tasks add those atomically
with the behavior they prove.

## Stage mapping

| Stage | Concrete checks |
| --- | --- |
| prepare | `gofmt -l` on `cmd`, `internal`, and `scripts` (fails on any unformatted file and never rewrites), `go run ./scripts/docscheck` (roadmap identity, statuses, one active task), `go vet ./...` |
| unit | `go test ./...` |

Live Grok capture belongs to TASK-004 and is not part of this empty gate.
