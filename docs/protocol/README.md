# Protocol artifacts

Language: English.

This directory pins the Codex app-server 0.149.0 **consumer subset** that
Dolgorae requires of a Codex app-server. The subset is a **client requirement
list**, not gamchi's server item allowlist. gamchi's emit
allowlist is [ACP item mapping](../specs/acp-item-mapping.md). The artifact
itself sets `architecture_contract_eligible: true` and
`production_runtime_eligible: false` (Dolgorae's Codex production campaign,
not a ban on using these wire shapes).

## Provenance

| Artifact | Path | SHA-256 |
| --- | --- | --- |
| Dolgorae required subset | [references/dolgorae-codex-0.149.0-required-subset.json](references/dolgorae-codex-0.149.0-required-subset.json) | `7d6b33228266826eb5077192867f31f0caf2df1875525c194ee23f179050d409` |
| Codex 0.149.0 stable schema bundle | (not vendored yet; digest recorded) | `dd10ff064b4beee2ebb676a81da5e78afd8ee8dfc65f963a5e574ba83444048e` |
| Codex 0.149.0 experimental schema bundle | (not vendored yet; digest recorded) | `917afec744f354d125964f7f5793de21acf2b4ce1aac30a1c2d808b25377cd6c` |

The subset bytes were copied from the local Dolgorae checkout
`/Users/draccoon/Workspace/RootKernel/dolgorae/dolgorae` at commit
`a72a2a9a4482304d206b0fe517375ce84d5f698e`. The digest matches the CCAS pin
of the same file. A byte change requires an explicit gamchi Task; do
not refresh from a moving Dolgorae tree.

## Relationship

```text
Dolgorae worker  --WebSocket over unix://  Codex app-server 0.149.0 subset-->  Codex
                                                                          -->  CCAS (Claude)
                                                                          -->  gamchi (Grok, EPIC-005)
```

Dolgorae is a **client**. It launches `<executable> app-server --listen unix://<socket>`,
upgrades HTTP/1.1 to WebSocket on `/`, and then sends `initialize` /
`initialized`, `account/read`, `model/list`, `thread/start|resume|read|fork`,
and `turn/start|interrupt`. It requires `optOutNotificationMethods: []` so
`item/started`, `item/completed`, `thread/started`, and turn lifecycle are
not suppressed. JSON-RPC objects omit the `jsonrpc` member.

gamchi is a **server** on that same wire. Honest identity:

- `userAgent` is `gamchi/app-server-v1`, not Codex and not `ccas/app-server-v1`
- `capabilities.ccas` is rejected
- models are Grok, not Claude or Codex

Dolgorae's Profile registry today validates a Codex executable, `CODEX_HOME`,
and the 0.149.0 schema campaign. Pointing a Dolgorae Profile at
`gamchi` is a Dolgorae change, not a gamchi v1 claim.
EPIC-005 proves a Dolgorae-**shaped** client against gamchi's socket.

Transport bounds taken from Dolgorae `src/app_server.rs` (same numbers CCAS
REQ-TRANSPORT-004 uses):

| Bound | Value |
| --- | --- |
| HTTP upgrade headers | 16 KiB |
| WebSocket frame | 16 MiB |
| reassembled message | 32 MiB |
| solicited response envelope excluding streamed `result` | 64 KiB |
| correlation wait | 4,096 messages |

HTTP/WS upgrade rejection (TASK-015). Copied as the closed inverse of
Dolgorae's client handshake in `src/app_server.rs` (the request Dolgorae
sends and the 101 response it requires). Numeric bounds above are the
CCAS REQ-TRANSPORT-004 / Dolgorae constants already in `source_wire`.
Do not import CCAS or Dolgorae.

| Condition | Response |
| --- | --- |
| Headers exceed 16 KiB before `\r\n\r\n` | Close with no HTTP status |
| Method is not `GET` | `405 Method Not Allowed` |
| Target is not `/` | `404 Not Found` |
| Version is not `HTTP/1.1` | `400 Bad Request` |
| `Upgrade` is missing or not `websocket` | `400 Bad Request` |
| `Connection` does not include `Upgrade` | `400 Bad Request` |
| `Sec-WebSocket-Key` missing or not 16-byte base64 | `400 Bad Request` |
| `Sec-WebSocket-Version` is not `13` | `426 Upgrade Required` with `Sec-WebSocket-Version: 13` |

A valid upgrade replies `HTTP/1.1 101 Switching Protocols` with
`Upgrade: websocket`, `Connection: Upgrade`, and `Sec-WebSocket-Accept`.
Occupied Unix listen paths fail closed and are not unlinked.

## Consumer scenarios (TASK-019)

A Dolgorae-**shaped** client, not a Dolgorae Profile, proves these five named
scenarios offline against `gamchi app-server`. Live Grok is not
required. `thread/fork` stays fail-closed; ADR-0002 did not decide it and
TASK-019 recorded no new capture.

| Name | Client sequence | Passes when |
| --- | --- | --- |
| `probe` | `initialize`, `initialized`, `account/read`, `model/list`, `grok/runtime/read`; `thread/read` of an absent id | Honest identity; `grok/runtime/read` shape; absent thread is JSON-RPC `-32600` |
| `first-turn` | `thread/start` + `turn/start` | `thread/started` then item and `turn/completed` notifications; omitted socket defaults stay `read-only` / `untrusted` |
| `follow-up` | `thread/resume` then a second `turn/start` on the same thread after the first is terminal | New turn on the same ledger thread |
| `approval` | untrusted turn parks on `requestApproval`; client `{decision}` | Socket server request, not MCP `grok_respond`; `turn/completed` after accept |
| `interrupt` | `turn/interrupt` of `inProgress`; `thread/fork` | `turn/completed` interrupted; fork remains `-32602` |

## grok/runtime/read

Grok facade method. Not a subset client method and not in `samchi-core`.
JSON-RPC objects omit the `jsonrpc` member.

Request: `{ "id": <integer or string>, "method": "grok/runtime/read", "params": {} }`.
Non-empty `params` is JSON-RPC `-32602`.

Result:

```text
{
  "runtime": "grok",
  "userAgent": "gamchi/app-server-v1",
  "home": "<absolute ledger home>"
}
```

`runtime` is the Grok identity. `userAgent` matches `initialize`. `home` is
the resolved ledger root (`--home`, then `GAMCHI_HOME`, then
`~/.gamchi`). Absent `thread/read` is JSON-RPC `-32600` from the
typed ledger `NotFound` (not from other `thread/read` param errors, which
stay `-32602`). Other methods keep the existing `-32602` fail-closed mapping.

## Internal types

Rust types in `crates/core` (`source_wire`) close what the **subset bytes actually name**:
client methods, approval policies, sandbox values, terminal turn statuses,
server-request method names, and approval decisions. Public ThreadItem
**emit** types are gamchi policy (ACP-projectable) and are tested as
such, not as a subset-derived allowlist.
