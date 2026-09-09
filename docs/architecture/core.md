# Core and adapters

Language: English.

**samchi-for-grok is Grok-only.** The binary talks to Grok. It does not run
Claude or GLM.

The **structure** of its core (ledger, generation, thread/turn, facades) is
what a later Claude backend or zcode/GLM backend can copy or extract. Those
products are not features of samchi-for-grok.

## Place in the Dolgorae map

| Dolgorae backend | Product | Notes |
| --- | --- | --- |
| Codex | native `codex app-server` | unchanged |
| Claude | CCAS today | A future Claude backend may reuse this core’s *shape*, not this binary |
| Grok | **samchi-for-grok** | this repo |
| GLM / zcode | later | reuse core structure / crate extract; not a samchi-for-grok adapter slot |

v1 does not implement Claude or GLM. Do not add `adapter-claude` or
`adapter-glm` packages here.

## Layers (this repo)

```text
 parents
   Claude Code / Codex  --MCP-->   facade: mcp
   Dolgorae             --unix WS app-server 0.149.0 subset-->  facade: app-server
   humans / tests       --CLI-->   facade: cli
                                │
                                ▼
                           CORE   (extractable later)
                           listen/home, ledger, generation liveness
                           thread / turn / item
                           spawn / await / wait / cancel
                                │
                                ▼
                           Grok adapter
                           grok agent ACP
```

Keep Grok-specific mapping, catalog, and `userAgent=samchi-for-grok/app-server-v1`
out of core so a later extract does not drag Grok with it.

### Core owns

- Unix-domain WebSocket transport bounds (Dolgorae `app_server.rs` / CCAS table)
- JSON-RPC profile (no `jsonrpc` member, initialize/initialized)
- Durable thread/turn/item ledger and generation
- Domain operations: start, await, wait, status, result, cancel, list, follow-up, respond
- Dead-generation → `failed` / `worker_gone`
- Closed subset vocabularies that are actually in the pinned JSON
  (methods, approval policies, sandbox, terminal turn statuses)

### A facade owns

- MCP tool names and Gaori-shaped await (Claude Code / Codex parents)
- `app-server --listen unix://` (Dolgorae parent)
- CLI verbs

Facades call core. They do not speak ACP.

### The Grok adapter owns

- `grok agent stdio`
- ACP `session/update` → ThreadItems ([ACP mapping](../specs/acp-item-mapping.md))
- Grok model catalog verification
- Honest Grok identity (`runtime: grok`)

Core never imports the Grok, Claude, or zcode SDKs.

## Language

**Rust** for core, facades, and the Grok adapter (ADR-0001).

The core crate is Rust (`crates/core`, package `samchi-core`). The Grok ACP
adapter crate is `crates/adapter-grok` (package `samchi-adapter-grok`).
TASK-003's fake ACP agent and stdio harness live there; they do not import
Grok SDKs. The TASK-004 live `grok agent stdio` capture is
`crates/adapter-grok/captures/task-004` and is parsed offline. TASK-007
spawns parent-owned `grok --sandbox workspace agent --no-leader
--always-approve stdio` and maps `session/update` onto `source_wire` items.
ACP `fs/read_text_file` and `fs/write_text_file` stay inside the turn cwd.
TASK-008 publishes CLI worker start/wait/status/result/list. TASK-009
publishes MCP stdio six tools (`grok_spawn` returns immediately; the MCP
process owns the child). TASK-010 ships `use-samchi-for-grok` and Codex/Claude
snippets (`tool_timeout_sec = 3600`); copy them only when the user asks.
TASK-011 publishes MCP `grok_cancel` and CLI `worker cancel`: process-group
teardown of the `grok agent stdio` child, TurnStatus `interrupted`. Host
timeout of await is not cancel. TASK-012 publishes MCP `grok_followup` and
CLI `worker followup`: `session/load` of the stored ACP session id, then a
new turn. Load replay is history. Missing `loadSession` fails closed.
TASK-013 publishes MCP `grok_respond` and CLI `worker respond`: untrusted
and on-request park ACP `session/request_permission` while the turn stays
`inProgress`. `pending_approval` is not a TurnStatus. Default `never` still
auto-approves. TASK-014 forbids auto-replay of the same input after a dead
generation: observe `failed`/`worker_gone` only. Crash is not `interrupted`.
TASK-015 publishes the app-server facade listen:
`app-server --listen unix://<absolute-path> [--home]`. The process owns the
Unix socket, upgrades HTTP/1.1 GET `/` to WebSocket, and fails closed on an
occupied path. JSON-RPC `initialize` remains TASK-016.

## v1 scope

Ship: extractable core + CLI/MCP/app-server facades + **Grok** ACP adapter.

Do not: Claude/GLM inside this binary, importing CCAS, teaching Dolgorae to
select samchi-for-grok as a Profile (Dolgorae change).
