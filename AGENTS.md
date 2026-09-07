# AGENTS.md

Repository guidance for AI coding agents working on grokgrok.

The rules below are the local authority for how agents inspect, implement, and
verify work in this repository. They favor correctness and fail-closed behavior
over speed; apply them proportionally for trivial work.

## Product

`grokgrok` is a **working title** (rename later). It is **Grok-only**. Keep
the core free of Grok/Claude/GLM SDKs so a later Claude or zcode backend can
reuse that structure. Do not add Claude or GLM adapters in this repo. Do not
import CCAS. Claude stays on CCAS until a separate project extracts the core.

The MCP facade is a write-capable subagent (review *and* implementation).
Default spawn is `approvalPolicy=never` and `sandbox=workspace-write`. The
Grok adapter uses ACP (`grok agent stdio`), never `grok -p`. CLI `start`
stays in the foreground; MCP spawn may return because the server process
owns the child. Do not publish a tool before its Task implements it.

Language: **Rust** for core (ADR-0001).

Dolgorae is the **consumer reference** for the Codex app-server 0.149.0 subset
(`docs/protocol/`). grokgrok is a server on that wire. Do not claim a Dolgorae
Profile can launch grokgrok until Dolgorae itself accepts that executable.

## Core behavior

1. Inspect the requested code and its source of truth before changing it.
2. Implement the minimum change that fully satisfies the verified requirement.
3. Touch only what the requested outcome and its verification require.
4. Define success before implementation and continue until it is proved.

## Roadmap

[docs/roadmap/README.md](docs/roadmap/README.md) is the sole Phase, Epic, Task,
dependency, execution-order, and lifecycle authority. At most one Task may be
`In Progress` or `In Review`. Follow the dependency order. A completed Task is
immutable; later changes use a new Task.

Do not start EPIC-003 until EPIC-002 records a go ADR. Do not fall back to
`grok -p` on a no-go.

Internal worker types live in `crates/core` (`ggwire`) and are Codex app-server
`thread` / `turn` / `ThreadItem` from the pinned Dolgorae subset. Do not invent
an ad hoc `job` JSON. Do not change subset bytes without a new Task.

Claude/Codex completion is
[docs/specs/mcp-async-host-contract.md](docs/specs/mcp-async-host-contract.md):
`grok_spawn` once, then `grok_await`. Completion is a terminal TurnStatus
(`completed` / `interrupted` / `failed`). Do not poll status. Dead worker
generation fails immediately (`worker_gone`). Do not document `grok -p`.

ACP emit policy:
[docs/specs/acp-item-mapping.md](docs/specs/acp-item-mapping.md). TASK-007
must follow that table.

## Verification

`make test` is the repository acceptance gate. Makefile handlers are
authoritative; disagreement with [TESTING.md](TESTING.md) is a blocking
contract defect. Do not treat compilation alone as proof of live Grok
behavior when a Task requires a live capture.

## Documentation language

Normative docs and the roadmap are English.
