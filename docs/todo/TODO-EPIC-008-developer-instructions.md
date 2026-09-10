# EPIC-008 execution map

Authority: Temporary execution detail for
[EPIC-008](../roadmap/README.md#epic-008-generation-immutable-developer-instructions).
This file does not own roadmap identity or lifecycle status.

## Goal

A Dolgorae-shaped app-server parent can send subset `developerInstructions`
on `thread/start`. Gamchi either makes Grok follow that text for the life
of that **Gamchi thread**, or refuses a non-empty value. Silent
store-and-ignore is invalid.

Freeze (this epic, not Dolgorae thread generation): one Gamchi thread
keeps one instruction string; resume with the same value succeeds; a
change is refused. Dolgorae sends the field on resume, so same-value
retransmit is required. Gamchi `generation_id` stays per admitted turn.

Restore is separate from freeze. A completed turn kills the Grok child
(`crates/adapter-grok/src/turn.rs` after `drive_prompt`). The next turn
starts a new process and `session/load`s the stored ACP session.
First-turn `session/new` is not proof the later process still has the
text. If the observed channel is process-scoped, follow-up re-applies
the stored string (restore, not a new instruction generation).

Instructions are role/purpose text, not a sandbox-class security
boundary.

## Non-goals

- Dolgorae Profile registry or a Gamchi adapter in the Dolgorae repo
- `thread/fork`, `item/fileChange/patchUpdated`, collaboration mailboxes
- MCP `grok_spawn` instruction field
- Camchi / Zamchi
- Prepending instructions onto `session/prompt` user input
- Importing CCAS
- Inventing a Grok CLI flag or ACP `_meta` key without a live capture
  that Grok honors
- Claiming this epic attaches Dolgorae to gamchi

## Constraints

ACP `session/new` in the pinned `agent-client-protocol` 2.1.0 / schema
1.7.0 client has `cwd`, `additionalDirectories`, `mcpServers`, and
`_meta` only. TASK-004 `session/new` was `{cwd, mcpServers:[],
_meta:{"yoloMode":true}}`. `_meta` is an extension channel already used
for yolo; it is not evidence of an instruction key. Unauthenticated live
capture is Blocked, not a bypass. Compilation is not live proof.

First investigation candidate (TASK-030): installed Grok `1.0.25`
`--rules` (CLI reference: extra rules appended to the system prompt).
Also investigate `--system-prompt-override` / `--system-prompt` (full
system-prompt replacement, not `--rules`). Adopt a full replacement
only if the ADR verifies its effect on default agent behavior and that
required working instructions are preserved; a role-response difference
alone is not adoption. Honor under `grok agent stdio` is unproven until
capture. Traffic that only contains the string is not go
(sent-but-ignored).

Today `thread/start` treats a non-string `developerInstructions` as
empty (`app_server.rs`); `thread/resume` ignores the field. Those
silent-drop paths are in TASK-029's table.

## Input and failure table (TASK-029 writes the outcomes)

Cover every row in [grok-launch.md](../specs/grok-launch.md):

- `thread/start`: omitted, JSON null, empty string, whitespace-only,
  wrong JSON type.
- `thread/resume`: same non-empty as stored (succeed), different
  non-empty (refuse), explicit empty, omitted (keep stored).
- `turn/start` with a `developerInstructions` member (forbidden).
- Reuse of a pre-EPIC-008 thread that already stored non-empty text.
- Statically known unsupported: refuse before admit.
- Runtime apply failure: abort before `session/prompt`.
- If a turn was already admitted: publish `failed` and the completion
  notification; do not leave the parent waiting.

## Task order

1. TASK-029 — contract in
   [grok-launch.md](../specs/grok-launch.md) and
   [acp-item-mapping.md](../specs/acp-item-mapping.md): the table above,
   freeze vs restore, not Dolgorae generation, not a sandbox-class
   boundary. Omitted/empty first start stays the current no-instruction
   path.
2. TASK-030 — live `grok agent stdio` capture or recorded absence, then
   an ADR. Evidence: grok version and argv, exact channel, separation
   from user prompt, observable behavior with vs without the text, and
   first turn → child exit → new child `session/load` → second turn.
   Distinguish unverified (Blocked); this-version no-go for apply / go
   for refuse; go for apply.
3. TASK-031 — implement the ADR. Offline tests cover the table,
   same-value resume, and the restore path. `make test` does not call
   live Grok. Go-for-apply requires running and passing the ignored live
   test against the TASK-031 implementation before completion. That live
   run must exercise instruction delivery and restoration through
   Gamchi’s app-server path. If authentication or the execution
   environment prevents the live run, do not complete; leave remaining
   verification explicit (`Blocked`). Refuse-only stays offline:
   safe-refusal, not attach-ready.

## Remaining Dolgorae attach (not this epic)

Instruction delivery does not select gamchi from Dolgorae:

- Profile validation is Codex `0.153.4` and runs Codex schema
  generation.
- Dolgorae turns send `networkAccess: false`; write turns send
  `writableRoots` and related extras. Gamchi refuses those fields.
  Unverified `sandbox=read-only` also refuses.

Successor work owns Profile/launch contract, sandbox extras, and a real
consumer scenario. Do not fold them into TASK-031.

## Verification

`make test` after TASK-031 (offline). TASK-029 is documentation.
TASK-030 is a repo capture or a no-go ADR. Go-for-apply TASK-031 is not
complete until the ignored live app-server test has been run and
passed; an unrun live test is not remaining verification recorded.
Do not start TASK-030 or TASK-031 until the predecessor is `Completed`.
