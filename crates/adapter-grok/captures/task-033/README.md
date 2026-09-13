# TASK-033 live `_meta.rules` delivery and restore

Language: English.

Live `grok agent stdio` probes of first-turn ACP `session/new`
`_meta.rules` honor and same-session restore after child exit.
Not the TASK-003 fake agent and not `grok -p`. `make test` reads
[metadata.json](metadata.json) offline and does not spawn Grok.

The capture parent owned each child (`--no-leader`). CHECK_A and
CHECK_B used independent unique tokens. Traffic dumps are not checked
in: honor is the agent reply containing the expected token, not bytes
on the ACP wire.

## Verdict

**go for apply.** On grok 1.0.30, `session/new` `_meta.rules` is
honored. After the first process exits, a **different** process that
`session/load`s the same ACP session id still honors an undisclosed
CHECK_B. Rules were **not** re-sent on load. Control with no `_meta`
replied `NONE`. CHECK_B was not present in the first-turn reply.

Restore method: `session/load` of the stored ACP session id on a new
`grok agent stdio` process. The observed channel is session-scoped
persistence, not a process-scoped re-install.

See [ADR-0004](../../../../docs/architecture-decision-records/0004-developer-instructions-meta-rules.md).
Current runtime still refuses non-empty `developerInstructions` until
TASK-034.
