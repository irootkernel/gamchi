# TASK-035 live app-server instruction apply

Language: English.

Ignored `live_instructions` test through Gamchi **app-server** (not a
direct ACP client). `make test` does not spawn Grok.

Parent-owned `grok agent stdio` with `--no-leader`, sandbox
`workspace-write`, `approvalPolicy=never`. Honor is the agent reply,
not wire bytes. CHECK_B was not in the first-turn reply. Restore used
a different child pid and the same ACP session id.

See [ADR-0004](../../../../docs/architecture-decision-records/0004-developer-instructions-meta-rules.md).
