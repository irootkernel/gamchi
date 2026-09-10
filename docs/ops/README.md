# Operations

Language: English.

Host packaging for Claude Code and Codex. Copy these snippets only when the
user asks. Do not silently edit user host config.

| Artifact | Parent |
| --- | --- |
| [codex-mcp.toml](codex-mcp.toml) | Codex `mcp_servers.gamchi` |
| [claude-mcp.json](claude-mcp.json) | Claude Code project `.mcp.json` |
| [../../skills/use-gamchi/SKILL.md](../../skills/use-gamchi/SKILL.md) | Agent skill |

`grok_await` has no gamchi timeout. Codex `tool_timeout_sec` must be
3600. Host observer timeout of await is not `grok_cancel`. `grok_cancel` is
published and tears down the Grok process group.

Live parent exercise (ignored): [host-exercise.md](host-exercise.md).
