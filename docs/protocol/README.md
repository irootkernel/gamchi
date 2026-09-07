# Protocol artifacts

Language: English.

This directory pins the Codex app-server 0.149.0 **consumer subset** that
Dolgorae requires of a Codex app-server. The subset is a **client requirement
list**, not samchi-for-grok's server item allowlist. samchi-for-grok's emit
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
of the same file. A byte change requires an explicit samchi-for-grok Task; do
not refresh from a moving Dolgorae tree.

## Relationship

```text
Dolgorae worker  --WebSocket over unix://  Codex app-server 0.149.0 subset-->  Codex
                                                                          -->  CCAS (Claude)
                                                                          -->  samchi-for-grok (Grok, EPIC-005)
```

Dolgorae is a **client**. It launches `<executable> app-server --listen unix://<socket>`,
upgrades HTTP/1.1 to WebSocket on `/`, and then sends `initialize` /
`initialized`, `account/read`, `model/list`, `thread/start|resume|read|fork`,
and `turn/start|interrupt`. It requires `optOutNotificationMethods: []` so
`item/started`, `item/completed`, `thread/started`, and turn lifecycle are
not suppressed. JSON-RPC objects omit the `jsonrpc` member.

samchi-for-grok is a **server** on that same wire. Honest identity:

- `userAgent` is `samchi-for-grok/app-server-v1`, not Codex and not `ccas/app-server-v1`
- `capabilities.ccas` is rejected
- models are Grok, not Claude or Codex

Dolgorae's Profile registry today validates a Codex executable, `CODEX_HOME`,
and the 0.149.0 schema campaign. Pointing a Dolgorae Profile at
`samchi-for-grok` is a Dolgorae change, not a samchi-for-grok v1 claim.
EPIC-005 proves a Dolgorae-**shaped** client against samchi-for-grok's socket.

Transport bounds taken from Dolgorae `src/app_server.rs` (same numbers CCAS
REQ-TRANSPORT-004 uses):

| Bound | Value |
| --- | --- |
| HTTP upgrade headers | 16 KiB |
| WebSocket frame | 16 MiB |
| reassembled message | 32 MiB |
| solicited response envelope excluding streamed `result` | 64 KiB |
| correlation wait | 4,096 messages |

## Internal types

Rust types in `crates/core` (`source_wire`) close what the **subset bytes actually name**:
client methods, approval policies, sandbox values, terminal turn statuses,
server-request method names, and approval decisions. Public ThreadItem
**emit** types are samchi-for-grok policy (ACP-projectable) and are tested as
such, not as a subset-derived allowlist.
