# Architecture decision records

| ADR | Status | Decision |
| --- | --- | --- |
| [0001-rust-core.md](0001-rust-core.md) | Accepted | Grok-only product; Rust extractable core |
| [0002-grok-agent-stdio.md](0002-grok-agent-stdio.md) | Accepted | Live `grok agent stdio` is the v1 Grok worker (go) |

EPIC-002 TASK-005 recorded this go from the TASK-004 capture. `grok agent serve`
and `grok agent leader` are named; v1 still uses parent-owned `--no-leader` stdio.
