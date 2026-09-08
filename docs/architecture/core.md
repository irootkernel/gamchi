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
- Domain operations: start, await, wait, status, result, cancel, list, follow-up
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
CLI and MCP verbs stay unpublished until TASK-008/009.

## v1 scope

Ship: extractable core + CLI/MCP/app-server facades + **Grok** ACP adapter.

Do not: Claude/GLM inside this binary, importing CCAS, teaching Dolgorae to
select samchi-for-grok as a Profile (Dolgorae change).
