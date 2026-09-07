# Product (repository baseline)

Language: English. This is the in-repo product authority. Session plans are
not required to implement.

## What it is

**samchi-for-grok** is a **Grok-only** local worker. Parents
delegate review *or* implementation. A later Claude or zcode backend may
reuse the core’s shape; those adapters are not this product.

## Parents and facades

| Parent | Facade | v1 |
| --- | --- | --- |
| Claude Code, Codex | MCP (Gaori-shaped spawn/await) | yes |
| Dolgorae | `app-server --listen unix://` | after the worker (EPIC-005) |
| Human / tests | CLI | yes, same domain ops, different process lifetime |

## v1 done when

TASK-010 is accepted: one real host spawn→await that **edits a file**, with
the skill and Codex `tool_timeout_sec = 3600` snippet. EPIC-004/005 are
beyond that milestone.

## Defaults (MCP)

`approvalPolicy=never`, `sandbox=workspace-write`. Write-capable, not
read-only review.

## Exclusions

- `grok -p` print mode
- Claude or GLM adapters in this binary
- Importing CCAS or Dolgorae
- Claiming a Dolgorae Profile can select samchi-for-grok (Dolgorae change)
- MCP push as the completion signal
- Detached daemon / `grok agent leader` as v1 owner (TASK-005 records why)

## Supported environments (intent)

Apple Silicon macOS, local `grok` login. Network sandbox on macOS is not
claimed; see [grok-launch.md](grok-launch.md).
