# Documentation

Language: English.

## Role ownership

| Role | Canonical owner | Responsibility |
| --- | --- | --- |
| Roadmap | [roadmap/](roadmap/) | Sole Phase, Epic, Task, dependency, execution-order, and lifecycle authority |
| Specifications | [specs/](specs/) | Product baseline, MCP await, Grok launch, ACP mapping |
| Protocol artifacts | [protocol/](protocol/) | Pinned Dolgorae Codex 0.149.0 consumer subset and samchi-for-grok wire identity |
| Architecture | [architecture/](architecture/) | Core / facade / adapter split |
| Architecture decisions | [architecture-decision-records/](architecture-decision-records/) | Accepted implementation decisions after roadmap settlement |

The [roadmap](roadmap/README.md) owns delivery identity, order, dependencies,
and lifecycle status. [Product](specs/product.md) is the in-repo requirements
baseline.

## Authority and precedence

1. [Roadmap](roadmap/README.md) owns delivery identity, order, dependencies,
   and lifecycle status.
2. [Product](specs/product.md) owns scope, defaults, and exclusions.
3. [Core](architecture/core.md) owns Grok-only vs extractable core.
   [ADR-0001](architecture-decision-records/0001-rust-core.md) owns Rust.
4. [MCP async host contract](specs/mcp-async-host-contract.md),
   [Grok launch](specs/grok-launch.md), and
   [ACP item mapping](specs/acp-item-mapping.md) own host/adapter contracts.
5. [Protocol artifacts](protocol/) pin the Dolgorae consumer subset. A digest
   mismatch is a validation failure.
