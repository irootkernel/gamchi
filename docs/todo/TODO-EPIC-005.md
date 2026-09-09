# EPIC-005 execution dossier

Language: English.

Status: Temporary execution SOT for EPIC-005. This file does not own roadmap
identity, ordering, dependencies, lifecycle vocabulary, or status.

Consumer epics: `EPIC-005` only.

## Goal

Layer the CCAS-shaped Unix-domain app-server on the same worker already
used by MCP and CLI. A parent launches
`samchi-for-grok app-server --listen unix://<absolute-path> [--home <absolute-path>]`
and drives thread/turn on the existing ledger. Canonical goal:
[roadmap EPIC-005](../roadmap/README.md). Product attach remains TASK-010
([product.md](../specs/product.md)); this epic does not reopen that
milestone.

## Non-goals

Owned by [product.md](../specs/product.md) exclusions and earlier epics:

- A new runtime, daemon, or second ledger
- Teaching Dolgorae to select samchi-for-grok as a Profile (Dolgorae change)
- `grok -p`, Claude/GLM adapters, importing CCAS or Dolgorae
- `grok agent serve` / `grok agent leader` as v1 owner
  ([ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md))
- Changing MCP v1 defaults away from `never` + `workspace-write`
- Inventing the five consumer-scenario names or the `grok/runtime/read`
  schema in this dossier (TASK-019 records those in-repo)

## Requirement owners

| Topic | Owner |
| --- | --- |
| Identity, order, lifecycle | [roadmap](../roadmap/README.md) |
| Product scope and Dolgorae parent | [product.md](../specs/product.md) |
| App-server omitted-field defaults | [mcp-async-host-contract.md](../specs/mcp-async-host-contract.md) |
| Transport bounds and subset methods | [protocol/README.md](../protocol/README.md), `source_wire` |
| Core vs app-server facade vs Grok adapter | [architecture/core.md](../architecture/core.md) |
| `thread/fork` not decided | [ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md) |
| HTTP/WS rejection table | TASK-015 recording |
| Honest `userAgent` / `capabilities.ccas` | TASK-016 |
| Named five scenarios + `grok/runtime/read` | TASK-019 recording |

Do not copy those documents here. If they disagree, they win; this dossier
only maps them.

## Member tasks

Order: TASK-015 → TASK-016 → TASK-017 → TASK-018 → TASK-019.
TASK-015 depends on TASK-014 (`Completed`).

The subset lists nine client methods. TASK-016 owns the handshake
(`initialize`, `initialized`, `account/read`, `model/list`). TASK-017 owns
the worker-calling remainder except fork. `thread/fork` stays fail-closed
until TASK-019.

| Task | Runtime owner | Verification | Must not |
| --- | --- | --- | --- |
| TASK-015 | Facade `app-server --listen unix://<absolute-path> [--home]`. HTTP/1.1 upgrade to WebSocket on `/`. Occupied path fail-closed. Copy the HTTP/WS rejection table from CCAS/Dolgorae constants into this repo. Transport numeric bounds already live in `source_wire` | Offline: listen, rejection table recorded, occupied path fails closed. Live Grok not required | Bind a new runtime; overwrite an occupied socket; change subset bytes; treat the current `NOT_IMPLEMENTED` stub as done |
| TASK-016 | Honest `initialize` / `initialized` / `account/read` / Grok `model/list`. `userAgent=samchi-for-grok/app-server-v1` stays out of `samchi-core`. `capabilities.ccas` is JSON-RPC `-32602` | Offline JSON-RPC. Models are Grok, not Claude or Codex | Claim Codex or `ccas/app-server-v1` identity; put `userAgent` in core |
| TASK-017 | `thread/start`, `thread/resume`, `thread/read`, `turn/start`, `turn/interrupt` call the existing worker. Socket `thread/start`+`turn/start` write the same ledger as MCP spawn. `thread/fork` is recognized and fail-closed | Offline: same-ledger identity with MCP spawn; one inProgress turn per thread. Live Grok not required for the ledger proof | A second ledger; treat fork as proven; apply MCP omitted-field defaults on the socket |
| TASK-018 | Server notifications `item/started`, `item/completed`, `turn/completed` (subset also names `thread/started` and `item/fileChange/patchUpdated`; `optOutNotificationMethods` stays `[]`). Approvals are socket server requests (`item/commandExecution/requestApproval`, `item/fileChange/requestApproval`) on the existing pending-approval core, not MCP `grok_respond` | Offline notification and server-request shapes. Map to the existing `pending_approval` / respond path | Drop the empty opt-out list; invent a second approval store; publish MCP tool names on the socket |
| TASK-019 | Name five consumer scenarios in this repo; record the `grok/runtime/read` shape; a Dolgorae-**shaped** client passes. `thread/fork` only if a **new** capture proves it | Named scenarios and shape live in-repo. Shaped client. Live Grok only if a named scenario requires a turn | Dolgorae Profile integration; enable fork from ADR-0002 alone |

## Cross-document acceptance

EPIC-005 is ready for audit when TASK-015..019 are successfully terminal and:

1. `app-server --listen unix://… [--home]` upgrades on `/` and fails closed
   on an occupied path. The HTTP/WS rejection table lives in this repo.
2. Identity is `samchi-for-grok/app-server-v1`. `capabilities.ccas` is
   `-32602`. Models are Grok.
3. Socket `thread/start` + `turn/start` write the same ledger as MCP spawn.
   One inProgress turn per thread still holds.
4. Notifications and approval server-requests match the subset
   classification a Dolgorae-shaped client requires
   (`optOutNotificationMethods: []`).
5. Five named scenarios and `grok/runtime/read` live in this repo. Fork
   remains fail-closed unless TASK-019 recorded a new capture.
6. Omitted socket fields stay `sandbox=read-only` and
   `approvalPolicy=untrusted`. MCP defaults stay `never` +
   `workspace-write`.

## Execution constraints

- Same worker, same home, same generation liveness. The app-server process
  owns the listen socket and any Grok children it admits, analogous to MCP.
- Occupied Unix path fail-closed. Listen path is `unix://` plus an
  absolute filesystem path.
- Extra subset fields (`writableRoots`, `networkAccess`,
  `excludeSlashTmp`, `excludeTmpdirEnvVar`) stay fail-closed when sent.
- `thread/fork` is not a v1 worker call until TASK-019 has a new live
  capture. ADR-0002 does not decide it.
- `make test` stays offline. Compilation is not live Grok proof. Live or
  shaped-client evidence stays in an ignored disposable cwd and must not
  mutate this repository's dirty worktree or unrelated user files.
- Do not change subset JSON bytes without a new Task.
- CLI `start` stays in the foreground; MCP spawn may return because the
  server process owns the child. App-server is a long-lived listen process
  like MCP, not a Dolgorae Profile.

## Handoffs and commit boundaries

This design session does not implement, stage, commit, or push.

Later `/aquarium:epic-handler` starts at TASK-015 after its own plan
envelope. Each member task is one isolated `/aquarium:task-commit` under
that Task ID after its acceptance and review. Stage and commit happen
only inside that envelope; they are not authorized here. Preserve
unrelated work. The next task starts only when its predecessor is
`Completed` with that commit. Next eligible Task remains `TASK-015`.

| Task | Canonical docs to promote if behavior changes | Notes |
| --- | --- | --- |
| TASK-015 | protocol (rejection table if it is more than transport bounds); architecture if listen ownership needs a sentence | Occupied-path fail-closed |
| TASK-016 | protocol honest identity already specified; architecture already keeps `userAgent` out of core | `capabilities.ccas` is `-32602` |
| TASK-017 | mcp-async-host-contract only if socket omitted-field defaults need a cross-link; architecture facade already named | Same ledger as MCP spawn |
| TASK-018 | acp-item-mapping / mcp-async-host-contract only if socket approvals differ from pending_approval | Server requests, not MCP tool names |
| TASK-019 | new in-repo scenario names and `grok/runtime/read` shape; ADR only if a new capture decides fork | Not a Dolgorae Profile |

## Closeout

This dossier has one consumer. At EPIC-005 closeout, after a clean epic
audit:

1. Promote durable behavior into the owners in the table above (do not
   leave it only here).
2. Confirm the published `app-server` surface, honest identity, same-ledger
   thread/turn, notifications, named scenarios, and `grok/runtime/read`
   still match roadmap acceptance.
3. Replace this epic's `Detailed SOT` with Canonical Outcomes links.
4. Delete this file and its TODO index entry.

Unowned durable information blocks closeout. A later epic must not keep
this file as a hidden second spec.
