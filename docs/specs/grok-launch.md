# Grok adapter launch (sandbox, approvals, model, and effort)

Language: English. TASK-007 implements sandbox and approval mapping. Do
not treat Codex sandbox names as Grok enforcement. TASK-023 specifies
model and reasoning-effort launch; TASK-024 implements the argv.

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

Samchi owns omitted `model` and `effort`. Do not inherit
`~/.grok/config.toml`. There is no project-local or cwd config file.

Per field, the first **present** non-blank value wins:

1. Explicit parent field (MCP, CLI, or app-server)
2. Home config `<resolved-home>/config.yaml`
3. Built-in `grok-4.6` / `high`

`resolved-home` is `--home`, then `SAMCHI_FOR_GROK_HOME`, then
`~/.samchi-for-grok` (the ledger home). Spawn does not create the file.

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
