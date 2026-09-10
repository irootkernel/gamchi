# ADR-0002: Live grok agent stdio is the v1 Grok worker

- Status: Accepted
- Decided: 2026-09-08
- Evidence: [TASK-004 capture](../../crates/adapter-grok/captures/task-004)
  (`grok agent --no-leader --always-approve stdio`, grok 1.0.13). Not the
  TASK-003 fake agent, not a new live spawn, and not `grok -p`.

## Decision

**go.** The v1 Grok worker is parent-owned `grok agent stdio` launched with
`--no-leader`. EPIC-003 may start for the observed MCP v1 capabilities listed
below. A no-go path and a `grok -p` fallback are not used.

## Context

TASK-004 checked in an offline ACP capture of a live `grok agent stdio` child.
That capture already shows initialize, session/new, session/prompt,
session/update, a real file edit (`kind` `edit` plus `fs/write_text_file`),
and turn end (`stopReason` `end_turn`). The child argv is
`grok agent --no-leader --always-approve --reasoning-effort low stdio`. The
capture parent owned that process because of `--no-leader`.

This ADR decides from that capture only.

## Launch candidates

These three commands are named so the choice is explicit:

- `grok agent stdio` — chosen for v1. The gamchi process (MCP server
  or CLI `start`) spawns the child and owns it. `--no-leader` keeps that
  parent as the owner.
- `grok agent serve` — not the v1 owner. Not captured. Unobserved as an
  owner.
- `grok agent leader` — not the v1 owner. Not captured. A detached leader
  would not leave this process as the owner of the ACP child.

v1 still uses `grok agent stdio` because the only captured owner is a
parent-owned `--no-leader` stdio child.

## session/load

`session/load` is **advertised, not invoked**. The initialize result sets
`agentCapabilities.loadSession` to true, matching capture metadata
`load_session_advertised`. Traffic contains no `session/load` method.
Advertisement is enough for TASK-012 to attempt follow-up; invocation is
not proven here. If a later agent does not advertise `loadSession`,
TASK-012 fails closed.

## thread/fork

`thread/fork` is **not decided** here. It is an EPIC-005 / TASK-019
question. This capture does not prove it.

## EPIC-003 in-scope (observed)

EPIC-003 is unblocked only for:

- parent-owned `grok agent stdio` with `--no-leader`
- `initialize`
- `session/new`
- `session/prompt`
- `session/update`
- edit/write file change (`kind` `edit`, `fs/write_text_file`)
- turn end (`end_turn`)
- `--always-approve`

## Not proven (unobserved)

These are not treated as proven and are not in the EPIC-003 go:

- `session/load` invocation
- sandbox matrix (`--sandbox` was not on the captured argv)
- `grok agent serve` as owner
- `grok agent leader` as owner
- `thread/fork`

## Rejected alternatives

- **no-go / stop.** Rejected: the capture shows a complete ACP turn with a
  real file write on live `grok agent stdio`.
- **`grok -p` fallback.** Rejected: print mode is out of product scope and
  is not a substitute for ACP stdio.

## Consequences

- EPIC-003 (TASK-006 onward) may start, bound to the in-scope list.
- Launch remains `grok agent --no-leader stdio` (plus `--always-approve` when
  `approvalPolicy=never`).
- `grok agent serve` and `grok agent leader` stay out of v1 ownership.
- TASK-012 may use advertised `session/load`; it must fail closed if a later
  initialize omits `loadSession`.
- `thread/fork` waits for EPIC-005 evidence.
