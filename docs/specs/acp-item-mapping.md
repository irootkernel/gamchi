# ACP session/update → ThreadItem mapping

Status: Adopted before TASK-007. Language: English.

The Dolgorae subset is a **client requirement** against Codex, not
samchi-for-grok's emit allowlist. This file is samchi-for-grok policy for what
the ACP adapter may project onto `source_wire` items. TASK-007 implements this
table; it does not invent another.

## Emit allowlist (MCP worker)

These are the only public items the adapter emits in v1:

| ItemType | ACP source |
| --- | --- |
| `userMessage` | Turn input from `grok_spawn` / `turn/start`, not `session/update` |
| `agentMessage` | `agent_message_chunk` accumulated; completed when `session/prompt` returns |
| `plan` | `plan` update |
| `commandExecution` | `tool_call` with kind `execute` (and `other` when it is a shell) |
| `fileChange` | `tool_call` with kind `edit` |
| `webSearch` | `tool_call` with kind `fetch` or `search` when the title/url is web |

`tool_call` creates or updates one item. If the first `tool_call` is already
terminal, the item is created completed (or failed) in one step.

`tool_call_update` **merges** into that item. Complete only when ACP status is
terminal (`completed`, `failed`, or equivalent). `in_progress` and omitted
status must not complete the item. Do not emit a second item for the same
tool call.

Unknown ACP kinds are skipped (no item), not mapped to a fake Codex type.

`kind` `delete` / `move` do not have their own ItemType in v1. They still
affect `files_changed` if git sees them ([grok-launch.md](grok-launch.md)).
Shell-mediated edits are `commandExecution` items plus git `files_changed`.

## Not emitted (no ACP source in v1)

`imageView`, `sleep`, `imageGeneration`, `enteredReviewMode`,
`exitedReviewMode`, `contextCompaction` are not in `source_wire.PUBLIC_ITEM_TYPES`.
If EPIC-005 needs to *accept* them on the socket from a Dolgorae-shaped
client, add them in a new Task with a source.

## Reasoning

`agent_thought_chunk` is discarded after receipt (subset
`notifications.reasoning_policy.classification = content_not_retained`).
Do not emit a ThreadItem. Do not suppress ACP lifecycle notifications to hide
reasoning.

## Excluded-fatal (subset schema branches samchi-for-grok will not emit)

If the adapter would have to project `subAgentActivity` or
`collabAgentToolCall`, fail the turn (`failed`). Those strings exist in the
subset as **schema presence** Dolgorae requires of Codex, not as
samchi-for-grok emit targets.

Unknown item types are neither public nor excluded-fatal: the adapter skips
them.

## stopReason

See [mcp-async-host-contract.md](mcp-async-host-contract.md). Preserve the
raw ACP stopReason on `Turn.StopReason`.

## session/load history (TASK-012)

ACP `session/load` replays prior conversation as `session/update`. That
replay is a **history phase**, not a new turn.

- Buffer updates until load finishes (prompt is not running yet).
- Reconcile against the ledger by stable ACP ids; do not append duplicates.
- Do not treat replayed tool calls as live execution or approval requests.
- Do not re-send prior user input.
- When the follow-up `session/prompt` starts, only *new* updates attach to
  the new turn.

Capability: `session/load` is optional. If initialize does not advertise it,
TASK-012 follow-up is out of scope or fail closed (TASK-005 records which).
