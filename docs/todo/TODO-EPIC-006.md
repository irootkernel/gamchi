# EPIC-006 execution dossier

Language: English.

Status: Temporary execution SOT for EPIC-006. This file does not own roadmap
identity, ordering, dependencies, lifecycle vocabulary, or status.

Consumer epics: `EPIC-006` only.

## Goal

Parents choose a Grok model and reasoning effort on the same worker that
MCP, CLI, and app-server already use. Canonical goal:
[roadmap EPIC-006](../roadmap/README.md).

## Non-goals

Owned by [product.md](../specs/product.md) exclusions and earlier epics:

- `read-only` sandbox enforcement
- Changing the model on an existing thread
- `grok -p`, Claude/GLM adapters, importing CCAS or Dolgorae
- Teaching Dolgorae to select samchi-for-grok as a Profile
- Rewriting completed TASK-007, TASK-008, TASK-009, or TASK-016
- Parsing ignored Grok runtime files as catalog authority
- Promising `grok-4.5` + `xhigh` unless a live child start admits that pair
- Pre-admitting or rejecting spawn by parsing `grok models`
- Project-local / cwd config files
- Migrating a samchi TOML config (none exists; home YAML is new)

## Requirement owners

| Topic | Owner |
| --- | --- |
| Identity, order, lifecycle | [roadmap](../roadmap/README.md) |
| Omitted MCP spawn defaults and home config | [product.md](../specs/product.md), [grok-launch.md](../specs/grok-launch.md) (TASK-023) |
| Launch argv and child-start failure | [grok-launch.md](../specs/grok-launch.md) (TASK-023) |
| MCP/CLI/socket field behavior and thread lock | [mcp-async-host-contract.md](../specs/mcp-async-host-contract.md) (TASK-023) |
| Adapter owns catalog verification | [architecture/core.md](../architecture/core.md) |
| v1 `grok agent stdio` owner | [ADR-0002](../architecture-decision-records/0002-grok-agent-stdio.md) |

Do not copy those documents here. If they disagree after TASK-023, they
win; this dossier only maps them until then.

## Locked decisions

These were locked before the epic was adopted:

1. Samchi owns omitted defaults. Do not inherit `~/.grok/config.toml`.
2. Spawn passes the resolved `model` and `effort` to Grok. If the Grok
   child does not start, samchi returns that error. Do not pre-admit
   against `grok models`.
3. A samchi thread freezes `model` at `grok_spawn` / `thread/start`.
   Later `grok_followup` / `turn/start` may change `effort` only.

## Default resolution

Per spawn field (`model`, `effort`), the first **present** non-blank
value wins:

1. Explicit parent field (MCP, CLI, or socket)
2. Home config `<resolved-home>/config.yaml`
3. Built-in `grok-4.6` / `high`

Absent and blank are different:

| Input | Treatment |
| --- | --- |
| Parent field omitted | Skip to the next layer |
| Parent field present and blank or whitespace | `INVALID_CONFIG` |
| Config file missing, or key omitted | Skip that layer |
| Config key present and blank or whitespace | `INVALID_CONFIG` |
| Config file malformed, or unknown key | `INVALID_CONFIG` |

`resolved-home` is `--home`, then `SAMCHI_FOR_GROK_HOME`, then
`~/.samchi-for-grok` (the ledger home). There is no project-local
config.

Config is YAML with optional `default_model` and `default_effort`
keys only. Those keys fill omitted spawn `model` and `effort`.
They are not spawn field names.

```yaml
default_model: grok-4.6
default_effort: high
```

Spawn does not create config files. The ledger stays out of the
project directory by default.

The `use-samchi-for-grok` skill (TASK-025) tells the parent to add
`.samchi-for-grok/` to the consumer project's `.gitignore`. It does
not silently edit that file.

Do not parse `~/.grok/` caches. `grok models` may feed app-server
`model/list` advertisement only; it does not gate spawn.

After the cascade, `model` is never blank: omitted input becomes
`grok-4.6` unless an explicit or config value was present. The same
holds for `effort` and `high`. The literal id `grok` is the pre-EPIC-006
stub alias and always normalizes to `grok-4.6`, whether it came from
the parent, config, or a stored thread. Any other pair is passed to
Grok. Child start failure, ACP handshake failure, or a process error
is the spawn/turn error.

## Launch argv

Keep `--cwd` and `--sandbox` as top-level `grok` flags. Put `-m` and
`--reasoning-effort` on `agent`, matching the TASK-004 capture shape:

```text
grok --cwd <abs> --sandbox workspace agent --no-leader --always-approve -m grok-4.6 --reasoning-effort high stdio
```

Gated policies still omit `--always-approve` and set
`--permission-mode default`.

## Session lock

- First-turn omitted fields use the cascade above, then built-in
  `grok-4.6` / `high`.
- Later turn omitted effort uses the previous turn's effort when that
  value is non-blank. A stored empty effort (today's adapter writes
  `effort: ""`) is treated as omitted: home `default_effort`, else
  `high`. Follow-up does not re-resolve the thread model from config.
- Explicit later effort is passed to Grok after the blank/alias rules
  above. Child rejection fails the turn.
- Explicit later model that is not the thread model is refused by samchi
  before a new child starts, after normalizing `grok` to `grok-4.6` on
  both sides.
- The stored or parent-sent model id `grok` always normalizes to
  `grok-4.6`. There is no separate legacy marker. A model literally
  named `grok` is not supported.

## Member tasks

Order: TASK-023 → TASK-024 → TASK-025 → TASK-026 → TASK-027.
TASK-023 depends on TASK-019 (`Completed`).

| Task | Runtime owner | Verification | Must not |
| --- | --- | --- | --- |
| TASK-023 | Specs: grok-launch.md, mcp-async-host-contract.md, product.md omitted defaults and home `config.yaml`. This dossier stays the map | `make test` docscheck. Specs name the cascade, blank-vs-absent, `grok` alias, and empty stored effort | Implement argv; treat the current stub `model/list` as the final catalog |
| TASK-024 | Adapter reads home `config.yaml`, then `plan_launch` adds `-m` and `--reasoning-effort`. Child start/ACP failure is the error | Unit: built-in default, `default_model`/`default_effort` override, explicit wins, absent key skips, blank value refuses, `grok` → `grok-4.6`, empty stored effort → `high` (or home default). Offline fake agent still runs | Call live Grok from `make test`; pre-parse `grok models` as a spawn allowlist; read `~/.grok/`; add project-local config; treat config keys as `model`/`effort`; skip a present blank config key |
| TASK-025 | MCP `grok_spawn` / `grok_followup`, CLI `worker start` / `followup`, skill | Schema and CLI usage publish the fields. Follow-up model mismatch refuses. Skill recommends adding `.samchi-for-grok/` to the consumer `.gitignore` and does not edit that file | Change app-server in this Task; default to user Grok config; silently edit user gitignore |
| TASK-026 | App-server `model/list`, `thread/start` omitted model, `turn/start` model lock | Offline: listing may use a `grok models` fixture; listing failure is not a spawn gate. Model change fails closed. Effort may change | Rewrite TASK-016 history; claim Codex model names; refuse spawn because `model/list` failed |
| TASK-027 | Ignored live capture of a requested pair | Child argv and capture `model_id` match the request. Follow-up effort change. Follow-up model change refuses. Follow-up on a pre-EPIC-006 turn with empty effort uses `high` or home `default_effort`, not `""` | Treat compilation as live proof; skip fail-closed cases |

## Cross-document acceptance

EPIC-006 is ready for audit when TASK-023..027 are successfully terminal
and:

1. Omitted spawn uses explicit fields, then home `config.yaml`, then
   `grok-4.6` / `high` on the Grok argv.
2. Resolved model/effort are passed to Grok. Child start failure is
   the error. `grok models` does not gate spawn.
3. Thread model cannot change after `grok` → `grok-4.6` normalization.
   Effort can change on a later turn. Empty stored effort is omitted.
4. MCP, CLI, and app-server all pass the selected pair to the same
   adapter launch.
5. An ignored live test proves one requested pair on the child.

## Handoff

Implementation starts at TASK-023 through `/aquarium:task-handler`.
This design session does not implement, stage, commit, or publish.
