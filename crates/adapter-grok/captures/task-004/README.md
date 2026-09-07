# TASK-004 live `grok agent stdio` capture

Language: English.

This directory is the TASK-004 evidence: ACP JSON-RPC recorded from a live
`grok agent --no-leader --always-approve stdio` child, not the TASK-003 fake
agent and not `grok -p`.

| File | Contents |
| --- | --- |
| `traffic.ndjson` | One envelope per line: `direction` plus the JSON-RPC `message` |
| `metadata.json` | Pinned `grok --version`, argv, process owner, session id |
| `TASK004_CAPTURE.txt` | Copy of the file written during the captured turn |

The child was launched with `--no-leader`, so the capture parent owned that
Grok process. `_x.ai/*` extension notifications were dropped from traffic
(MCP server lists and UI settings); standard ACP methods and `fs/*` client
requests remain. Hostnames and user home paths were redacted. The write
path is recorded as `/CAPTURE_CWD/TASK004_CAPTURE.txt`.

`make test` parses these files offline and does not spawn Grok.
