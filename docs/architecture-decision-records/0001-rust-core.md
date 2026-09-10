# ADR-0001: Rust core inside a Grok-only product

- Status: Accepted
- Decided: 2026-09-07

## Decision

**gamchi is Grok-only.** Implement its supervision **core in Rust**, kept
free of Grok SDK imports, so a later Claude or zcode/GLM backend can extract
or copy that core. MCP and app-server are facades. ACP is the Grok adapter.

## Context

Go was chosen only to resemble CCAS. Dolgorae (the client we must satisfy) is
Rust. Grok ACP is native to a Rust agent. CCAS must not be imported.

A future GLM backend should not clone ledger/transport. That reuse is
**structure**, not a second adapter living in this binary.

## Rejected alternatives

- **Keep Go** to match CCAS and Gaori. Rejected: we do not import CCAS; Gaori
  is an await pattern, not a library we link.
- **gamchi embeds Claude and GLM adapters.** Rejected: this product is Grok.
- **Replace CCAS in v1.** Rejected: Claude stays on CCAS until a separate
  backend project reuses this core.

## Consequences

- Convert the Go skeleton before TASK-003 (TASK-021).
- Core crate has no Grok/Claude/GLM dependencies.
- Grok adapter crate uses `agent-client-protocol`.
- MCP facade uses a Rust MCP SDK.
