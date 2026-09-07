# grokgrok Roadmap

Authority: Sole Phase, Epic, Task, dependency, execution-order, and lifecycle
status.

Language: English.

## Governance

Allowed statuses are `Planned`, `In Progress`, `In Review`, `Completed`,
`Blocked`, and `Deferred`.

```text
Planned -> In Progress -> In Review -> Completed
In Review -> In Progress
In Progress | In Review -> Blocked
Blocked -> In Progress
Planned | Blocked -> Deferred
Deferred -> Planned
```

At most one Task may be `In Progress` or `In Review`. Follow the dependency
order strictly. A completed Task is immutable; later changes use a new Task.
A Task completes only when its acceptance and required verification are
settled.

Epic status is derived from its Tasks. Phase status is derived from its Epics.
Each Epic has at most seven Tasks.

Do not start EPIC-003 until EPIC-002 records a go ADR. Do not fall back to
`grok -p` on a no-go. The TASK-005 ADR must mention `grok agent serve` and
`grok agent leader` and say why v1 still uses `grok agent stdio` (or why it
does not).

`grokgrok` is a working title. It is **Grok-only**. The core is structured so
a later Claude or zcode backend can reuse it; those adapters are not this
repo. Language: Rust (ADR-0001).

MCP v1 is a write-capable subagent (review and implementation). Default spawn
is `approvalPolicy=never` and `sandbox=workspace-write`. Optional gated
approvals are TASK-013, not the TASK-010 smoke.

## Current execution

Current Task: none.

Next eligible Task: `TASK-003` (EPIC-001).

## Phase index

| Phase | Outcome | Status | Epics |
| --- | --- | --- | --- |
| Phase 1 — Contract and feasibility | Test gate, internal thread/turn/item types, ACP go/no-go | `In Progress` | EPIC-001..EPIC-002 |
| Phase 2 — Claude/Codex v1 | Async worker + MCP, approvals, resume, crash | `Planned` | EPIC-003..EPIC-004 |
| Phase 3 — CCAS-shaped app-server | UDS wire on the same worker, five consumer scenarios | `Planned` | EPIC-005 |

## EPIC-001: Foundation

Status: `In Progress`

Depends on: —

The repo has build, test, and this roadmap. Pin the Codex 0.149.0 internal
types the worker will use. Do not implement the transport wire yet.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-001](#epic-001-foundation) | Go module, `make test`, docs/roadmap, AGENTS.md | `Completed` | — | `make test` passes an empty gate. This roadmap owns the table |
| [TASK-002](#epic-001-foundation) | Pin internal thread/turn/item types (0.149.0 subset) | `Completed` | TASK-001 | No ad hoc job JSON. Schema or Go types close the item allowlist |
| [TASK-020](#epic-001-foundation) | ACP session/update → item mapping spec | `Completed` | TASK-002 | [acp-item-mapping.md](../specs/acp-item-mapping.md) exists. Emit allowlist is ACP-projectable only |
| [TASK-021](#epic-001-foundation) | Replace Go skeleton with Rust core crate | `Completed` | TASK-020 | Cargo workspace: core crate + grokgrok binary stub. `make test` runs `cargo fmt --check` (no rewrite), clippy, test, and docscheck. Keep subset digest/vocab tests. TESTING.md and Makefile stay in agreement. No Grok/Claude/GLM SDKs in core |
| [TASK-003](#epic-001-foundation) | Stub ACP agent + test harness | `Planned` | TASK-021 | Fake agent reproduces session/new, prompt, and update **in Rust** |

## EPIC-002: ACP feasibility

Status: `Planned`

Depends on: EPIC-001

Do not pick among candidates. Decide from evidence whether ACP stdio can be
the worker.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-004](#epic-002-acp-feasibility) | Capture live `grok agent stdio` | `Planned` | TASK-003 | Repo capture: initialize (pin grok version, `--no-leader` or documented owner), session/new, prompt, update, **real file edit**, turn end. Unauthenticated is Blocked, not a bypass. Fork and full sandbox matrix are out of this capture |
| [TASK-005](#epic-002-acp-feasibility) | go/no-go ADR | `Planned` | TASK-004 | go → EPIC-003 only for MCP v1 capabilities actually observed. no-go → stop. Do not use `grok -p`. Record stdio vs `agent serve` / `leader`. Record whether `session/load` is advertised. `thread/fork` is a later EPIC-005 decision, not this ADR |

## EPIC-003: Async worker and MCP

Status: `Planned`

Depends on: EPIC-002 = go

The v1 product Claude/Codex attach. Domain ops are shared; **CLI and MCP
process lifetimes differ** (see [MCP async host contract](../specs/mcp-async-host-contract.md)).

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-006](#epic-003-async-worker-and-mcp) | Disk ledger (thread/turn/item, generation) | `Planned` | TASK-005 | Home is --home, GROKGROK_HOME, or ~/.grokgrok. One inProgress turn per thread, lock, atomic terminal publish, waiter wakeup. grokgrok pid and Grok child/ACP EOF both resolve waiters. Dead generation → failed worker_gone unless a terminal record already exists. Optional client_request_id dedup |
| [TASK-007](#epic-003-async-worker-and-mcp) | ACP adapter: spawn grok agent, update → item | `Planned` | TASK-006, TASK-020 | Follows acp-item-mapping.md and grok-launch.md. No early complete on in_progress tool updates. live turn edits a file. Unenforceable sandbox/approval rejected |
| [TASK-008](#epic-003-async-worker-and-mcp) | CLI worker start (owns process), wait, status, result, list | `Planned` | TASK-007 | `--json`. start prints ids then stays until terminal. No CLI daemon. wait 50s does not cancel. Usage matches published verbs only |
| [TASK-009](#epic-003-async-worker-and-mcp) | `grokgrok mcp` stdio; six tools | `Planned` | TASK-008 | tools/list is spawn, await, wait, status, result, list. spawn→await with default never + workspace-write. untrusted spawn rejected. host timeout of await does not cancel the turn |
| [TASK-010](#epic-003-async-worker-and-mcp) | Host packaging: Codex/Claude config + use-grokgrok skill | `Planned` | TASK-009 | skill matches the async host contract. Codex tool_timeout_sec = 3600. Exercise each available host; record which. spawn→await performs an edit. Observer timeout ≠ grok_cancel |

## EPIC-004: Supervision

Status: `Planned`

Depends on: EPIC-003

Make the worker supervisable. No app-server socket yet. MCP already has a
long-lived server process; that is not a Dolgorae unix socket.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-011](#epic-004-supervision) | cancel / interrupt, process-group teardown | `Planned` | TASK-010 | in-flight prompt dies and the turn is cancelled |
| [TASK-012](#epic-004-supervision) | follow-up: session/load + new turn | `Planned` | TASK-011 | second turn same ACP session. load replay is history, not new items or approvals. fail closed if session/load was not advertised |
| [TASK-013](#epic-004-supervision) | Optional grok_respond for untrusted/on-request | `Planned` | TASK-012 | untrusted does not run a shell without grok_respond. await returns pending_approval (not a TurnStatus) then the parent awaits again |
| [TASK-014](#epic-004-supervision) | Crash: no input replay; resume is observation only | `Planned` | TASK-013 | killing the worker does not auto-replay the same turn |

## EPIC-005: App-server wire

Status: `Planned`

Depends on: EPIC-004

Layer the CCAS standard wire on the same worker. No new runtime.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-015](#epic-005-app-server-wire) | app-server --listen unix://… [--home], UDS websocket, occupied path | `Planned` | TASK-014 | This repo records the HTTP/WS rejection table (copy from CCAS/Dolgorae constants). Occupied path fail-closed |
| [TASK-016](#epic-005-app-server-wire) | initialize / initialized / account/read / Grok model/list | `Planned` | TASK-015 | userAgent=grokgrok/app-server-v1. capabilities.ccas is -32602 |
| [TASK-017](#epic-005-app-server-wire) | nine thread/turn methods call the worker | `Planned` | TASK-016 | socket thread/start+turn/start writes the same ledger as MCP spawn |
| [TASK-018](#epic-005-app-server-wire) | server notifications + requestApproval on the socket | `Planned` | TASK-017 | item/started, completed, turn/completed. approvals are socket server requests |
| [TASK-019](#epic-005-app-server-wire) | five consumer scenarios + grok/runtime/read | `Planned` | TASK-018 | Named scenarios and grok/runtime/read shape live in this repo. Dolgorae-shaped client passes. thread/fork only if TASK-005 leftover or a new capture proved it. Not a Dolgorae Profile integration |
