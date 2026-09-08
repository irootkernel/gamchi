# EPIC-003 execution dossier

Language: English.

Status: Temporary execution SOT for EPIC-003. This file does not own roadmap
identity, ordering, dependencies, lifecycle vocabulary, or status.

Consumer epics: `EPIC-003` only.

## Goal

Claude Code and Codex attach to samchi-for-grok as a write-capable Grok
subagent. Canonical goal: [product.md](../specs/product.md) (v1 done when
TASK-010) and [roadmap EPIC-003](../roadmap/README.md).

## Non-goals

Owned by [product.md](../specs/product.md) exclusions and later epics:

- EPIC-004 supervision (cancel, follow-up, respond, crash)
- EPIC-005 app-server wire
- `grok -p`, Claude/GLM adapters, Dolgorae Profile integration
- `grok agent serve` / `grok agent leader` as v1 owner
  ([ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md))

## Requirement owners

| Topic | Owner |
| --- | --- |
| Identity, order, lifecycle | [roadmap](../roadmap/README.md) |
| Product scope and MCP defaults | [product.md](../specs/product.md) |
| CLI vs MCP lifetimes, tools, await | [mcp-async-host-contract.md](../specs/mcp-async-host-contract.md) |
| Launch flags, fail-closed sandbox/approval, `files_changed` | [grok-launch.md](../specs/grok-launch.md) |
| ACP update → items | [acp-item-mapping.md](../specs/acp-item-mapping.md) |
| Core vs adapter split | [architecture/core.md](../architecture/core.md) |
| v1 stdio go and observed capabilities | [ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md) |

Do not copy those documents here. If they disagree, they win; this dossier
only maps them.

## Member tasks

Order: TASK-006 (Completed) → TASK-007 → TASK-008 → TASK-009 → TASK-010.
TASK-007 also depends on TASK-020 (Completed).

| Task | Runtime owner | Verification | Must not |
| --- | --- | --- | --- |
| TASK-006 | `samchi-core` ledger (already shipped) | `make test` offline | Spawn Grok; publish CLI/MCP |
| TASK-007 | `samchi-adapter-grok` spawn + ACP map; core stays SDK-free | Offline mapping tests in `make test`; **live** turn that edits a file is required evidence outside `make test` (`cargo test -p samchi-adapter-grok --test live_edit -- --ignored`) | Early-complete `in_progress` tools; `grok -p`; publish CLI/MCP verbs |
| TASK-008 | `samchi-for-grok` CLI facade | CLI `--json` tests; `wait` timeout does not cancel | CLI daemon; detached await |
| TASK-009 | MCP facade; six tools | tools/list + spawn→await; untrusted rejected; host timeout of await does not cancel | Publish cancel/follow-up/respond; MCP push as completion |
| TASK-010 | Host skill + config snippets | Exercise each available host; record which; spawn→await edits a file | Silently edit user host config; treat observer timeout as `grok_cancel` |

## Cross-document acceptance

EPIC-003 is ready for audit when TASK-006..010 are successfully terminal and:

1. Default spawn is `approvalPolicy=never` and `sandbox=workspace-write`.
2. Completion is a terminal TurnStatus returned by `grok_await` (MCP) or by
   foreground CLI `start`. Host timeout does not cancel the turn.
3. Adapter emit allowlist matches [acp-item-mapping.md](../specs/acp-item-mapping.md).
4. Unenforceable sandbox or approval is rejected before the Grok child starts.
5. A live host or adapter turn edits a file (TASK-007 and TASK-010).

## Execution constraints

- Launch remains parent-owned `grok agent stdio` with `--no-leader`.
- `approvalPolicy=never` maps to `--always-approve` (observed in TASK-004).
- `sandbox=workspace-write` maps to top-level `grok --sandbox workspace`
  before `agent` (`grok agent --sandbox` is rejected by grok 1.0.13). If that
  profile cannot be shown to allow cwd writes as documented, refuse spawn.
  Do not treat the TASK-004 argv (no `--sandbox`) as proof of enforcement.
- `untrusted` / `on-request` stay rejected until TASK-013.
- Extra subset fields (`writableRoots`, `networkAccess`, and siblings) stay
  fail-closed when the parent sends them; v1 MCP spawn omits them.
- `make test` stays offline. Live Grok is TASK-007/TASK-010 evidence, not the
  aggregate gate. That live turn uses an isolated disposable workspace and a
  named file; it must not mutate this repository's dirty worktree or unrelated
  user files.
- CLI `start` stays in the foreground; MCP spawn may return because the server
  process owns the child.

## Closeout

This dossier has one consumer. At EPIC-003 closeout, promote any durable
behavior that landed in specs or architecture, replace this epic's
`Detailed SOT` link with Canonical Outcomes, then delete this file and its
TODO index entry.
