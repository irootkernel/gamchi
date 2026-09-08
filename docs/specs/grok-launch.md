# Grok adapter launch (sandbox and approvals)

Language: English. TASK-007 implements this. Do not treat Codex sandbox
names as Grok enforcement.

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

v1 argv for default never + workspace-write is:

```text
grok --cwd <abs> --sandbox workspace agent --no-leader --always-approve stdio
```

v1 argv for `untrusted` / `on-request` omits `--always-approve` and sets
`--permission-mode default` so a user config yolo cannot bypass gating:

```text
grok --cwd <abs> --sandbox workspace --permission-mode default agent --no-leader stdio
```

`--sandbox` is a top-level `grok` flag. `grok agent --sandbox` is rejected by
grok 1.0.13.

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
