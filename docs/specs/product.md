# Product (repository baseline)

Language: English. This is the in-repo product authority. Session plans are
not required to implement.

## What it is

**gamchi** is a **Grok-only** local worker. Parents
delegate review *or* implementation. A later Claude or zcode backend may
reuse the core’s shape; those adapters are not this product.

## Parents and facades

| Parent | Facade | v1 |
| --- | --- | --- |
| Claude Code, Codex | MCP (Gaori-shaped spawn/await) | yes |
| Dolgorae | `app-server --listen unix://` | after the worker (EPIC-005). Five named Dolgorae-shaped scenarios (`probe`, `first-turn`, `follow-up`, `approval`, `interrupt`) plus `grok/runtime/read`. Not a Dolgorae Profile. |
| Human / tests | CLI | yes, same domain ops, different process lifetime |

## v1 done when

TASK-010 is accepted: one real host spawn→await that **edits a file**, with
the skill and Codex `tool_timeout_sec = 3600` snippet. EPIC-004/005 are
beyond that milestone.

## Defaults (MCP)

`approvalPolicy=never`, `sandbox=workspace-write`. Write-capable, not
read-only review.

Omitted `model` and `effort` on MCP spawn, CLI `worker start`, and
app-server `thread/start` resolve in [grok-launch.md](grok-launch.md):
explicit parent field, then home `config.yaml` keys `default_model` and
`default_effort`, then built-in `grok-4.6` / `high`. Gamchi does not
inherit `~/.grok/config.toml`. There is no project-local config.

App-server `developerInstructions` follow [grok-launch.md](grok-launch.md).
Omitted or empty stays the no-instruction path. A non-empty value is
applied or refused; silent ignore is invalid.

## Exclusions

- `grok -p` print mode
- Claude or GLM adapters in this binary
- Importing CCAS or Dolgorae
- Claiming a Dolgorae Profile can select gamchi (Dolgorae change)
- MCP push as the completion signal
- Detached daemon / `grok agent leader` as v1 owner
  ([ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md):
  parent-owned `grok agent stdio` with `--no-leader`)
- Pre-admitting or rejecting spawn by parsing `grok models`
- Project-local / cwd model config, or inheriting `~/.grok/config.toml`
- Silent drop of app-server `developerInstructions` (apply or refuse)

## Supported environments (intent)

Apple Silicon macOS, local `grok` login. Network sandbox on macOS is not
claimed; see [grok-launch.md](grok-launch.md).
