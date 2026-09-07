# grokgrok

Working title. **Grok-only** worker: facades + an extractable core + a Grok
ACP adapter. Claude and GLM are not this binary. A later backend may reuse
the core’s shape. CCAS stays Claude’s backend for now.

Parents delegate **review or implementation**. The Grok adapter uses ACP
(`grok agent stdio`), not print mode. MCP spawn defaults to yolo +
workspace-write.

```text
Claude Code / Codex --MCP--> facade
Dolgorae            --app-server--> facade
                              core  --adapter-grok--> grok agent stdio
```

See [docs/architecture/core.md](docs/architecture/core.md). Core language is
Rust ([ADR-0001](docs/architecture-decision-records/0001-rust-core.md)). The
core crate is `crates/core`; the `grokgrok` binary stub is `crates/grokgrok`;
roadmap docscheck is `crates/docscheck`.

The v1 host contract is MCP, Gaori-shaped: spawn returns an id immediately;
`grok_await` stays pending until the turn finishes; that return is the
completion signal. Details:
[docs/specs/mcp-async-host-contract.md](docs/specs/mcp-async-host-contract.md).

The CCAS-compatible Unix-domain app-server is layered on after the worker is
proven. That socket speaks the same Codex 0.149.0 subset Dolgorae requires as
a client; see [docs/protocol/](docs/protocol/). grokgrok does not import
Dolgorae.

## Status

See [docs/roadmap/README.md](docs/roadmap/README.md). Next work is TASK-003
(stub ACP agent in Rust).

## Build and test

```bash
make test
make build
./bin/grokgrok version
```

`worker`, `mcp`, and `app-server` are not implemented yet.
