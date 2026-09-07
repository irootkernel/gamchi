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
| `approvalPolicy=untrusted` or `on-request` | no yolo; ACP `session/request_permission` | refuse spawn until TASK-013 (`grok_respond`) exists |
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

Launch with `--no-leader` (or equivalent) unless TASK-005 proves leader mode
still leaves this grokgrok process as the owner. Stdio connect is not proof
of an isolated backend.

## Files changed

`files_changed` is `git diff --name-only` against the turn’s start HEAD in
`cwd` (when cwd is a git work tree). It is **not** a complete inventory
inferred from `fileChange` items. Deletes, moves, and shell-mediated writes
show up only if git sees them. Non-git cwd: `files_changed` is empty and
`files_changed_complete` is false.
