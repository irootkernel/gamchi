# TASK-010 host exercise

Language: English.

Record of which parents ran `grok_spawn` then `grok_await` until a named file
appeared. Isolated disposable git workspaces. User host config was not modified.

Grok: 1.0.13. Date: 2026-09-08.

Command (ignored, not `make test`):

```bash
cargo test -p samchi-for-grok --test live_host -- --ignored --nocapture
```

| Host | Available | spawn→await edit | How |
| --- | --- | --- | --- |
| JSON-RPC MCP client | yes | yes (`TASK010_LIVE.txt`) | `samchi-for-grok mcp --home <tmp>` |
| Claude Code | yes (`claude`) | yes (`TASK010_CLAUDE.txt`, ledger terminal) | `--mcp-config` + `--strict-mcp-config` + `--dangerously-skip-permissions` in the disposable cwd; no user `.mcp.json` edit |
| Codex | yes (`codex`) | yes (`TASK010_CODEX.txt`, ledger terminal) | `codex exec -c mcp_servers.samchi-for-grok.*`; no `~/.codex/config.toml` edit |

The Codex harness uses `--dangerously-bypass-approvals-and-sandbox` only inside
that disposable cwd so the Grok child can write. The shipped snippet stays
`tool_timeout_sec = 3600` and does not recommend that flag.

Observer timeout of await is not `grok_cancel`. That is skill and ops text.
This exercise did not wait out a 3600s host deadline.
