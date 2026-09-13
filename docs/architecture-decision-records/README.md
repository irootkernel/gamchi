# Architecture decision records

| ADR | Status | Decision |
| --- | --- | --- |
| [0001-rust-core.md](0001-rust-core.md) | Accepted | Grok-only product; Rust extractable core |
| [0002-grok-agent-stdio.md](0002-grok-agent-stdio.md) | Accepted | Live `grok agent stdio` is the v1 Grok worker (go) |
| [0003-developer-instructions-refuse.md](0003-developer-instructions-refuse.md) | Superseded | No spawn-owned CLI instruction channel on grok 1.0.25; refuse non-empty `developerInstructions` (superseded by ADR-0004) |
| [0004-developer-instructions-meta-rules.md](0004-developer-instructions-meta-rules.md) | Accepted | Apply via `session/new` `_meta.rules`; restore with `session/load` on grok 1.0.30. Current code still refuses until TASK-034 |

EPIC-002 TASK-005 recorded this go from the TASK-004 capture. `grok agent serve`
and `grok agent leader` are named; v1 still uses parent-owned `--no-leader` stdio.
