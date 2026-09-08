# EPIC-004 execution dossier

Language: English.

Status: Temporary execution SOT for EPIC-004. This file does not own roadmap
identity, ordering, dependencies, lifecycle vocabulary, or status.

Consumer epics: `EPIC-004` only.

## Goal

Make the already-attached Grok worker supervisable: cancel an in-flight
turn, continue on the same ACP session, optionally gate untrusted
approvals, and treat a dead worker as observation-only. Canonical goal:
[roadmap EPIC-004](../roadmap/README.md). Product v1 attach remains
TASK-010 ([product.md](../specs/product.md)); this epic does not reopen
that milestone.

## Non-goals

Owned by [product.md](../specs/product.md) exclusions and later epics:

- EPIC-005 app-server unix socket (MCP's long-lived server is not that
  socket)
- `grok -p`, Claude/GLM adapters, Dolgorae Profile integration
- `grok agent serve` / `grok agent leader` as v1 owner
  ([ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md))
- Treating host observer timeout, `grok_wait` timeout, or host cancel of
  `grok_await` as `grok_cancel`
- Auto-replaying the same prompt after the worker dies
- Changing MCP v1 defaults away from `never` + `workspace-write`

## Requirement owners

| Topic | Owner |
| --- | --- |
| Identity, order, lifecycle | [roadmap](../roadmap/README.md) |
| Product scope and MCP defaults | [product.md](../specs/product.md) |
| Tool publication, await, stopReason, pending_approval | [mcp-async-host-contract.md](../specs/mcp-async-host-contract.md) |
| Launch flags; untrusted refused until TASK-013 | [grok-launch.md](../specs/grok-launch.md) |
| session/load history vs new-turn items | [acp-item-mapping.md](../specs/acp-item-mapping.md) |
| Core cancel/follow-up vs adapter ACP | [architecture/core.md](../architecture/core.md) |
| `session/load` advertised, not invoked | [ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md) |
| Host skill text | [use-samchi-for-grok](../../skills/use-samchi-for-grok/SKILL.md) |

Do not copy those documents here. If they disagree, they win; this dossier
only maps them.

## Member tasks

Order: TASK-011 → TASK-012 → TASK-013 → TASK-014.
TASK-011 depends on TASK-010 (`Completed`).

| Task | Runtime owner | Verification | Must not |
| --- | --- | --- | --- |
| TASK-011 | Core cancel path; adapter process-group teardown of the `grok agent stdio` child (already launched with `process_group(0)`); MCP `grok_cancel`; CLI `worker cancel` when this Task lands | Offline: published surface, `interrupted` mapping, group teardown without live Grok, idempotent second cancel, first-terminal-wins vs `worker_gone`. **Live** (ignored, disposable cwd): an in-flight prompt dies and the turn is `interrupted`. `make test` stays offline | Treat wait/observer/host-await timeout as cancel; publish follow-up or respond; app-server; overwrite an existing terminal record |
| TASK-012 | Adapter `session/load` of the thread's stored ACP session id (new stdio child allowed after teardown) then a new `session/prompt`; MCP `grok_followup`; CLI follow-up when this Task lands | Offline: load replay is history (no duplicate items, no live tool/approval). Fail closed when initialize omits `loadSession` or `session/load` errors. **Live**: second turn on the same ACP session id | Re-send prior user input; treat load replay as a new turn; `session/new` disguised as follow-up; proceed if `loadSession` is not advertised |
| TASK-013 | Facade: MCP `grok_respond` / CLI respond. Core: keep `inProgress` and the `request_id`. Adapter: ACP `session/request_permission` decision. Default `never` still auto-approves | Offline: `untrusted`/`on-request` no longer spawn-reject; `pending_approval` is not a TurnStatus; unknown/stale/duplicate `request_id` rejected; deny/cancel/worker death does not leave `inProgress`. **Live**: untrusted does not run a shell without `grok_respond`; parent `grok_respond` then `grok_await` again | Make untrusted the MCP default; treat `pending_approval` as terminal; skip skill/launch updates |
| TASK-014 | Generation liveness already publishes `failed`/`worker_gone` (TASK-006). This Task forbids auto-replay of the same input on that turn | Offline: kill the owner or child of an `inProgress` turn; ledger is `worker_gone` (or an earlier terminal) and no second Grok child is started for that `turn_id`. Parent may only `grok_result` / `grok_list`. **Live not required** (process kill is offline) | Replay the prompt; resume model reasoning on a dead generation; overwrite an existing terminal record; call crash `interrupted` |

## Cross-document acceptance

EPIC-004 is ready for audit when TASK-011..014 are successfully terminal and:

1. `grok_cancel` (MCP) and CLI `cancel` tear down the child process group
   and publish TurnStatus `interrupted`. Host timeout of await is still
   not cancel.
2. Follow-up is a new turn on the same ACP session. `session/load` replay
   is a history phase ([acp-item-mapping.md](../specs/acp-item-mapping.md)).
   Missing `loadSession` fails closed.
3. `untrusted` / `on-request` admit a turn only with `grok_respond`.
   `grok_await` may return `await_reason: pending_approval` (not a
   TurnStatus). Default spawn remains `never` + `workspace-write`.
4. Killing the worker does not auto-replay the same turn. Resume is
   observation of the ledger only.
5. tools/list and CLI usage publish only verbs those Tasks landed. One
   inProgress turn per thread still holds.
6. TASK-012 live `session/load` is a new invocation; ADR-0002 still
   records advertised-not-invoked. `grok-launch.md` lifts the untrusted
   spawn-reject only when TASK-013 lands.

## Execution constraints

- Launch remains parent-owned `grok agent stdio` with `--no-leader`.
- Adapter spawn already places the child in its own process group
  (`process_group(0)`). TASK-011 must kill that group, not only the grok
  pid, so grandchildren die with the in-flight prompt. Bounded waitpid
  after the kill; do not leave the turn `inProgress` if the group is
  already gone.
- First terminal record wins. Cancel of an already-terminal turn is
  idempotent. If liveness publishes `worker_gone` first, do not overwrite
  it with `interrupted`.
- Cancel does not have to keep the Grok child alive. TASK-012 restores
  the thread's stored ACP session id with `session/load` (a new stdio
  child is allowed). `session/new` is not follow-up.
- `cancelled` ACP `stopReason` already maps to `interrupted`. `grok_cancel`
  uses that same TurnStatus.
- `pending_approval` keeps the turn `inProgress` and returns `request_id`.
  `grok_respond` is only valid for that id. Unknown, stale, or duplicate
  ids are rejected. Deny, cancel, or worker death while pending must
  converge to a terminal TurnStatus without running the gated shell.
- CLI `start` stays in the foreground; MCP spawn may return because the
  server process owns the child. Cancel/follow-up/respond on CLI appear
  when the matching Task lands ([mcp-async-host-contract.md](../specs/mcp-async-host-contract.md)).
- `make test` stays offline. Live Grok is per-task ignored evidence in a
  disposable workspace; it must not mutate this repository's dirty
  worktree or unrelated user files. Compilation is not live proof.
- Do not silently edit user host config. Update
  [use-samchi-for-grok](../../skills/use-samchi-for-grok/SKILL.md) when a
  tool is published.
- Extra subset fields stay fail-closed; v1 MCP spawn still omits them.

## Handoffs and commit boundaries

This design session does not implement, stage, commit, or push.

Later `/aquarium:epic-handler` starts at TASK-011 after its own plan
envelope. Each member task is one isolated `/aquarium:task-commit` under
that Task ID after its acceptance and review. Stage and commit happen
only inside that envelope; they are not authorized here. Preserve
unrelated work. Live Grok stays on ignored tests in a disposable cwd; do
not edit user host config. The next task starts only when its predecessor
is `Completed` with that commit.

| Task | Canonical docs to promote if behavior changes | Skill / ops |
| --- | --- | --- |
| TASK-011 | mcp-async-host-contract (published `grok_cancel`); TESTING.md live cancel note | skill: cancel is published; still not host-await timeout |
| TASK-012 | acp-item-mapping (load history already specified); mcp-async-host-contract (`grok_followup`) | skill: follow-up on same ACP session id |
| TASK-013 | grok-launch (untrusted no longer spawn-reject); mcp-async-host-contract (`grok_respond`, `pending_approval`) | skill: respond then await again |
| TASK-014 | mcp-async-host-contract only if the no-replay rule is not already implied by worker_gone | none |

## Closeout

This dossier has one consumer. At EPIC-004 closeout, after a clean epic
audit:

1. Promote durable behavior into the owners in the table above (do not
   leave it only here).
2. Confirm tools/list and CLI usage match the verbs those Tasks landed,
   the host skill matches the contract, and roadmap acceptance still
   holds.
3. Replace this epic's `Detailed SOT` with Canonical Outcomes links.
4. Delete this file and its TODO index entry.

Unowned durable information blocks closeout. A later epic must not keep
this file as a hidden second spec.
