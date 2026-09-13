# ADR-0004: Apply developerInstructions via session/new `_meta.rules`

- Status: Accepted
- Decided: 2026-09-13
- Supersedes: [ADR-0003](0003-developer-instructions-refuse.md)
- Evidence: [TASK-033 capture](../../crates/adapter-grok/captures/task-033)
  (live `grok agent stdio`, grok 1.0.30). Not the TASK-003 fake agent,
  not `grok -p`, and not compilation.

## Decision

**go for apply** on grok 1.0.30 `agent stdio`. Install a frozen
non-empty app-server `developerInstructions` string on first-turn ACP
`session/new` `_meta.rules`. Restore after child exit by
`session/load` of the same ACP session id on a new process. Do not
re-send `_meta.rules` on load. Do not use `_meta.systemPromptOverride`
for ordinary role/purpose text. Do not bundle `_meta.yoloMode`.

**Current code still refuses** non-empty start values. This ADR adopts
the channel for TASK-034. It is not implementation and not live
app-server acceptance (TASK-035).

## Context

TASK-030 / ADR-0003 recorded no-go for CLI `--rules` and
`--system-prompt-override` on grok 1.0.25, and rejected inventing an
ACP `_meta` key because that channel was not probed. TASK-033 probed
the documented `session/new` `_meta.rules` field on grok 1.0.30.

Honor means an observable reply difference. Unique CHECK_A / CHECK_B
tokens were installed once in `_meta.rules`. The first user prompt was
only `CHECK_A`. After that child exited, a second process loaded the
same session id and prompted only `CHECK_B`. Control with no `_meta`
ended in `NONE`. CHECK_B was not in the first-turn reply.

| Probe | Process | ACP | Reply |
| --- | --- | --- | --- |
| control (no `_meta.rules`) | pid 27532 | `session/new` | contains `NONE`, not CHECK_A |
| first-turn `_meta.rules` | pid 47233 | `session/new` | exact CHECK_A token |
| restore | pid 48783 (different) | `session/load` same session | exact CHECK_B token |

`initialize` advertised `loadSession: true`. The capture parent spawned
`grok --cwd <abs> --sandbox workspace agent --no-leader --always-approve -m grok-4.6 --reasoning-effort low stdio`.

## Restore method

Observed: rules persist in the ACP session. A new `grok agent stdio`
process restores them with `session/load` of that session id, cwd, and
empty `mcpServers`. No `session/new` and no `_meta.rules` on the second
process.

Must not change is not must not restore. Same-value `thread/resume` and
this load-time restore are allowed. A different instruction string is
not.

Do not fake restore by creating a new ACP session.

## Legacy sessions

Do not infer successful installation from non-empty ledger text or from
the mere presence of an ACP session id.

| State | Policy |
| --- | --- |
| Non-empty freeze, no ACP session id | Install on the first `session/new` `_meta.rules` |
| ACP session created by that install path | `session/load` the stored id (observed restore) |
| ACP session without reliable install evidence | Fail closed; require a new Gamchi thread |

TASK-031 never admitted non-empty `developerInstructions`, so existing
ledger rows with an ACP session id are no-instruction sessions. Do not
attach rules onto those via load. Keep Grok `_meta` out of
`samchi-core`. v1 adds no separate provenance column: persist an ACP
session id only after the `session/new` that applied this channel (or
the empty no-instruction `session/new`).

## `set_acp_session_id` failure

If `session/new` succeeds but persisting the ACP session id fails, do
not send `session/prompt`. Tear down the child. If the turn was already
admitted, publish `failed` and the completion notification. Do not
report durable publication when it failed.

## Rejected alternatives

- **Keep refuse-only after honor was observed.** Rejected: silent
  store-and-ignore is invalid, and a verified parent-owned channel
  exists.
- **`_meta.systemPromptOverride` for role text.** Rejected: additive
  `rules` honors the token without replacing the default agent system
  prompt.
- **Re-send `_meta.rules` on `session/load`.** Rejected: not observed
  as required; load of the same session restored CHECK_B without it.
  Do not invent a load-time field.
- **CLI `--rules` / cwd instruction files.** Rejected by ADR-0003;
  not reopened.
- **Prompt-prepend.** Rejected by TASK-029; not reopened.
- **Treat unauthenticated as no channel.** Rejected: this capture
  authenticated. Inability to run remains `Blocked`.

## TASK-034 / TASK-035

TASK-034 implements this channel offline. TASK-035 live-proves it
through Gamchi app-server. Do not update EPIC-008 Canonical Outcomes
until TASK-035 passes. The Decision section's "current code still
refuses" sentence is the TASK-033 snapshot; it is not live runtime
status after TASK-034.
