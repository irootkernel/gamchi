# Samchi for Grok

**Grok-only** worker: facades + an extractable core + a Grok ACP adapter.
Claude and GLM are not this binary. A later backend may reuse the core’s
shape. CCAS stays Claude’s backend for now.

The command and crate identifier is `samchi-for-grok`. Parents delegate
**review or implementation**. The Grok adapter uses ACP (`grok agent stdio`),
not print mode. MCP spawn defaults to yolo + workspace-write.

```text
Claude Code / Codex --MCP--> facade
Dolgorae            --app-server--> facade
                              core  --adapter-grok--> grok agent stdio
```

See [docs/architecture/core.md](docs/architecture/core.md). Core language is
Rust ([ADR-0001](docs/architecture-decision-records/0001-rust-core.md)). The
core crate is `crates/core` (`samchi-core`); the `samchi-for-grok` binary stub
is `crates/samchi-for-grok`; the Grok ACP adapter (fake agent + stdio harness)
is `crates/adapter-grok` (`samchi-adapter-grok`); roadmap docscheck is
`crates/docscheck`.

The v1 host contract is MCP, Gaori-shaped: spawn returns an id immediately;
`grok_await` stays pending until the turn finishes; that return is the
completion signal. Details:
[docs/specs/mcp-async-host-contract.md](docs/specs/mcp-async-host-contract.md).

The CCAS-compatible Unix-domain app-server is layered on after the worker is
proven. That socket speaks the same Codex 0.149.0 subset Dolgorae requires as
a client; see [docs/protocol/](docs/protocol/). samchi-for-grok does not import
Dolgorae.

## Status

See [docs/roadmap/README.md](docs/roadmap/README.md). Next work is TASK-017
(app-server thread/turn). ACP go is
[ADR-0002](docs/architecture-decision-records/0002-grok-agent-stdio.md).
The TASK-004 live `grok agent stdio` capture is
[crates/adapter-grok/captures/task-004](crates/adapter-grok/captures/task-004).
The TASK-006 disk ledger lives in `crates/core` (`samchi-core`). The TASK-007
ACP adapter spawn and item mapper live in `crates/adapter-grok`.

## Build and test

```bash
make test
make build
./bin/samchi-for-grok version
```

`samchi-for-grok worker start|wait|status|result|list|cancel|followup|respond --json`
and `samchi-for-grok mcp` (spawn, await, wait, status, result, list, cancel,
followup, respond) are implemented. Host packaging lives in
[skills/use-samchi-for-grok](skills/use-samchi-for-grok/SKILL.md) and
[docs/ops](docs/ops/). `samchi-for-grok app-server --listen unix://<absolute-path>`
binds a Unix socket, upgrades HTTP/1.1 GET `/` to WebSocket, and answers
honest `initialize` / `initialized` / `account/read` / Grok `model/list`.
Occupied paths fail closed. Thread/turn methods are TASK-017.
