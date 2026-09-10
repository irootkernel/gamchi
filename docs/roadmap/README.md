# gamchi Roadmap

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

EPIC-002 recorded a go
([ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md)).
EPIC-003 is unblocked only for capabilities that ADR lists as observed.
Do not fall back to `grok -p`. v1 uses parent-owned `grok agent stdio` with
`--no-leader`; `grok agent serve` and `grok agent leader` are not the v1
owner.

`gamchi` is **Grok-only**. The core is structured so a later Claude
or zcode backend can reuse it; those adapters are not this repo. Language:
Rust (ADR-0001).

MCP v1 is a write-capable subagent (review and implementation). Default spawn
is `approvalPolicy=never` and `sandbox=workspace-write`. Optional gated
approvals are TASK-013, not the TASK-010 smoke.

## Current execution

Current Task: none.

Next eligible Task: `TASK-029`.

## Phase index

| Phase | Outcome | Status | Epics |
| --- | --- | --- | --- |
| Phase 1 — Contract and feasibility | Test gate, internal thread/turn/item types, ACP go/no-go | `Completed` | EPIC-001..EPIC-002 |
| Phase 2 — Claude/Codex v1 | Async worker + MCP, approvals, resume, crash | `Completed` | EPIC-003..EPIC-004 |
| Phase 3 — CCAS-shaped app-server | UDS wire on the same worker, five consumer scenarios | `Completed` | EPIC-005 |
| Phase 4 — Selectable Grok model | Parents choose model and reasoning effort on the same worker | `Completed` | EPIC-006 |
| Phase 5 — Product identity | Grok worker command is gamchi | `Completed` | EPIC-007 |
| Phase 6 — Codex-shaped developer instructions | Socket `developerInstructions` either reach Grok for that thread generation or fail closed | `Planned` | EPIC-008 |

## EPIC-001: Foundation

Status: `Completed`

Depends on: —

The repo has build, test, and this roadmap. Pin the Codex 0.149.0 internal
types the worker will use. Do not implement the transport wire yet.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-001](#epic-001-foundation) | Go module, `make test`, docs/roadmap, AGENTS.md | `Completed` | — | `make test` passes an empty gate. This roadmap owns the table |
| [TASK-002](#epic-001-foundation) | Pin internal thread/turn/item types (0.149.0 subset) | `Completed` | TASK-001 | No ad hoc job JSON. Schema or Go types close the item allowlist |
| [TASK-020](#epic-001-foundation) | ACP session/update → item mapping spec | `Completed` | TASK-002 | [acp-item-mapping.md](../specs/acp-item-mapping.md) exists. Emit allowlist is ACP-projectable only |
| [TASK-021](#epic-001-foundation) | Replace Go skeleton with Rust core crate | `Completed` | TASK-020 | Cargo workspace: core crate + grokgrok binary stub. `make test` runs `cargo fmt --check` (no rewrite), clippy, test, and docscheck. Keep subset digest/vocab tests. TESTING.md and Makefile stay in agreement. No Grok/Claude/GLM SDKs in core |
| [TASK-003](#epic-001-foundation) | Stub ACP agent + test harness | `Completed` | TASK-021 | Fake agent reproduces session/new, prompt, and update **in Rust** |
| [TASK-022](#epic-001-foundation) | Rename product identity to samchi-for-grok | `Completed` | TASK-003 | Binary, crates, userAgent, home, and docs use `samchi-for-grok`. README title is Samchi for Grok. `ggwire` is `source_wire`. `make test` passes. Completed task Done when strings stay historical |

## EPIC-002: ACP feasibility

Status: `Completed`

Depends on: EPIC-001

Decided from the TASK-004 capture: live `grok agent stdio` is the v1 worker
([ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md)).

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-004](#epic-002-acp-feasibility) | Capture live `grok agent stdio` | `Completed` | TASK-003 | Repo capture: initialize (pin grok version, `--no-leader` or documented owner), session/new, prompt, update, **real file edit**, turn end. Unauthenticated is Blocked, not a bypass. Fork and full sandbox matrix are out of this capture |
| [TASK-005](#epic-002-acp-feasibility) | go/no-go ADR | `Completed` | TASK-004 | go → EPIC-003 only for MCP v1 capabilities actually observed. no-go → stop. Do not use `grok -p`. Record stdio vs `agent serve` / `leader`. Record whether `session/load` is advertised. `thread/fork` is a later EPIC-005 decision, not this ADR |

## EPIC-003: Async worker and MCP

Status: `Completed`

Depends on: EPIC-002 = go

Canonical Outcomes: default spawn `never` + `workspace-write`
([product.md](../specs/product.md),
[mcp-async-host-contract.md](../specs/mcp-async-host-contract.md));
CLI vs MCP lifetimes and six tools
([mcp-async-host-contract.md](../specs/mcp-async-host-contract.md));
launch fail-closed and cwd-confined ACP fs
([grok-launch.md](../specs/grok-launch.md));
ACP mapping ([acp-item-mapping.md](../specs/acp-item-mapping.md));
live file-edit evidence ([TESTING.md](../../TESTING.md),
[host-exercise.md](../ops/host-exercise.md)).

The v1 product Claude/Codex attach. Domain ops are shared; **CLI and MCP
process lifetimes differ** (see [MCP async host contract](../specs/mcp-async-host-contract.md)).

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-006](#epic-003-async-worker-and-mcp) | Disk ledger (thread/turn/item, generation) | `Completed` | TASK-005 | Home is --home, SAMCHI_FOR_GROK_HOME, or ~/.samchi-for-grok. One inProgress turn per thread, lock, atomic terminal publish, waiter wakeup. samchi-for-grok pid and Grok child/ACP EOF both resolve waiters. Dead generation → failed worker_gone unless a terminal record already exists. Optional client_request_id dedup |
| [TASK-007](#epic-003-async-worker-and-mcp) | ACP adapter: spawn grok agent, update → item | `Completed` | TASK-006, TASK-020 | Follows acp-item-mapping.md and grok-launch.md. No early complete on in_progress tool updates. live turn edits a file. Unenforceable sandbox/approval rejected |
| [TASK-008](#epic-003-async-worker-and-mcp) | CLI worker start (owns process), wait, status, result, list | `Completed` | TASK-007 | `--json`. start prints ids then stays until terminal. No CLI daemon. wait 50s does not cancel. Usage matches published verbs only |
| [TASK-009](#epic-003-async-worker-and-mcp) | `samchi-for-grok mcp` stdio; six tools | `Completed` | TASK-008 | tools/list is spawn, await, wait, status, result, list. spawn→await with default never + workspace-write. untrusted spawn rejected. host timeout of await does not cancel the turn |
| [TASK-010](#epic-003-async-worker-and-mcp) | Host packaging: Codex/Claude config + use-samchi-for-grok skill | `Completed` | TASK-009 | skill matches the async host contract. Codex tool_timeout_sec = 3600. Exercise each available host; record which. spawn→await performs an edit. Observer timeout ≠ grok_cancel |

## EPIC-004: Supervision

Status: `Completed`

Depends on: EPIC-003

Canonical Outcomes: cancel / process-group teardown
([mcp-async-host-contract.md](../specs/mcp-async-host-contract.md));
follow-up `session/load` history
([acp-item-mapping.md](../specs/acp-item-mapping.md),
[mcp-async-host-contract.md](../specs/mcp-async-host-contract.md));
optional `grok_respond` / `pending_approval`
([mcp-async-host-contract.md](../specs/mcp-async-host-contract.md),
[grok-launch.md](../specs/grok-launch.md));
crash is observation-only `worker_gone`
([mcp-async-host-contract.md](../specs/mcp-async-host-contract.md)).

Make the worker supervisable. No app-server socket yet. MCP already has a
long-lived server process; that is not a Dolgorae unix socket.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-011](#epic-004-supervision) | cancel / interrupt, process-group teardown | `Completed` | TASK-010 | in-flight prompt dies and the turn is cancelled |
| [TASK-012](#epic-004-supervision) | follow-up: session/load + new turn | `Completed` | TASK-011 | second turn same ACP session. load replay is history, not new items or approvals. fail closed if session/load was not advertised |
| [TASK-013](#epic-004-supervision) | Optional grok_respond for untrusted/on-request | `Completed` | TASK-012 | untrusted does not run a shell without grok_respond. await returns pending_approval (not a TurnStatus) then the parent awaits again |
| [TASK-014](#epic-004-supervision) | Crash: no input replay; resume is observation only | `Completed` | TASK-013 | killing the worker does not auto-replay the same turn |

## EPIC-005: App-server wire

Status: `Completed`

Depends on: EPIC-004

Canonical Outcomes: `app-server --listen unix://` HTTP/WS upgrade and occupied-path fail-closed
([protocol/README.md](../protocol/README.md));
honest `userAgent=gamchi/app-server-v1` and `capabilities.ccas` `-32602`
([architecture/core.md](../architecture/core.md));
same-ledger thread/turn with omitted socket `read-only`/`untrusted`
([mcp-async-host-contract.md](../specs/mcp-async-host-contract.md));
notifications and socket `requestApproval`
([acp-item-mapping.md](../specs/acp-item-mapping.md));
five named Dolgorae-shaped scenarios and `grok/runtime/read`
([protocol/README.md](../protocol/README.md)).

Layer the CCAS standard wire on the same worker. No new runtime.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-015](#epic-005-app-server-wire) | app-server --listen unix://… [--home], UDS websocket, occupied path | `Completed` | TASK-014 | This repo records the HTTP/WS rejection table (copy from CCAS/Dolgorae constants). Occupied path fail-closed |
| [TASK-016](#epic-005-app-server-wire) | initialize / initialized / account/read / Grok model/list | `Completed` | TASK-015 | userAgent=samchi-for-grok/app-server-v1. capabilities.ccas is -32602 |
| [TASK-017](#epic-005-app-server-wire) | nine thread/turn methods call the worker | `Completed` | TASK-016 | socket thread/start+turn/start writes the same ledger as MCP spawn |
| [TASK-018](#epic-005-app-server-wire) | server notifications + requestApproval on the socket | `Completed` | TASK-017 | item/started, completed, turn/completed. approvals are socket server requests |
| [TASK-019](#epic-005-app-server-wire) | five consumer scenarios + grok/runtime/read | `Completed` | TASK-018 | Named scenarios and grok/runtime/read shape live in this repo. Dolgorae-shaped client passes. thread/fork only if a new capture proved it (ADR-0002 did not decide it). Not a Dolgorae Profile integration |

## EPIC-006: Selectable Grok model

Status: `Completed`

Depends on: EPIC-005

Canonical Outcomes: home `config.yaml` cascade and argv `-m` / `--reasoning-effort`
([grok-launch.md](../specs/grok-launch.md));
MCP/CLI spawn and follow-up `model` / `effort` with thread model lock
([mcp-async-host-contract.md](../specs/mcp-async-host-contract.md));
app-server `model/list` advertisement, omitted `thread/start` cascade, and
`turn/start` model lock ([architecture/core.md](../architecture/core.md));
ignored live argv proof ([TESTING.md](../../TESTING.md)).

Parents choose a Grok model and reasoning effort when they spawn. Omitted
fields resolve from home `config.yaml`, then built-in `grok-4.6` / `high`,
and are placed on the Grok argv. Gamchi returns an error if the child
does not start. A thread freezes its model; later turns may change
effort only. Do not rewrite completed Tasks.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-023](#epic-006-selectable-grok-model) | Spec model/effort launch, home YAML config, pass-through, and thread lock | `Completed` | TASK-019 | grok-launch.md maps `-m` and `--reasoning-effort`, home `config.yaml` keys `default_model` and `default_effort`, blank-vs-absent, `grok` alias, empty stored effort, child-start failure, and thread lock. mcp-async-host-contract spawn/followup fields. product.md omitted defaults. Dossier remains the execution map |
| [TASK-024](#epic-006-selectable-grok-model) | Home `config.yaml` plus argv `-m`/`--reasoning-effort`; Grok child failure is the error | `Completed` | TASK-023 | Reads home `config.yaml` keys `default_model` and `default_effort`, then built-in grok-4.6/high. Absent key skips; blank value refuses. `grok` normalizes to grok-4.6. Empty stored effort is omitted. Launch argv carries the pair. Child start/ACP failure fails the turn. No `grok models` spawn allowlist. No project-local config. Offline fake agent; `make test` does not call live Grok |
| [TASK-025](#epic-006-selectable-grok-model) | MCP/CLI spawn and follow-up fields plus skill | `Completed` | TASK-024 | grok_spawn, grok_followup, worker start, and worker followup accept model/effort. Follow-up model mismatch refuses. Skill recommends adding `.samchi-for-grok/` to `.gitignore` and does not edit that file |
| [TASK-026](#epic-006-selectable-grok-model) | App-server model/list and reject mid-thread model change | `Completed` | TASK-025 | model/list may list live Grok ids for advertisement, not as a spawn gate. Omitted thread/start uses the same cascade as MCP. turn/start model change fails closed. Effort may change |
| [TASK-027](#epic-006-selectable-grok-model) | Live proof of requested model/effort | `Completed` | TASK-026 | Ignored live test: requested pair is on the child argv and capture model_id. Follow-up effort change works. Follow-up model change refuses. Follow-up with empty stored effort uses high or home default_effort. Compilation is not live proof |

## EPIC-007: Product identity Gamchi

Status: `Completed`

Depends on: EPIC-006

Canonical Outcomes: live product identity is Gamchi / `gamchi`
([product.md](../specs/product.md), [architecture/core.md](../architecture/core.md));
hard home cutover
([grok-launch.md](../specs/grok-launch.md),
[mcp-async-host-contract.md](../specs/mcp-async-host-contract.md));
honest `userAgent=gamchi/app-server-v1`
([protocol/README.md](../protocol/README.md)).

The Grok worker is Gamchi. Every current runtime, diagnostic, and live-doc
identity uses `gamchi`: binary and facade crate path/package, Makefile,
CLI usage and `version --json`, ACP client name, fake-agent and stdio
harness names, `userAgent`, `GAMCHI_*` env (including `STARTUP_ERROR` and
test/launch fixtures), default home `~/.gamchi`, skill, MCP server key,
README title, live specs/architecture/protocol/ops/TESTING.md/AGENTS.md,
and accepted ADRs (in place; the decisions stay Rust / Grok-only).
`samchi-core` and `samchi-adapter-grok` stay. MCP tools stay `grok_*`.
The `fake-acp-agent` binary name stays. Zamchi and Camchi are
sibling-product names, not this repo.

Hard cutover: no `SAMCHI_FOR_GROK_*` env fallback; no auto-discover, copy,
or migrate of `~/.samchi-for-grok`. Default is `GAMCHI_HOME`, then
`~/.gamchi`. Explicit `--home` still accepts any absolute path, including
the old directory as an ordinary path.

Old-name allowlist (do not rewrite): completed Task rows in this roadmap;
`crates/adapter-grok/captures/task-004/**` bytes and literals that must
match them, including `samchi-for-grok-task-004-live-capture`; GitHub
remote and clone path. Live ignored-test markers that are not capture
bytes (including TASK-007 `live_edit`) change with the product. Do not
rewrite completed Tasks.

Verification is `make test`, `make build`, `./bin/gamchi version --json`,
home/env precedence, host packaging skill and config paths, and an
allowlisted leftover-name search. No live Grok rerun. No old-name aliases.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-028](#epic-007-product-identity-gamchi) | Rename product identity to Gamchi | `Completed` | TASK-027 | Live runtime/diagnostic/doc identity is `gamchi` (binary, facade crate, Makefile, CLI usage/version, ACP client name, fake-agent/harness names, userAgent, GAMCHI_* including STARTUP_ERROR, home, skill, MCP server key, live docs/ADRs/TESTING.md). `samchi-core` and `samchi-adapter-grok` stay. MCP tools stay `grok_*`. Hard cutover: no SAMCHI_FOR_GROK_* fallback, no ~/.samchi-for-grok auto-discover/copy/migrate; `--home` still accepts any absolute path. Allowlist: completed Task rows; task-004 capture bytes and matching literals; GitHub remote/clone path. `make test`, `make build`, `./bin/gamchi version --json`, home precedence, host packaging, allowlisted leftover search. No live Grok rerun. No old-name aliases |

## EPIC-008: Generation-immutable developer instructions

Status: `Planned`

Depends on: EPIC-007

Detailed SOT: [TODO-EPIC-008-developer-instructions.md](../todo/TODO-EPIC-008-developer-instructions.md)

Canonical owners while Planned: this roadmap;
[grok-launch.md](../specs/grok-launch.md);
[acp-item-mapping.md](../specs/acp-item-mapping.md).

A Dolgorae-shaped app-server parent assigns role/purpose with subset
`developerInstructions` on `thread/start`. Gamchi either installs that
text so Grok follows it for the life of that Gamchi thread, or refuses a
non-empty value. Silent drop is invalid. This is **not** Dolgorae thread
generation (start/resume/fork instruction binding). Gamchi
`generation_id` is per admitted turn. This epic's freeze is: **one
Gamchi thread keeps one instruction string; resume with the same value
succeeds; a change is refused.** Dolgorae sends the field on resume, so
same-value retransmit is required.

A completed turn kills the Grok child. The next turn starts a new
process and `session/load`s the stored ACP session. First-turn
`session/new` delivery is not proof that a later process still has the
instructions. Distinguish **must not change** from **must not restore
the same text**. If the observed channel is process-scoped, follow-up
must re-apply the stored string; that is restore, not a new generation.

Instructions are role/purpose text, not a sandbox-class security
boundary. Do not claim they confine Grok the way `--sandbox` claims to.

ACP `session/new` has no instruction field in the pinned client crate.
Do not prepend instructions onto `session/prompt` user input. Do not
invent a Grok CLI flag or `_meta` key without a live capture that Grok
**honors**, not merely echoes. Unauthenticated capture is Blocked.

This epic is a predecessor for instruction-delivery compatibility. It
does not make Dolgorae able to select gamchi. Remaining attach work
(other repo / later epic): Profile validation is Codex 0.153.4 and
runs Codex schema generation; Dolgorae turns send `networkAccess:
false` and write turns send `writableRoots` (gamchi refuses those
extras); gamchi also refuses unverified `sandbox=read-only`. A
refuse-only closeout is **safe refusal**, not **role-instruction
attach**.

Non-goals: Dolgorae Profile or adapter (other repo); `thread/fork`; MCP
`grok_spawn` instruction field; Camchi/Zamchi; prompt-prepend; CCAS
import; claiming Dolgorae Profile attach.

| Task | Title | Status | Depends on | Done when |
| --- | --- | --- | --- | --- |
| [TASK-029](#epic-008-generation-immutable-developer-instructions) | Spec instruction contract and fail-closed | `Planned` | TASK-028 | grok-launch.md owns the input/lifetime/failure table and states instructions are not a sandbox-class boundary. acp-item-mapping.md says they are not a userMessage and not prompt-prepend. The table covers: omit, JSON null, empty string, whitespace-only, wrong JSON type; resume same value (succeed), different value (refuse), explicit empty; turn/start with forbidden developerInstructions; reuse of a pre-EPIC-008 thread that already stored non-empty text. Statically known unsupported refuses before admit. Runtime apply failure aborts before session/prompt. If a turn was already admitted, publish `failed` and the completion notification so the parent is not left waiting. Omitted/empty first start stays today's no-instruction path. Must not change ≠ must not restore. |
| [TASK-030](#epic-008-generation-immutable-developer-instructions) | Capture how live grok agent stdio receives instructions | `Planned` | TASK-029 | ADR from a live capture or recorded absence. First investigation candidate: installed Grok `1.0.25` `--rules` (docs.x.ai CLI reference: extra rules appended to the system prompt). Also investigate `--system-prompt-override` / `--system-prompt` (full system-prompt replacement, not the same as `--rules`). Adopt a full replacement only if the ADR verifies its effect on default agent behavior and that required working instructions are preserved; a role-response difference alone is not adoption. `agent stdio` honor is unproven until this capture. Evidence must record grok version and argv, the exact channel, separation from user prompt, an **observable behavior difference** with vs without the text (traffic that only contains the string is not go — sent-but-ignored fails), and survival across first turn → child exit → new child `session/load` → second turn (re-apply if the channel is process-scoped). Distinguish: unverified (Blocked, not a bypass); this-version no-go for apply / go for refuse; go for apply. Unauthenticated is Blocked. Compilation is not proof. |
| [TASK-031](#epic-008-generation-immutable-developer-instructions) | Apply the captured channel, or refuse non-empty | `Planned` | TASK-030 | Fake agent plus app-server tests for the TASK-029 table: omit/empty still works; non-empty follows the ADR; same-value resume succeeds; change refuses; restore after child exit (first turn → kill → new child `session/load` → second turn). `make test` is the offline gate and does not call live Grok. Go-for-apply requires running and passing the ignored live test against the TASK-031 implementation before completion. That live run must exercise instruction delivery and restoration through Gamchi’s app-server path. `make test` still skips it. If authentication or the execution environment prevents that live run, do not complete; leave remaining verification explicit (`Blocked`, not a skip). Refuse-only stays offline: `make test` proves refusal and the closeout is safe-refusal, not attach-ready. |
