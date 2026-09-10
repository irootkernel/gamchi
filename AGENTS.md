# AGENTS.md

gamchi uses this file as local agent guidance.

## Core Behavior

### 1. Inspect Before Acting

- Resolve repository facts and named authorities before implementation.
- Inspect the requested code and its source of truth before changing it.
- State material assumptions, surface trade-offs, and ask when unresolved ambiguity would materially change the result.
- Push back when a request conflicts with repository authority, safety, or the user's stated goal.

### 2. Prefer the Smallest Complete Solution

- Implement the minimum change that fully satisfies the verified requirement and reuse established patterns.
- Avoid speculative features, abstractions, configurability, and compatibility layers.
- Simplify an implementation whose size or complexity is not justified by its behavior.

### 3. Prefer Durable Root-Cause Solutions

- For fixes and solution proposals, prefer the smallest complete approach that addresses the verified root cause, weighing correctness, performance, maintainability, and structural fit instead of optimizing for the smallest diff.
- Prefer durable designs over symptomatic patches while keeping the current work proportional to the verified requirement and repository authority.
- When a broader ideal design exceeds the current scope, implement a bounded durable step that fully satisfies current success criteria and preserves a clear path forward.
- Record only remaining independent actionable work in the repository's canonical `deferred-feedback` owner. If no owner exists, report the proposed entry and obtain approval before creating one.
- Promote epic-sized work to a TODO candidate or roadmap work unit. Do not defer work required for current correctness or acceptance.

### 4. Make Surgical Changes

- Touch only what the requested outcome and its verification require.
- Preserve unrelated work and match local style.
- Remove only artifacts made obsolete by the current change.

### 5. Work Toward Verifiable Goals

- Define success before implementation and continue until it is proved.
- Match verification strength to the claimed behavior and relevant failure paths.
- Continue until the result is verified or a concrete blocker is established; report skipped checks and remaining uncertainty.

## Master Preferences

- Respond to Master in Korean using polite speech. When directly addressing the user, use exactly `Master`.
- Keep repository artifacts in the repository's established language and style. When no convention exists, use English unless Master requests otherwise.
- Report concise conclusions and useful evidence without exposing private chain-of-thought.

## Aquarium Development Guide

- Use `/aquarium:task-handler` for one named roadmap task.
- Use `/aquarium:epic-handler` to implement one roadmap epic as sequential task goals.
- Use `/aquarium:epic-validator` to cold-validate and remediate one completed roadmap epic.
- Use `/aquarium:new-project`, `/aquarium:new-feature`, or `/aquarium:refactor` for an explicitly requested Ouroboros-assisted project or epic design workflow.
- Use `/aquarium:war-room` to diagnose one difficult bug and stop at a task, epic, or incomplete-investigation proposal.
- Use `/aquarium:dev-setup` to diagnose or configure development tooling and repository operating guidance.
- Use `/aquarium:docs-setup` to audit, establish, adopt, or migrate canonical documentation structure and roadmap IDs.
- Use `/aquarium:test-setup` to audit or configure the common Make or Bun testing contract and evidence-backed legacy waivers.
- Use `/aquarium:release-handler` for one stable release lifecycle and `/aquarium:release-qa` for its exact committed-candidate scenario verification.
- Use `/use-mulgae` for an authorized Mulgae review, run inspection, finding follow-up, configuration diagnosis, cleanup plan, or recovery.
- Use `/use-gaori` when a selected long or noisy check is routed through Gaori or existing Gaori evidence must be inspected.
- Let Aquarium workflows use Podway by default for Git-backed work unless the current user opts out before the first managed-session mutation. No Aquarium skill owns a Podway session; only when starting a different session should the workflow ask whether to preserve, finish, delete, or replace the existing one.
- Use `/use-podway` directly for an explicitly requested Procedure v2 lifecycle, goal, diagnosis, recovery, cancellation, or discard operation.
- Use the separately installed upstream `/humanize-korean` skill once as the final prose pass for Korean human-authored documentation. Keep its `_workspace/` output untracked and remove it after applying the accepted text.
- For that writing pass, preserve meaning, facts, code, commands, identifiers, URLs, citations, quotes, legal text, and generated content. Route mixed-language prose by block, and fail closed with the unchanged draft when the skill is unavailable or validation fails.
- Keep `.mulgae/**`, `.gaori/runs/**`, `.podway/runtime/**`, and disposable roots as local runtime evidence. Do not cite their paths or identities as durable evidence in tracked documentation or commit messages; use an approved tracked `aquarium.promoted-evidence/v1` package only when a downstream consumer genuinely requires retained evidence.

## Project Configuration

### Repository Index and Authorities

- Product: Grok-only worker (facades + extractable core + Grok ACP adapter). Command and crate identifier is `gamchi`.
- Language: Rust ([ADR-0001](docs/architecture-decision-records/0001-rust-core.md)).
- Roadmap: [docs/roadmap/README.md](docs/roadmap/README.md) is the sole Phase, Epic, Task, dependency, execution-order, and lifecycle authority.
- Specs: [docs/specs/](docs/specs/) (`product.md`, `mcp-async-host-contract.md`, `grok-launch.md`, `acp-item-mapping.md`).
- Architecture: [docs/architecture/core.md](docs/architecture/core.md).
- Protocol subset: [docs/protocol/](docs/protocol/).
- Workspace members: `crates/core` (`samchi-core`), `crates/docscheck`, `crates/gamchi`, `crates/adapter-grok` (`samchi-adapter-grok`).
- Acceptance gate: `make test`. Makefile handlers are authoritative; disagreement with [TESTING.md](TESTING.md) is a blocking contract defect.
- Other commands: `make test-prepare`, `make test-unit`, `make fmt-check`, `make clippy`, `make test-docs`, `make build`.
- Gaori command IDs: `test`, `test-prepare`, `test-unit`.
- Home resolution: `--home`, then `GAMCHI_HOME`, then `~/.gamchi`.
- Ignored local runtime includes `/bin/`, `/target/`, `.sorage/`, `.gamchi/`, `.omc/`, `.gaori/` except portable tester files, and after this setup `/.mulgae/*` except `/.mulgae/config.yaml`.

### Commit Messages

- Subject form: `[TASK-NNN] <imperative English subject>`.
- Use the roadmap task ID as written, for example `[TASK-007]`.
- This file is the commit-header authority. Recent Git history is not a second authority.
- Do not stage, commit, or push unless that exact Git action is separately authorized.

### Project-Specific Operating Rules

- `gamchi` is **Grok-only**. Keep the core free of Grok/Claude/GLM SDKs so a later Claude or zcode backend can reuse that structure. Do not add Claude or GLM adapters in this repo. Do not import CCAS. Claude stays on CCAS until a separate project extracts the core.
- The MCP facade is a write-capable subagent (review *and* implementation). Default spawn is `approvalPolicy=never` and `sandbox=workspace-write`. The Grok adapter uses ACP (`grok agent stdio`), never `grok -p`. CLI `start` stays in the foreground; MCP spawn may return because the server process owns the child. Do not publish a tool before its Task implements it.
- Dolgorae is the **consumer reference** for the Codex app-server 0.149.0 subset (`docs/protocol/`). gamchi is a server on that wire. Do not claim a Dolgorae Profile can launch gamchi until Dolgorae itself accepts that executable.
- At most one Task may be `In Progress` or `In Review`. Follow the dependency order. A completed Task is immutable; later changes use a new Task.
- Do not start EPIC-003 until EPIC-002 records a go ADR. Do not fall back to `grok -p` on a no-go. EPIC-002 already recorded a go ([ADR-0002](docs/architecture-decision-records/0002-grok-agent-stdio.md)); EPIC-003 is unblocked only for capabilities that ADR lists as observed.
- Internal worker types live in `crates/core` (`source_wire`) and are Codex app-server `thread` / `turn` / `ThreadItem` from the pinned Dolgorae subset. Do not invent an ad hoc `job` JSON. Do not change subset bytes without a new Task.
- Claude/Codex completion is [docs/specs/mcp-async-host-contract.md](docs/specs/mcp-async-host-contract.md): `grok_spawn` once, then `grok_await`. Completion is a terminal TurnStatus (`completed` / `interrupted` / `failed`). Do not poll status. Dead worker generation fails immediately (`worker_gone`). Do not document `grok -p`.
- ACP emit policy: [docs/specs/acp-item-mapping.md](docs/specs/acp-item-mapping.md). TASK-007 must follow that table.
- Do not treat compilation alone as proof of live Grok behavior when a Task requires a live capture.
- Normative docs and the roadmap are English.
