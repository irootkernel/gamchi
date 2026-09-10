# TODO

Language: English.

Future epic-sized candidates and temporary execution dossiers. This index
does not own roadmap identity or lifecycle status.

No current execution dossier. This index does not record lifecycle status.

Future candidates (not lifecycle):

- Teardown should treat a zombie child as dead (`waitpid` `WNOHANG` or
  equivalent) so `grok_cancel` does not wait ~2.5s after SIGTERM
  (EPIC-004 confirmation Lows F001/F002).
- Add a deterministic test for the post-`set_child_pid` terminal-abort
  branch (EPIC-004 confirmation Low F003). The immediate-cancel MCP
  test can miss that branch.
- Buffer `session/load` replay and reconcile it against the ledger by
  stable ACP ids, or record discard as v1 policy in
  [acp-item-mapping.md](../specs/acp-item-mapping.md) (EPIC-004
  validation Low F004). Re-enter if a consumer needs load-replay
  history merged without duplicating items. TASK-012 acceptance still
  holds: replay is history, not new items or approvals.
- Extract the MCP JSON-RPC test `Rpc` harness shared by `mcp_stdio`
  and the live_* tests (EPIC-004 confirmation Low F002). Re-enter
  when adding another MCP stdio test that would copy the child
  harness again. The confirmation review predates this note.
- When two prior turns on one thread share `started_epoch`,
  `previous_turn_effort` tie-breaks on process-prefixed turn ids
  instead of start time (EPIC-006 validation Low F002). Re-enter
  if omitted follow-up effort must stay correct for sub-second or
  cross-process turns. TASK-025 sequential follow-up still holds.
- Add a unit test that a duplicate `default_model` or
  `default_effort` key in home `config.yaml` is `INVALID_CONFIG`
  (EPIC-006 confirmation Low F001). Re-enter when extending the
  YAML parser. TASK-024 blank-vs-absent still holds.
- Move `previous_turn_effort` into the ledger as a thread-scoped
  query (EPIC-006 confirmation Low F003). Re-enter if adapter-layer
  turn scans become a maintenance cost. TASK-025 follow-up still
  holds.
- Assert ACP capture `model_id` in the ignored `live_model` test
  (EPIC-006 confirmation Low F004). Re-enter if a consumer needs
  session/update model bytes beyond stored `turn.model`. TASK-027
  argv proof still holds.
- Add a test that a broken `GAMCHI_MODEL_LIST_FIXTURE`
  still returns the stub `model/list` row (EPIC-006 confirmation
  Low F007). Re-enter when changing advertisement fallback.
  TASK-026 listing-is-not-a-spawn-gate still holds.
  The confirmation review predates this note.
