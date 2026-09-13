# ADR-0003: Refuse non-empty developerInstructions (no spawn-owned apply channel)

- Status: Superseded by [ADR-0004](0004-developer-instructions-meta-rules.md)
- Decided: 2026-09-11
- Evidence: [TASK-030 capture](../../crates/adapter-grok/captures/task-030)
  (live `grok agent stdio`, grok 1.0.25). Not the TASK-003 fake agent,
  not `grok -p`, and not compilation.

## Decision

**this-version no-go for apply / go for refuse.** Gamchi must refuse a
non-empty app-server `developerInstructions` value. Do not install the
text through a Grok CLI flag, ACP `_meta` key, prompt-prepend, or a
file written into the thread cwd.

Omitted or empty `thread/start` stays today's no-instruction path.

## Context

TASK-029 requires either a captured channel that Grok **honors**, or
refusal of non-empty values. Silent store-and-ignore is invalid.
Authentication succeeded; replies were live model text.

The capture parent spawned:

```text
grok --cwd <abs> --sandbox workspace agent --no-leader --always-approve -m grok-4.6 --reasoning-effort low stdio
```

Honor means an **observable behavior difference** with versus without
the instruction. Traffic that only contains the string is not go.
Unique tokens were used for the probes that claim honor, so a prior
run cannot explain the reply.

## Spawn-owned candidates (not honored)

| Channel | Unique token | Honored on `agent stdio` |
| --- | --- | --- |
| no extra flags or files | no (control NONE) | no |
| `grok --rules` inline text | no | no (same NONE as control) |
| `grok --rules` absolute path outside cwd | yes | no |
| `grok --system-prompt-override` (`--system-prompt` alias) | yes | no |

`--rules` is documented as extra rules appended to the system prompt.
Under `grok agent stdio` 1.0.25, `--rules` inline text was not honored
and an out-of-cwd `--rules` path was not honored.
`--system-prompt-override` (documented compat alias `--system-prompt`)
did not install a unique token. These are not apply channels.

## Cwd instruction files (honored, not adoptable)

| Channel | Unique token | Honored |
| --- | --- | --- |
| cwd `extra.rules` without `--rules` | yes | yes |
| cwd `extra.rules` plus `--rules` pointing at it | yes | yes |
| cwd `AGENTS.md` | yes | yes |
| cwd `instruction.rules` | yes | yes |

Grok already loads project instruction files from cwd. `--rules` is
not required for that. Writing such a file to deliver Dolgorae
`developerInstructions` would mutate the user's workspace and collide
with files they already own. That is not a parent-owned spawn channel
and is not restore-via-ACP: the file simply remains on disk.

Must not change is not must not restore. Because no spawn-owned
channel exists, there is nothing to restore on `session/load` after
child exit. Refuse stays refuse.

## Rejected alternatives

- **Apply via cwd `AGENTS.md` / `extra.rules`.** Rejected: project
  discovery, not a Gamchi install path; collides with user files.
- **Apply via `--rules` anyway.** Rejected: inline text and out-of-cwd
  paths were not honored; cwd honor is the file, not the flag.
- **Apply via `--system-prompt-override`.** Rejected: unique token not
  honored on `agent stdio`; full replacement would also need proof
  that default working instructions survive.
- **Prompt-prepend.** Rejected by TASK-029; not reopened.
- **Invent ACP `_meta`.** Rejected: no honor capture.
- **Treat as unverified / Blocked.** Rejected: the probes ran
  authenticated and produced live replies.

## TASK-031

Implement the TASK-029 table as **refuse-only**. Offline `make test`
proves refusal. Closeout is safe refusal, not role-instruction attach.
