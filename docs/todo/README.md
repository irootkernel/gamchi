# TODO

Language: English.

Future epic-sized candidates and temporary execution dossiers. This index
does not own roadmap identity or lifecycle status.

No current execution dossier.

Future candidates (not lifecycle):

- Teardown should treat a zombie child as dead (`waitpid` `WNOHANG` or
  equivalent) so `grok_cancel` does not wait ~2.5s after SIGTERM
  (EPIC-004 confirmation Lows F001/F002).
- Add a deterministic test for the post-`set_child_pid` terminal-abort
  branch (EPIC-004 confirmation Low F003). The immediate-cancel MCP
  test can miss that branch.
