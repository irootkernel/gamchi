# TODO

Language: English.

Future epic-sized candidates and temporary execution dossiers. This index
does not own roadmap identity or lifecycle status.

Current execution dossier: [TODO-EPIC-005.md](TODO-EPIC-005.md) (EPIC-005
App-server wire). This index does not record lifecycle status.

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
- Share follow-up `TurnRequest` assembly between the MCP and CLI
  facades (EPIC-004 confirmation Low F001). Re-enter when changing
  follow-up spawn fields so the two copies cannot diverge.
- Extract the MCP JSON-RPC test `Rpc` harness shared by `mcp_stdio`
  and the live_* tests (EPIC-004 confirmation Low F002). Re-enter
  when adding another MCP stdio test that would copy the child
  harness again. The confirmation review predates this note.
