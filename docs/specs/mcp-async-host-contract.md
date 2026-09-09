# MCP async host contract

Status: Adopted. EPIC-003 implemented TASK-008 through TASK-010.
TASK-013 publishes `grok_respond`.
Language: English.

This is how Claude Code and Codex supervise a Grok turn through
samchi-for-grok.
It is modeled on Gaori (`start_*` + `await_run` + bounded `wait_run`).
Dolgorae is not in this path.

The product is a **write-capable subagent**. The parent delegates review *or*
implementation (edits, tests, commands). It is not a read-only reviewer.

## What “done notification” means

MCP servers cannot reliably wake the parent model with a push notification.
A **terminal** completion is the return of a pending `grok_await` whose turn
status is one of the pinned subset statuses: `completed`, `interrupted`,
`failed`.

```text
MCP:  grok_spawn returns ids; the **MCP server process** owns the Grok child
      grok_await holds until terminal (or pending_approval)
CLI:  worker start prints ids then **stays in the foreground** until terminal
      There is no v1 daemon. If the CLI process exits, the child is gone.
```

“Immediate return” applies to **MCP tool results**, not to the CLI process
exiting. A disk ledger does not replace a live owner.

Do not claim spawn-only work that later interrupts the parent with no waiter.

`cancelled` and `input_required` are **not** `TurnStatus` values. The pinned
subset lists terminal statuses `completed | interrupted | failed` and
nonterminal `inProgress`. Unlisted status strings are Unreadable.

## MCP v1 defaults (subagent, not CCAS)

| Field | MCP spawn default | Why |
| --- | --- | --- |
| `approvalPolicy` | `never` | Unattended implementation. Grok may edit and run commands. |
| `sandbox` | `workspace-write` | Real coding work, not read-only review. |
| `approvalPolicy` override | `untrusted` / `on-request` | Admitted. Pause for `grok_respond`. Default spawn remains `never`. |

TASK-010 host smoke uses these defaults (yolo + writable). It must show Grok
can change a file, not only reply `pong`. A parent that wants review-only
passes `sandbox: read-only` on spawn; if Grok cannot enforce it, spawn fails
(see [grok-launch.md](grok-launch.md)).

App-server omitted-field defaults (EPIC-005) stay on the Dolgorae/CCAS wire
(`sandbox` omitted → `read-only`, `approvalPolicy` omitted → `untrusted`).
Those are a different parent. Socket `thread/start` + `turn/start` still
write the same home ledger as MCP spawn; they do not inherit MCP spawn
defaults. Socket approvals are subset server requests
(`item/commandExecution/requestApproval`, `item/fileChange/requestApproval`)
on that same `pending_approval` / respond ledger. They are not MCP
`grok_respond`. TASK-019 publishes `grok/runtime/read` on that socket
(`runtime: grok`, `userAgent`, `home`). It is not an MCP tool. Five named
Dolgorae-shaped scenarios (`probe`, `first-turn`, `follow-up`, `approval`,
`interrupt`) live in [protocol/README.md](../protocol/README.md).

## Tool surface

Domain ops are shared. **Publication is staged.** A tool name with
not-implemented is not done.

| Tool | When published | Behavior |
| --- | --- | --- |
| `grok_spawn` | TASK-009 | MCP only: returns ids immediately. Owner = this MCP server. |
| `grok_await` | TASK-009 | No samchi-for-grok timeout. Returns on terminal TurnStatus, or `await_reason: pending_approval`. |
| `grok_wait` | TASK-009 | Default and max `timeout_ms` 50000 (under a typical 60s host tool deadline, same cap as Gaori). Timeout does **not** cancel the turn. |
| `grok_status` | TASK-009 | One snapshot. |
| `grok_result` | TASK-009 | Terminal envelope, or `not_ready`. Truncation: see Result bounds. |
| `grok_list` | TASK-009 | Recent turns from the disk ledger. |
| `grok_cancel` | TASK-011 | Process-group teardown → TurnStatus `interrupted`. |
| `grok_followup` | TASK-012 | New turn, same ACP session (`session/load`). |
| `grok_respond` | TASK-013 | ACP `session/request_permission` decision. Then `grok_await` again. |

TASK-009 smoke is **six** MCP tools. CLI TASK-008: `start` (print ids, stay
until terminal), `wait`, `status`, `result`, `list`. CLI has no detached
`await` in v1 because `start` is the owner. `cancel` / follow-up / respond
appear on CLI when those Tasks land.

## Result bounds and lost spawn

MCP/CLI JSON results are bounded (implementation picks a byte cap, default
64 KiB for text). Overflow sets `truncated: true` and a path under the home
where the full retained envelope lives. Do not drop the only copy of an
edit summary.

If spawn is admitted but the parent never sees the ids: `grok_list` for that
`cwd` shows `inProgress` turns. The skill may recover that id and await it.
Do not spawn again for the same user request if an inProgress turn already
matches. Optional `client_request_id` on spawn is allowed; duplicate
in-flight keys return the existing ids (TASK-006).

## ACP stopReason → TurnStatus

| ACP `session/prompt` stopReason | TurnStatus | Notes |
| --- | --- | --- |
| `end_turn` | `completed` | |
| `cancelled` | `interrupted` | Includes `grok_cancel` |
| `refusal` | `failed` | Preserve stopReason on the turn |
| `max_tokens` | `failed` | Preserve stopReason |
| `max_turn_requests` | `failed` | Preserve stopReason |
| anything else | `failed` | Preserve stopReason; do not invent subset statuses |

## Approval (optional, not v1 default)

`untrusted` and `on-request` admit a turn. Those policies pause
(`inProgress`). `grok_await` returns `await_reason: pending_approval` plus
`request_id`. That return is **not** a TurnStatus. The parent calls
`grok_respond` then `grok_await` again. Unknown, stale, or duplicate
`request_id` values are rejected. Deny, cancel, or worker death while
pending converges to a terminal TurnStatus without running the gated shell.

Default `never` never takes this path. TASK-010 does not require respond.

## Agent lifecycle (required skill text)

The `use-samchi-for-grok` skill (TASK-010) must state:

1. Call `grok_spawn` exactly once. Preserve `thread_id` and `turn_id`.
   Omit approval/sandbox unless the user asked for read-only or gated
   approvals. Default is write-capable implementation.
2. Call `grok_await` with that `turn_id`. Prefer a host-native wait that keeps
   the tool call pending until the turn is terminal.
3. If the host returns a deferred execution handle or cell, wait only on that
   same handle for up to five minutes at a time. Return early when the call
   completes. Do not resume model reasoning merely to report liveness.
4. Do not poll `grok_status`, `grok_wait`, or `grok_list` to prove the turn is
   still running.
5. A user progress question may use one `grok_status` or one `grok_wait`.
6. If `grok_await` ends because of **host** timeout, call `grok_await` again on
   the same `turn_id`. Never spawn a second turn for the same request. If spawn
   ids were lost, `grok_list` for that cwd and await an existing `inProgress`
   turn instead of spawning.
7. If `await_reason` is `pending_approval`, call `grok_respond` then await
   again. Default yolo turns never do this.
8. If the host deadline is verified too short for `grok_await`, fall back to
   `grok_wait` on the same `turn_id`.

## Host timeout

`grok_await` has no samchi-for-grok timeout. The MCP host tool deadline must exceed
the longest expected Grok turn plus ledger finalization.

For Codex, documented config (do not silently edit the user's config):

```toml
[mcp_servers.samchi-for-grok]
command = "samchi-for-grok"
args = ["mcp"]
tool_timeout_sec = 3600
```

Claude Code: project `.mcp.json` plus the skill. Long MCP calls may be
backgrounded by the host; that is **observer** behavior. Re-await the same
`turn_id`. Do not treat host cancel of the await tool as `grok_cancel`.

TASK-010 should exercise Codex and Claude Code when both are available. One
successful host does not prove the other. Record which host was verified.

## Ledger root

One home for worker, MCP, and later app-server:

1. `--home <absolute-path>` if passed
2. else `$SAMCHI_FOR_GROK_HOME`
3. else `~/.samchi-for-grok`

Layout under that home: `threads/`, `turns/`, `generations/`. Do not put the
ledger in the git workspace by default. `.sorage/` in `.gitignore` is the
Sorage handoff marker, not the ledger.

One home may be used by CLI, MCP, and later app-server. TASK-006 must
implement:

- Exclusive **one inProgress turn per thread**
- A lock so two hosts cannot admit two turns on the same thread
- Atomic terminal publish (temp file + rename)
- Waiters wake on that publish; they must not miss a transition
- Generation liveness is samchi-for-grok pid **and** Grok child waitpid / ACP EOF.
  Parent death and child death both resolve waiters without a host timeout
- If a terminal record is already published, do not overwrite it with
  `worker_gone`

## Worker ownership

`grok agent stdio` is a **child of the samchi-for-grok process** that spawned it:
the long-lived MCP server, or the CLI `start` process that stays in the
foreground. It is not detached in v1. CLI `start` must not exit while the
child should live.

`grok agent leader` / `grok agent serve` are not the v1 owner
([ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md)).
Launch with `--no-leader`.

Each turn records a **generation** (`samchi-for-grok` pid + start epoch).
`grok_await` / `grok_wait` / `grok_status` that see `inProgress` first check
generation liveness. A dead generation converges immediately to `failed` with
failure reason `worker_gone`. Infinite wait is forbidden.

After reconnect, the parent may **observe** a terminal ledger record
(`grok_result`, `grok_list`). It cannot resume an in-flight turn whose worker
is gone. Do not auto-replay the same input on that `turn_id`. Crash is
`failed` / `worker_gone`, not `interrupted`. A later distinct user request
may `grok_spawn` or `grok_followup` as a **new** turn. That is weaker than
“Gaori plus durability”; it is honest.

## Non-goals

- MCP Tasks extension as the only API
- MCP `notifications/*` as the completion signal
- Parent conversation auto-resume without a waiter
- Print mode (`grok -p`)
- MCP v1 defaulting to `untrusted` (that is CCAS/Dolgorae, not this parent)
