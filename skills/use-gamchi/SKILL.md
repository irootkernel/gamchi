---
name: use-gamchi
description: >
  Supervise a write-capable Grok turn through gamchi MCP.
  Call grok_spawn once, then grok_await until a terminal TurnStatus.
  Do not poll status. Host timeout does not cancel the turn.
---

# Use gamchi

gamchi is a Grok-only local worker. MCP v1 is a write-capable
subagent: default `approvalPolicy=never` and `sandbox=workspace-write`.
Omit those fields unless the user asked for read-only or gated approvals.
Do not pass `writableRoots`, `networkAccess`, `excludeSlashTmp`, or
`excludeTmpdirEnvVar`. `untrusted` and `on-request` pause for `grok_respond`.
Default spawn is still `never` and auto-approves.

Completion is a terminal TurnStatus (`completed`, `interrupted`, `failed`)
returned by `grok_await`. MCP push is not the completion signal.
`grok_cancel` is published: it tears down the Grok process group and the
turn becomes `interrupted`. Do not treat host cancel of the await tool, an
observer timeout, or `grok_wait` timeout as cancel.
`grok_followup` starts a new turn on the same ACP session via `session/load`.
Load replay is history, not new items or approvals. If `session/load` is not
advertised, follow-up fails closed.
Optional `model` and `effort` on `grok_spawn` and `grok_followup`. Omit them
to use home `config.yaml` then `grok-4.6` / `high`. Present blank values are
`INVALID_CONFIG`. Follow-up cannot change the thread model after `grok` →
`grok-4.6` normalization. Recommend adding `.gamchi/` to the
consumer project's `.gitignore`. Do not edit that file for the user.
`grok_respond` answers a parked `session/request_permission`. `pending_approval`
is not a TurnStatus; call `grok_respond` then `grok_await` again.

## Lifecycle

1. Call `grok_spawn` exactly once. Preserve `thread_id` and `turn_id`.
   Omit approval/sandbox unless the user asked for read-only or gated
   approvals. Default is write-capable implementation. Omit `model` /
   `effort` unless the user named them.
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
   `grok_wait` on the same `turn_id`. Timeout does not cancel the turn.
9. If the turn is `failed` with `worker_gone`, do not replay that prompt on
   the same `turn_id`. Observe with `grok_result` or `grok_list`. A later
   distinct user request may spawn or follow up as a new turn. Crash is not
   `interrupted`.

## Host deadline

`grok_await` has no gamchi timeout. The MCP host tool deadline must
exceed the longest expected Grok turn plus ledger finalization.

Copy the Codex snippet from [docs/ops/codex-mcp.toml](../../docs/ops/codex-mcp.toml)
into host config only when the user asks. Do not silently edit user config.
`tool_timeout_sec = 3600`.

Claude Code: copy [docs/ops/claude-mcp.json](../../docs/ops/claude-mcp.json)
into project `.mcp.json` only when the user asks, and keep this skill.

Long MCP calls may be backgrounded by the host; that is observer behavior.
Re-await the same `turn_id`. Do not treat host cancel of the await tool as
`grok_cancel`.
