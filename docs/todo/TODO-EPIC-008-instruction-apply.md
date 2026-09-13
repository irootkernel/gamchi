# EPIC-008 execution map (instruction apply)

Authority: Temporary execution detail for
[EPIC-008](../roadmap/README.md#epic-008-generation-immutable-developer-instructions)
remaining work (TASK-033..TASK-035). This file does not own roadmap
identity or lifecycle status.

Language: English.

It incorporates the former root `TODO-FIX.md` brief and `FEEDBACK.md`
(2026-09-13, verdict REVISE then proceed). The roadmap wins for identity
and status. Review record: untracked `FEEDBACK.md` at the repository
root, if present.

This dossier does not authorize implementation, staging, committing, or
pushing. TASK-033 observed `_meta.rules` honor and `session/load`
restore ([ADR-0004](../architecture-decision-records/0004-developer-instructions-meta-rules.md)).
Member tasks TASK-033..035 are done. Remaining handler work is
whole-epic audit and last-consumer dossier closeout.

Reviewed baseline at dossier adoption: HEAD
`895dd17c1d0ad1fd53792b99bc01f92a68d2bd2f`. Roadmap member tasks for
remaining work are TASK-033, TASK-034, and TASK-035.

## 1. Product to-be (do not argue this)

Gamchi is a Grok-only worker that speaks the Dolgorae Codex app-server
0.149.0 subset. Dolgorae (later) chooses a backend and sends the **same**
socket methods to Codex or to Gamchi.

- Codex executes them natively.
- Gamchi **translates** them onto `grok agent stdio` and projects results
  back onto the same subset.

That is why Gamchi is a separate server, not a mux inside Dolgorae.

Role/purpose text is subset `developerInstructions` on `thread/start` and
`thread/resume`. Dolgorae starts a worker when it has a purpose (review
now, implement now). It does not pre-warm a thread pool and does not
change the role mid-thread.

Mapping to-be:

```text
app-server thread/start.developerInstructions
  -> freeze the exact string in the Gamchi thread ledger
  -> translate in samchi-adapter-grok
  -> session/new._meta.rules
```

The core stays backend-agnostic: it stores the frozen string. Only
`crates/adapter-grok` knows `_meta.rules`. A later Camchi/Zamchi adapter
would install the same stored string on a Claude or zcode channel. Do not
put Grok `_meta` into `samchi-core`.

Choosing the role at `thread/start` does **not** require spawning Grok
inside the `thread/start` handler. Keep the existing split:

```text
thread/start:
  validate inputs
  freeze the exact string in the ledger
  return the Gamchi thread

first turn/start:
  admit the turn
  spawn Grok and initialize ACP
  create the ACP session with rules from the frozen string
  durably save the ACP session ID
  only then send session/prompt

subsequent turn/start:
  admit the turn
  spawn a new Grok process and initialize ACP
  session/load the stored ACP session id (cwd + empty mcpServers)
  do not re-send _meta.rules
  only then send session/prompt
```

TASK-033 observed that sequence. Do not treat the diagram as a new
protocol guarantee beyond that capture.

Invariants: one immutable instruction string per Gamchi thread; one
continuous ACP conversation; no user prompt after a known
installation, restore, or session-ID persistence failure. A Gamchi
per-turn `generation_id` is not a new instruction generation.

This is **role-instruction attach**, not Dolgorae Profile attach.
Profile selection, `networkAccess: false`, `writableRoots`, and
unverified `sandbox=read-only` remain out of scope.

## 2. What shipped (as-is)

EPIC-008 (TASK-029, TASK-030, TASK-031) is marked `Completed`. Phase 6 is
marked `Completed`. Canonical outcomes currently say **refuse-only**.

Current behavior in `crates/gamchi/src/app_server.rs`:

- omit / JSON `null` / `""` on first `thread/start` → empty string, today's
  no-instruction path
- whitespace-only or wrong JSON type → refuse before admit
- non-empty `developerInstructions` on `thread/start` → refuse
  (`developerInstructions not supported`) at about line 710
- resume same empty value → succeed; resume change → refuse
- `turn/start` with a `developerInstructions` member → refuse
- a ledger thread that already stored non-empty text → refuse new turns
  at about lines 774-775

`crates/adapter-grok/src/turn.rs` `session/new` does not install
instruction metadata. After a turn it kills the Grok child (about line
400). Follow-up uses `session/load` when an ACP session id is stored.
`set_acp_session_id` currently discards the persistence result
(`let _ = ...` at about line 480) and can still send `session/prompt`.

ADR-0003 was `Accepted` at refuse-only closeout and is now
**Superseded** by ADR-0004. Do not install through a Grok CLI flag,
prompt-prepend, or a cwd instruction file. TASK-034 installs through
`session/new` `_meta.rules`. Current runtime still refuses.

`docs/specs/grok-launch.md` and `docs/specs/product.md` describe that
refuse-only contract as current behavior.

## 3. What was wrong

The to-be was translation. The closeout was refusal. Refusal was chosen
because TASK-030 concluded there was **no parent-owned apply channel**
that Grok honors on `grok agent stdio` 1.0.25.

That conclusion was incomplete. The remaining design blocker is not the
first-turn field. It is a demonstrated, specified way to keep the same
instructions and conversation when Gamchi starts the next Grok process.

Do not replace “safe refusal mistaken for feature completion” with
“first-turn success mistaken for thread-lifetime correctness.”

### 3.1 Capture that was done (TASK-030)

Live `grok agent stdio` (not `grok -p`, not the fake agent), authenticated,
unique tokens, honor = observable reply difference.

Recorded in `crates/adapter-grok/captures/task-030/metadata.json`:

- CLI `--rules` inline text: not honored
- CLI `--rules` absolute path outside cwd: not honored
- CLI `--system-prompt-override`: not honored
- cwd `AGENTS.md` / `extra.rules` / `instruction.rules`: honored as Grok
  project discovery

Cwd files were correctly rejected as an install path: they mutate the
user workspace and collide with files the user already owns.

Traffic dumps were not checked in. Exact `--rules` argv placement is not
in metadata.

### 3.2 Capture that was not done

Grok's agent-mode guide lists `session/new` `_meta` fields:

- `rules` — extra rules appended to the system prompt (additive)
- `systemPromptOverride` — replace the entire system prompt
- `yoloMode` — per-session always-approve

TASK-030 did not probe `_meta.rules` / `_meta.systemPromptOverride`.
ADR-0003 then rejected “invent ACP `_meta`” because there was no honor
capture. That is circular: the documented channel was never tested, then
forbidden for lack of a test.

`grok agent --help` does not list `--rules`. `--rules` is a top-level
`grok` flag (`--rules <TEXT>` in the CLI reference). TASK-030 treated a
TUI/headless text flag as the stdio install path.

The out-of-cwd file-path probe does **not** establish a supported
file-loading channel, because path dereferencing was not verified. The
CLI reference describes `--rules <TEXT>`, not a file path. Sandbox
interference is only a possible confound if file reading actually occurs.
Keep flag placement, argument semantics, file access, and model honor as
separate questions. CLI flags stay non-adopted for this fix.

The inline-text probe remains a distinct historical observation. Neither
it nor the file-path probe justifies “all parent-owned instruction
channels are unavailable.”

### 3.3 Reported live re-probe (2026-09-12)

Same worker: `grok 1.0.25` `agent --no-leader --always-approve stdio`,
model `grok-4.6`, effort `low`, empty temp cwd, unique tokens.

User prompt: if extra rules or a system-prompt override gave a secret
token, reply with exactly that token; else `NONE`.

| Probe | Reply | Honored |
| --- | --- | --- |
| control (no rules in `_meta`) | `NONE` | no |
| `session/new` `_meta.rules` | exact unique token | **yes** |
| `session/new` `_meta.systemPromptOverride` | exact unique token | **yes** |

Control `NONE` plus two different unique tokens rules out leftover
contamination from prior runs of those tokens.

**Evidence grade:** reported by the planning session; not independently
rerun by the 2026-09-13 reviewer. TASK-033 reproduced initial
`_meta.rules` delivery and restore with full provenance on grok
1.0.30: control `NONE`, first-turn CHECK_A, then a different process
`session/load` of the same ACP session id answered undisclosed
CHECK_B. Rules were not re-sent on load. See
`crates/adapter-grok/captures/task-033` and ADR-0004.

A cwd `AGENTS.md` probe in the same script returned `NONE`; the user
prompt named “extra rules or system-prompt override”, so that miss is
not used against TASK-030's cwd-honor result.

### 3.4 Why refusal was the wrong product closeout

Silent store-and-ignore is still invalid. Given a false “no channel”
finding, fail-closed refusal was safer than pretending attach worked.

Once `_meta.rules` honors the text, refuse-only is the wrong shipped
outcome for the product to-be. EPIC-008 as marked `Completed` means
“safe refusal”, not “Dolgorae-shaped `developerInstructions` reach
Grok”. Reviewers must not treat the roadmap stamp as attach-ready.

## 4. Roadmap constraints (do not violate)

From `docs/roadmap/README.md` and `AGENTS.md`:

- Allowed statuses: `Planned`, `In Progress`, `In Review`, `Completed`,
  `Blocked`, `Deferred`. There is **no** `Reopen`.
- A completed Task is **immutable**. Later changes use a **new Task**.
- At most one Task may be `In Progress` or `In Review`.
- Epic status is derived from its Tasks. Phase status is derived from
  its Epics. Derive EPIC-008 / Phase 6 from revised membership; do not
  invent lifecycle vocabulary.
- Each Epic has at most seven Tasks. EPIC-008 currently has three
  (TASK-029..031). TASK-032 already exists under EPIC-005. Next unused
  task id at this baseline is **TASK-033**. Re-read the roadmap before
  reserving ids.
- Commit subject form: `[TASK-NNN] <imperative English subject>`.
- Do not stage, commit, or push unless separately authorized.
- Normative docs and the roadmap stay English.
- Do not prepend instructions onto `session/prompt`.
- Do not fall back to `grok -p`.
- Core stays free of Grok/Claude/GLM SDKs.

## 5. How to fix (process)

Do **not**:

- Change TASK-029 / TASK-030 / TASK-031 status away from `Completed`
- Rewrite those rows' historical Done-when strings
- Invent status `Reopen`
- Delete or rewrite ADR-0003 history so apply looks original
- Erase refuse-only history
- Re-run `/aquarium:epic-handler` on the original three member tasks
- Claim Dolgorae Profile attach
- Implement apply by writing `AGENTS.md` / `extra.rules` into cwd
- Prompt-prepend the role text onto `session/prompt`
- Use `_meta.systemPromptOverride` for ordinary role/purpose text
- Put `_meta` handling in `samchi-core`
- Unconditionally set `_meta.yoloMode: true` when adding rules
- Guess a `session/load` instruction field or a synthetic user message
- Fall back to `session/new` with a new session id after a failed load
  (that discards conversation continuity and is not restore)
- Mark current-behavior specs as apply while the code still refuses
- Accept the epic before live restore proof
- Treat an unexecuted restore probe as “no channel”
- Convert authentication failure into “no channel”; that is `Blocked`

Do:

1. Add three new member tasks on **EPIC-008** (six total, under the cap).
2. Derive EPIC-008 and Phase 6 from the unfinished task (`Planned` until
   work starts).
3. Set Current Task / Next eligible to the first new task.
4. TASK-033: live-verify first-turn `_meta.rules` **and** same-session
   restore; write ADR-0004 (or next id) that supersedes ADR-0003; keep
   current-behavior specs truthful (runtime still refuses until TASK-034).
5. TASK-034: implement the observed design, update current-behavior specs
   with the code, record the pending TASK-035 live gate, run `make test`
   offline.
6. TASK-035: live app-server proof of delivery and restore; only then
   update epic Canonical Outcomes.
7. Execute with `/aquarium:task-handler` on one named new task, or
   `/aquarium:epic-handler` only for the **new** remaining member tasks.
   Do not revive TASK-031.

## 6. P1 blockers the old brief under-specified

### 6.1 Restore channel (must be observed before go-for-apply)

Grok's guide lists `_meta.rules` under `session/new`, not as a documented
instruction setter on `session/load`. Applicability to load cannot be
assumed from a metadata container existing on `session/new`.

Gamchi kills the Grok child after a turn. Restoration is the normal
second-turn path, not crash-only recovery.

TASK-033 must demonstrate, with process and session identifiers:

```text
first process: session/new with _meta.rules -> session/prompt
child exits
different process: initialize -> session/load of the same ACP session id
-> subsequent prompt still honors the frozen instructions
```

Specify one observed restore strategy: exact method, fields, ordering,
capabilities, and error handling.

| Observed result | Acceptable decision |
| --- | --- |
| Rules persist in the session; a new process restores them on load | **Observed (TASK-033):** `session/load` the stored ACP session id on a new process; do not re-send `_meta.rules` |
| Rules do not persist, but a same-session reapplication operation is verified | Specify and use that exact operation before prompting |
| No supported restore route is demonstrated | Keep the requirement unmet; do not invent a protocol operation |

Authentication or environment inability to run the experiment is
`Blocked`, not proof that no channel exists.

Live restore must not reuse a value already disclosed in conversation
history. Required challenge shape:

```text
Frozen instructions, installed once:
  For CHECK_A, reply with exactly <random value A>.
  For CHECK_B, reply with exactly <independent random value B>.
  Do not disclose the answer to a challenge that was not requested.

First process:
  User prompt: CHECK_A
  Expected answer: value A

After the first process has exited:
  Start another process and load the same ACP session.
  User prompt: CHECK_B
  Expected answer: value B
```

Generate new independent values per run. Neither value belongs in a user
prompt. Assert B was not disclosed by the first exchange, tool output, or
other captured material that can become ordinary history. Premature
disclosure is a contaminated test, not a pass. Run a no-instruction
control with the same challenge sequence (defined no-rule response such
as `NONE`, without disclosing A or B). Distinguish normal per-turn
teardown from a forced-crash test. Keep failed or contaminated attempts
in the evidence; do not retry until a favorable answer appears.

Honor still means behavior difference, not wire bytes alone. Combine
behavior with the protocol trace.

### 6.2 Legacy sessions that never received the install

Removing the blanket non-empty turn rejection exposes more than new
threads. The adapter chooses `session/new` vs `session/load` from the
stored ACP session id. A legacy thread may have non-empty ledger
instructions and an ACP session created without those instructions.

That is first-time installation into an existing session, not restoration.

| State | Required policy |
| --- | --- |
| Non-empty freeze, no ACP session id | Install on the first `session/new` |
| Session created with the verified instruction channel | Use the verified restore path |
| Existing session without reliable evidence of prior installation | Use a verified existing-session install route, or fail closed and require a new Gamchi thread |

Do not infer successful installation from non-empty ledger text or from
the mere presence of an ACP session id. Same-value `thread/resume`
success is not proof that Grok was installed or loaded.

If provenance is required, use the smallest justified durable
representation and define how old records are read. Keep Grok-specific
metadata out of the core. Do not build a general migration framework.

### 6.3 Session-id persistence must fail before prompt

If `session/new` succeeds but `set_acp_session_id` fails, the current
code can still prompt. The next turn may have no durable id to load.

TASK-034 must propagate that failure, stop before `session/prompt`,
publish terminal failure if the turn was already admitted, and clean up
the child. Add offline fault injection: no prompt sent; turn terminal
where persistence remains available; parent observes failure rather than
waiting. If storage also cannot publish terminal state, surface that
through the existing worker/transport error contract. Do not report
successful durable publication when it failed.

## 7. Proposed work units

Keep freeze, exact comparison (no trim), resume, and `turn/start`
forbidden-field rules from TASK-029. Replace only “no channel ⇒ refuse
non-empty start”. Do not merge TASK-033 into implementation. Do not give
TASK-034 and TASK-035 circular completion.

Roadmap ids are TASK-033, TASK-034, and TASK-035.

### TASK-033 — Verify delivery and restoration; adopt the bounded design

Depends on: TASK-031 (historical predecessor; do not reopen it)

Purpose: eliminate protocol uncertainty before behavior implementation.

Do:

- Reproduce complete evidence for initial `_meta.rules` honor (version,
  argv, model, effort, sandbox, approval, unique tokens, control `NONE`,
  source provenance). Treat the 2026-09-12 probe as a lead, not a
  substitute.
- Prove restore with Section 6.1 (R1 sequence + R2 CHECK_A/CHECK_B).
- Decide legacy-session handling (Section 6.2), persistence-failure
  behavior (Section 6.3), capability prerequisites, and the exact
  installation point (`session/new` `_meta.rules` only, unless restore
  observation shows a different verified operation).
- Write and index ADR-0004 (or next id). Supersede ADR-0003 without
  rewriting it. The new ADR may adopt a future implementation while
  stating that **current code still refuses**. ADR status is not
  implementation and not live acceptance.
- Do not change `grok-launch.md` / `product.md` current-behavior claims
  to apply in this task.

Do not:

- Adopt `systemPromptOverride` for v1 role text
- Invent a load-time field
- Recreate a session to fake restore
- Change subset JSON bytes
- Add an MCP `grok_spawn` instruction field
- Treat compilation or docs.x.ai text as live proof

Done when: first-turn delivery and same-session restore routes are
**observed and specified**, legacy policy is explicit, and the ADR
matches those observations. An unexecuted restore probe does not satisfy
this task.

Next: TASK-034. If blocked, do not bypass with guessed metadata.

### TASK-034 — Implement the adapter path and offline acceptance

Depends on: TASK-033

Purpose: implement the demonstrated design with deterministic coverage.
Fake-agent success is not live honor.

Do:

- Accept and freeze valid non-empty start instructions without weakening
  the rest of the TASK-029 input table.
- On the first actual adapter turn, install from the stored string using
  the verified `session/new` `_meta.rules` path. Subsequent turns follow
  the exact verified restore/legacy policy.
- Add **only** the instruction metadata required by that route. Do not
  unconditionally add `yoloMode`. Approval policy stays on existing CLI
  mapping (`never` → `--always-approve`; gated policies omit it).
- Propagate `set_acp_session_id` failure and stop before prompt.
- Offline tests: metadata construction, input table, approval policy with
  non-empty instructions (`untrusted` / `on-request` stay gated),
  persistence-failure fault injection, history-replay suppression,
  thread isolation (two threads, same cwd, different rules), legacy row,
  failure notification.
- Update current-behavior specs and TESTING.md with the code. Record that
  TASK-035 live gate is still pending. Do not claim epic acceptance.
- `make test` does not call live Grok.

Do not:

- Write cwd instruction files
- Prompt-prepend
- CLI `--rules` as the install path
- Fabricate a production “rules applied” acknowledgement the Grok
  protocol does not provide
- Change MCP tool names

Done when: implementation and current-behavior documents agree, and
required offline checks pass. This is an implementation milestone, not
end-to-end acceptance.

Next: TASK-035.

### TASK-035 — Validate the actual app-server path against live Grok

Depends on: TASK-034

Purpose: verify parent-to-Grok integration so a first-turn-only
implementation cannot be accepted.

Do:

- Ignored live test through Gamchi **app-server**, not a direct ACP
  client. Use intended sandbox and approval settings.
- No-rule control; initial apply (CHECK_A); after process replacement on
  the **same** ACP session, undisclosed CHECK_B.
- Same-value resume succeeds; different or emptied non-empty value
  refuses without mutating the freeze.
- Record full argv, binary version, model, effort, policies, test
  command, implementation revision, ACP methods, process/session ids,
  outcomes. Record auth success without credentials. Keep failed or
  contaminated attempts.
- Leave the task `Blocked` if authentication or the environment prevents
  execution. Fix observed behavioral failures before completion.
- Update EPIC-008 Canonical Outcomes only after this gate passes.

Done when: live app-server delivery and restoration pass against the
TASK-034 implementation, with Section 6 P1 items resolved.

Next: close EPIC-008 according to the roadmap. Dolgorae Profile attach
remains a separate future candidate.

## 8. Minimum acceptance matrix

| Scenario | Required result | Evidence |
| --- | --- | --- |
| Start omitted, null, or empty | Empty freeze; no `_meta.rules`; existing path | Offline |
| Start whitespace-only or non-string | Refuse before thread/turn admission | Offline |
| Start valid non-empty text, including surrounding whitespace | Preserve exact text; install before first prompt | Offline + live |
| Resume omitted/null or exact stored value | Keep freeze; no mutation | Offline |
| Resume changed value or emptying a non-empty value | Refuse without modifying stored instructions | Offline + live changed-value |
| Resume whitespace-only or wrong type | Refuse | Offline |
| `turn/start` includes `developerInstructions`, including null | Refuse the forbidden member | Offline |
| New process continues an initialized session | Same session and frozen rules; history is not new output | Offline + live CHECK_B |
| Legacy session lacks verified installation | Follow the explicit install-or-refuse policy | Offline; live for any newly adopted route |
| Session create/load, restore, or ID persistence fails | Do not prompt; expose terminal/error to parent | Offline fault injection |
| Gated policy with instructions | No accidental always-approve override | Offline; live policy evidence where claimed |
| Two threads in the same cwd use different rules | No cross-thread instruction leakage | Offline + bounded live |
| Instruction delivery vs history replay | No synthetic instruction user message; load replay is history, not new items or approvals | Offline + trace |
| Default working behavior | Additive rules do not replace the default agent system prompt; existing worker tests remain valid | Metadata assertions + regression suite |

Do not read this matrix as expanding sandbox profiles, network controls,
Profile selection, or arbitrary new protocol methods.

## 9. Out of scope (do not fold into this fix)

- Dolgorae Profile / adapter (other repository)
- `thread/fork`
- MCP `grok_spawn` instruction field
- Camchi / Zamchi adapters in this repo
- Accepting `networkAccess`, `writableRoots`, or unverified
  `sandbox=read-only`
- Inventing Podway `Reopen`
- Rewriting TASK-004 or TASK-030 capture bytes
- Re-investigating CLI `--rules` as a prerequisite for `_meta.rules`

The `docs/todo/README.md` bullet “Dolgorae attach after EPIC-008” stays
a future candidate. Instruction apply is a predecessor for that attach,
not a substitute.

## 10. Reviewer checklist

A reviewing agent should confirm or reject:

1. To-be is translation at **thread start** (ledger freeze then first-turn
   install), not mid-thread role change and not Grok spawn inside
   `thread/start`.
2. `_meta.rules` is the Grok install channel; `_meta.systemPromptOverride`
   is not the v1 role channel; `yoloMode` is not bundled in.
3. Completed TASK-029..031 stay `Completed`; new ids start at TASK-033
   after a fresh roadmap check.
4. ADR-0003 is superseded, not deleted; current-behavior specs stay
   refuse-only until TASK-034.
5. Restore is observed on a new process + same ACP session **before**
   go-for-apply; CHECK_B is not in first-turn history.
6. Legacy sessions and `set_acp_session_id` failure are specified.
7. Live app-server proof is mandatory for epic acceptance.
8. Core remains Grok-metadata-free.
9. This file does not itself change roadmap status.
