# Gamchi

**Grok-only** worker: facades + an extractable core + a Grok ACP adapter.
Claude and GLM are not this binary. A later backend may reuse the core’s
shape. CCAS stays Claude’s backend for now.

The command and crate identifier is `gamchi`. Parents delegate
**review or implementation**. The Grok adapter uses ACP (`grok agent stdio`),
not print mode. MCP spawn defaults to yolo + workspace-write.

```text
Claude Code / Codex --MCP--> facade
Dolgorae            --app-server--> facade
                              core  --adapter-grok--> grok agent stdio
```

See [docs/architecture/core.md](docs/architecture/core.md). Core language is
Rust ([ADR-0001](docs/architecture-decision-records/0001-rust-core.md)). The
core crate is `crates/core` (`samchi-core`); the `gamchi` binary stub
is `crates/gamchi`; the Grok ACP adapter (fake agent + stdio harness)
is `crates/adapter-grok` (`samchi-adapter-grok`); roadmap docscheck is
`crates/docscheck`.

The v1 host contract is MCP, Gaori-shaped: spawn returns an id immediately;
`grok_await` stays pending until the turn finishes; that return is the
completion signal. Details:
[docs/specs/mcp-async-host-contract.md](docs/specs/mcp-async-host-contract.md).

The CCAS-compatible Unix-domain app-server is layered on after the worker is
proven. That socket speaks the same Codex 0.149.0 subset Dolgorae requires as
a client; see [docs/protocol/](docs/protocol/). gamchi does not import
Dolgorae.

## Status

See [docs/roadmap/README.md](docs/roadmap/README.md). EPIC-006 lets parents
choose a Grok model and reasoning effort on the same worker. EPIC-005 published the
CCAS-shaped app-server, five named consumer scenarios, and `grok/runtime/read`.
ACP go is
[ADR-0002](docs/architecture-decision-records/0002-grok-agent-stdio.md).
The TASK-004 live `grok agent stdio` capture is
[crates/adapter-grok/captures/task-004](crates/adapter-grok/captures/task-004).
The TASK-006 disk ledger lives in `crates/core` (`samchi-core`). The TASK-007
ACP adapter spawn and item mapper live in `crates/adapter-grok`.

## Build and test

```bash
make test
make build
./bin/gamchi version
```

`gamchi worker start|wait|status|result|list|cancel|followup|respond --json`
and `gamchi mcp` (spawn, await, wait, status, result, list, cancel,
followup, respond) are implemented. Host packaging lives in
[skills/use-gamchi](skills/use-gamchi/SKILL.md) and
[docs/ops](docs/ops/). `gamchi app-server --listen unix://<absolute-path>`
binds a Unix socket, upgrades HTTP/1.1 GET `/` to WebSocket, and answers
honest `initialize` / `initialized` / `account/read` / Grok `model/list`.
Occupied paths fail closed. `thread/start`, `thread/resume`, `thread/read`,
`turn/start`, and `turn/interrupt` use the same home ledger as MCP spawn.
Omitted socket sandbox and approvalPolicy stay `read-only` / `untrusted`.
`thread/fork` is recognized and fail-closed. The socket emits `thread/started`, `item/started`, `item/completed`, and
`turn/completed`. Approvals are subset server requests on the existing
pending-approval ledger, not MCP `grok_respond`.
`optOutNotificationMethods` stays empty. `grok/runtime/read` returns
`runtime: grok`, the same `userAgent`, and the resolved ledger `home`.
A Dolgorae-shaped client proves five named scenarios (`probe`, `first-turn`,
`follow-up`, `approval`, `interrupt`) offline. `thread/fork` stays fail-closed.
