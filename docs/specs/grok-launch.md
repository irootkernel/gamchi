# Grok adapter launch (sandbox, approvals, model, effort, and instructions)

Language: English. TASK-007 implements sandbox and approval mapping. Do
not treat Codex sandbox names as Grok enforcement. TASK-023 specifies
model and reasoning-effort launch; TASK-024 implements the argv.
TASK-029 specifies developer-instruction lifetime and fail-closed.

## Spawn fields vs Grok flags

MCP/core names are the Dolgorae subset strings. The Grok adapter must map
them to **actual** `grok agent` flags and then **fail closed** if Grok cannot
enforce the requested policy.

| Core request | Intended Grok launch | If Grok cannot enforce |
| --- | --- | --- |
| `approvalPolicy=never` | `--always-approve` / yolo | refuse spawn |
| `approvalPolicy=untrusted` or `on-request` | no yolo; ACP `session/request_permission` | omit `--always-approve`; parent `grok_respond` |
| `sandbox=workspace-write` | `--sandbox workspace` (writes in cwd; see Grok docs) | refuse spawn |
| `sandbox=read-only` | `--sandbox` profile that denies workspace writes, if one exists and is verified | refuse spawn; do not accept the name and hope |

Grok sandboxing is **off by default**. Naming a Codex-like policy is not
evidence. Official Grok `workspace` allows writes to cwd, `~/.grok/`, and
temps, and allows child network. On macOS, `read-only` / `strict`
child-network limits may be **unenforced**. Do not advertise
`networkAccess: false` as enforced on macOS.

Subset fields `writableRoots`, `networkAccess`, `excludeSlashTmp`,
`excludeTmpdirEnvVar` are **not silently ignored**. If the parent sends them
and the adapter cannot implement them, spawn returns an error. v1 MCP spawn
omits them.

## Always-approve vs sandbox

`--always-approve` is permission prompts. `--sandbox` is OS isolation. Both
must be set explicitly. yolo does not imply a sandbox profile.

## Isolation of the child

Launch with `--no-leader`.
[ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md) keeps v1
on parent-owned `grok agent stdio`; `grok agent leader` / `grok agent serve`
were not captured as the owner. Stdio connect is not proof of an isolated
backend.

v1 argv for default never + workspace-write, after model/effort
resolution, is:

```text
grok --cwd <abs> --sandbox workspace agent --no-leader --always-approve -m grok-4.6 --reasoning-effort high stdio
```

v1 argv for `untrusted` / `on-request` omits `--always-approve` and sets
`--permission-mode default` so a user config yolo cannot bypass gating:

```text
grok --cwd <abs> --sandbox workspace --permission-mode default agent --no-leader -m grok-4.6 --reasoning-effort high stdio
```

`--cwd` and `--sandbox` stay top-level `grok` flags. `grok agent --sandbox`
is rejected by grok 1.0.13. Put `-m` and `--reasoning-effort` on `agent`,
matching the TASK-004 capture shape. The resolved ids replace
`grok-4.6` / `high` when the cascade below selects a different pair.

## Model and reasoning effort

Gamchi owns omitted `model` and `effort`. Do not inherit
`~/.grok/config.toml`. There is no project-local or cwd config file.

Per field, the first **present** non-blank value wins:

1. Explicit parent field (MCP, CLI, or app-server)
2. Home config `<resolved-home>/config.yaml`
3. Built-in `grok-4.6` / `high`

`resolved-home` is `--home`, then `GAMCHI_HOME`, then
`~/.gamchi` (the ledger home). Spawn does not create the file.

Home config is YAML with optional keys `default_model` and
`default_effort` only. Those keys fill omitted spawn `model` and
`effort`. They are not spawn field names.

```yaml
default_model: grok-4.6
default_effort: high
```

Absent and blank are different:

| Input | Treatment |
| --- | --- |
| Parent field omitted | Skip to the next layer |
| Parent field present and blank or whitespace | `INVALID_CONFIG` |
| Config file missing, or key omitted | Skip that layer |
| Config key present and blank or whitespace | `INVALID_CONFIG` |
| Config file malformed, or unknown key | `INVALID_CONFIG` |

After the cascade, `model` is never blank: omitted input becomes
`grok-4.6` unless an explicit or config value was present. The same
holds for `effort` and `high`. The literal id `grok` is the pre-EPIC-006
stub alias and always normalizes to `grok-4.6`, whether it came from the
parent, config, or a stored thread. A model literally named `grok` is
not supported. Any other pair is passed to Grok.

Do not parse `~/.grok/` caches. Do not pre-admit or reject spawn by
parsing `grok models`. Child start failure, ACP handshake failure, or a
process error is the spawn/turn error. `grok models` may feed app-server
`model/list` advertisement only; listing failure is not a spawn gate
(TASK-026).

## Thread lock

A samchi thread freezes `model` at first `grok_spawn` / `thread/start`
after the cascade and `grok` → `grok-4.6` normalization. Later
`grok_followup` / `turn/start` may change `effort` only.

- First-turn omitted fields use the cascade above.
- Later-turn omitted effort uses the previous turn's effort when that
  stored value is non-blank. A stored empty effort (`effort: ""`) is
  treated as omitted: home `default_effort`, else `high`. Follow-up does
  not re-resolve the thread model from config.
- Explicit later effort is passed to Grok after the blank and alias
  rules above. Child rejection fails the turn.
- Explicit later model that is not the thread model is refused by
  samchi before a new child starts, after normalizing `grok` to
  `grok-4.6` on both sides.

## ACP filesystem

Host-side `fs/read_text_file` and `fs/write_text_file` stay inside the turn
`cwd`. Paths that escape cwd are refused. Missing reads return empty. Grok's
`--sandbox workspace` does not by itself confine this channel.

## Files changed

`files_changed` is `git diff --name-only` against the turn’s start HEAD in
`cwd` (when cwd is a git work tree). It is **not** a complete inventory
inferred from `fileChange` items. Deletes, moves, and shell-mediated writes
show up only if git sees them. Non-git cwd: `files_changed` is empty and
`files_changed_complete` is false.

## Developer instructions (EPIC-008)

Subset `developerInstructions` on app-server `thread/start` and
`thread/resume` is role/purpose text for **one Gamchi thread**. It is
not a Dolgorae thread generation, not a Gamchi `generation_id` (that
stays per admitted turn), and not a sandbox-class security boundary.
Ledger storage is not Grok enforcement. Do not prepend the text onto
`session/prompt`. MCP `grok_spawn` has no instruction field.

**Freeze:** one Gamchi thread keeps one instruction string. Resume with
the same value succeeds; a change is refused. Comparison is exact (no
trim). Dolgorae sends the field on resume, so same-value retransmit is
required.

**Restore is not freeze.** A completed turn kills the Grok child. The
next turn starts a new process and `session/load`s the stored ACP
session. First-turn `session/new` is not proof the later process still
has the text. If the observed channel is process-scoped, follow-up
re-applies the stored string. That restore is not a new instruction
generation.

### Input and failure table

JSON `null` is omit. Empty means `""`. Whitespace-only means a string
whose every character is Unicode whitespace. Wrong JSON type means
boolean, number, array, or object.

| Surface | Input | Outcome |
| --- | --- | --- |
| `thread/start` | omitted or JSON `null` | empty; today's no-instruction path |
| `thread/start` | `""` | empty; no-instruction path |
| `thread/start` | whitespace-only string | refuse before admit |
| `thread/start` | wrong JSON type | refuse before admit |
| `thread/start` | non-empty string | freeze that exact string; apply or refuse per the channel rules below |
| `thread/resume` | omitted or JSON `null` | keep stored |
| `thread/resume` | same non-empty as stored | succeed |
| `thread/resume` | different non-empty than stored | refuse before admit |
| `thread/resume` | `""` while stored is empty | succeed (no-instruction) |
| `thread/resume` | `""` while stored is non-empty | refuse before admit (change) |
| `thread/resume` | whitespace-only string | refuse before admit |
| `thread/resume` | wrong JSON type | refuse before admit |
| `turn/start` | any `developerInstructions` member | refuse before admit (forbidden) |
| existing thread | stored non-empty that cannot be applied | refuse the new turn before admit; not silent ignore |

Omitted or empty first `thread/start` stays today's no-instruction
path. A pre-EPIC-008 thread that already stored non-empty text is not
grandfathered as silent ignore: the stored string is the freeze value.

### Apply versus refuse

Must not change is not must not restore. A same-value resume and a
process-scoped re-apply of the stored string are allowed. A different
value is not.

Statically known unsupported refuses **before admit**: wrong type,
whitespace-only, a `turn/start` `developerInstructions` member, a
resume that would change the stored string, and a non-empty freeze
string when no verified apply channel exists.

When a verified channel exists, install the frozen non-empty string on
that channel for the life of the Gamchi thread. Runtime apply failure
aborts **before** `session/prompt`. If a turn was already admitted,
publish `failed` and the completion notification so the parent is not
left waiting.

Silent store-and-ignore is invalid. Traffic that only contains the
string is not proof Grok honors it.

TASK-030 captures whether live `grok agent stdio` honors `--rules`
(append) or `--system-prompt-override` / `--system-prompt` (full
replacement). Adopt a full replacement only if the ADR verifies its
effect on default agent behavior and that required working instructions
are preserved. TASK-031 implements apply or refuse-only. Refuse-only
is safe refusal, not role-instruction attach.

This epic does not make a Dolgorae Profile able to launch gamchi.
Remaining attach (later work): Codex 0.153.4 Profile validation,
`networkAccess` / `writableRoots` extras, unverified `read-only`.
