# TASK-030 live instruction-channel capture

Language: English.

Live `grok agent stdio` probes of how Grok 1.0.25 receives extra
instructions. Not the TASK-003 fake agent and not `grok -p`.
`make test` reads [metadata.json](metadata.json) offline and does not
spawn Grok.

The capture parent owned the child (`--no-leader`). Probes used unique
tokens unless noted. Traffic dumps are not checked in: honor is the
agent reply containing or omitting that token, not bytes on the ACP
wire.

## Verdict

**no-go for apply / go for refuse.** There is no parent-owned spawn
channel (CLI flag or ACP field) that Grok honors for
`developerInstructions`. Cwd instruction files (`AGENTS.md`,
`extra.rules`, `instruction.rules`) are honored as Grok project
discovery. That is not a Gamchi install path: it mutates the user's
workspace and collides with files the user already owns.

`--rules` inline text and `--rules` pointing outside cwd were not
honored. `--system-prompt-override` with a unique token was not
honored on `agent stdio`.

See [ADR-0003](../../../../docs/architecture-decision-records/0003-developer-instructions-refuse.md).
