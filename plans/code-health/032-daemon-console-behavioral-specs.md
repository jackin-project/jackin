# Plan 032: Phase 3 — behavioral specs for the capsule daemon and the operator console state machine

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat c856acc9d..HEAD -- docs/content/docs/reference/developer-reference/specs/ crates/jackin-capsule/src/daemon.rs crates/jackin-console/src/tui/state/`
> Production-code drift here is EXPECTED over time and does not block — the
> spec documents current behavior; read the live code. A restructured specs
> directory (format change) IS a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M-L (two docs; deep code reading, zero code changes)
- **Risk**: LOW (docs-only; the risk is writing fiction — mitigated by the cite-or-MISSING rule)
- **Depends on**: plans/code-health/015-docs-gates-brand-specs-readme.md (soft — establishes the Tests-citation format and the `docs specs` gate; if 015 has not landed, use its citation format anyway: a `Tests` column with `crate::module::tests::fn_name` or `MISSING`)
- **Category**: docs / tests
- **Planned at**: commit `c856acc9d`, 2026-07-09

## Why this matters

Roadmap Phase 3 item 2 verbatim: "Write behavioral specs for the capsule daemon and operator console state machine. Each spec section should link to existing tests or explicitly mark missing coverage." These are the two highest-risk stateful subsystems without an invariant contract: the capsule daemon (`daemon.rs`, ~1490 lines production + a 7742-line `tests.rs`) owns client attach/displace, session lifecycle, input routing, and cleanup — and Phase 2 plans to decompose it into subsystems, a refactor the roadmap says must be characterization-first ("Keep the … Behavioral Spec as the oracle for any launch extraction" is the launch precedent; the daemon needs its own oracle before its decomposition); the console state machine (`tui/state/manager.rs`, 1228 lines) appeared on the audit's untested-large-modules list. A spec whose every invariant either cites a real test or says MISSING converts "we think the daemon preserves sessions on displace" into either a fact or a visible coverage gap — and the MISSING rows become the prioritized worklist for the Phase 2 characterization tests.

## Current state

Verified at the planning commit.

- Specs directory: `docs/content/docs/reference/developer-reference/specs/` — `index.mdx`, `meta.json`, and three specs: `runtime-launch.mdx`, `op-picker.mdx`, `auth-source-folder-sync.mdx`. Format exemplar (`runtime-launch.mdx:28-36`): an INV table —

  ```
  | INV | Description | Verify by |
  |---|---|---|
  | INV-1 | Trust confirmation runs before the image build — … | `confirm_trust_for_test` closure runs before `build_agent_image` in `load_role_with` |
  ```

  — plus surrounding sections ("Test seams", stage narratives). Post-015 the table gains a `Tests` column citing `crate::module::tests::fn_name` or `MISSING`; write the new specs WITH that column regardless of 015's status.
- Sidebar discipline (docs/CLAUDE.md hard rules): every new MDX under the reference tree must be registered in the section's `meta.json`, and `index.mdx` (`specs/index.mdx:8-11` lists the three current specs) must list the new pages. `cargo xtask research check`/`roadmap audit`/`docs repo-links` gate it.
- Subject 1 — capsule daemon: `crates/jackin-capsule/src/daemon.rs` plus its subsystem files (`daemon/` dir: `mouse_input.rs`, `input_dispatch.rs`, `file_export.rs`, more — `ls crates/jackin-capsule/src/daemon/`). The roadmap's own decomposition list names the subsystems to structure the spec by: client registry, session supervisor, active-client policy, status model, clipboard transfers, git/PR watch, usage totals, control-protocol routing. The 7742-line `daemon/tests.rs` is the citation source — it contains the echo-back render-conformance harness (TESTING.md:61 documents it) and behavior tests to mine.
- Subject 2 — operator console state machine: `crates/jackin-console/src/tui/state/manager.rs` (1228 lines, NO sibling tests per the audit's coverage-map baseline — expect many MISSING cells) plus the screens' model/update modules. Known test surfaces to mine for citations: `tui/model/tests.rs` (2292 lines), `tui/op_picker/tests.rs`, `screens/settings/update/tests.rs`.
- Known-behavior anchors the specs must cover (from recorded findings — each becomes an INV row, MISSING where no test exists): attach/displace ("second client displaces, input routes to B" — TEST-displace-policy recorded exactly one reattach test exists), PTY failure recovery (TEST-pty-recovery: `session.rs:470-555` writer-lock-failure/closed-channel/PTY-EOF paths unexercised), clipboard transfer expiry (plan 024 adds its tests — cite them if landed), status publication, persistence/reattach, cleanup outcomes.
- Audience: contributor docs (Behind jackin❯ — Internals) — Rust symbol names and file references are correct and expected here, via the RepoFile component for repo files.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Docs gates | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | all pass |
| Spec gate (post-015) | `cargo run -p jackin-xtask -- docs specs` | OK; new specs' citations verified |
| Cited-test existence (manual) | `rg -n 'fn <cited_name>' crates/<crate>/src/<module>/tests.rs` | 1 match per citation |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `docs/content/docs/reference/developer-reference/specs/capsule-daemon.mdx` (create)
- `docs/content/docs/reference/developer-reference/specs/operator-console.mdx` (create)
- `specs/meta.json` + `specs/index.mdx` (register both)
- Roadmap Phase 3 item 2 status; `plans/code-health/README.md` (row + strike the P3-08 deferred entry)

**Out of scope**:
- ANY code or test change (MISSING cells are the deliverable, not tests to write now)
- Rewriting the three existing specs (015 owns their Tests-column retrofit)
- The launch spec's territory (runtime-launch.mdx covers launch; the daemon spec starts where the capsule process starts)
- Speculative invariants — every INV row must be traced to code you read (file:line in the Verify-by cell) or to a test that asserts it; no aspirational behavior

## Git workflow

- Branch off `main`: `docs/daemon-console-specs`.
- One `docs(specs): …` commit per spec + one for registration. `-s`, push each. PR to `main`; do not merge.

## Steps

### Step 1: Mine the daemon

Read `crates/jackin-capsule/src/daemon.rs` and its `daemon/` subsystem files, then sweep `daemon/tests.rs` (`rg -n '^\s*(async )?fn ' crates/jackin-capsule/src/daemon/tests.rs | head -80` for the test inventory; read the tests behind the behaviors you spec). For each of the eight roadmap-named subsystems, extract 2-5 invariants that the CODE currently guarantees (e.g., from the recorded findings: what happens to the displaced client's stream on second attach; what state survives daemon restart; when cleanup classifies preserve-vs-teardown). Each invariant gets: a one-sentence description, a Verify-by cell citing the production symbol + file:line that implements it, and a Tests cell (`crate::module::tests::fn` or `MISSING`).

**Verify**: a draft INV table with ≥16 rows across the eight subsystems; every Verify-by cites code you opened.

### Step 2: Write `capsule-daemon.mdx`

Structure (mirror `runtime-launch.mdx`'s shape): frontmatter title; one-paragraph scope statement (what the daemon owns; where the spec starts/ends relative to the launch spec and the protocol); the INV table (grouped by subsystem with subheadings); a "Test seams" section naming the injection points a characterization test uses (the echo-back PTY harness per TESTING.md:61, the control-socket entry, plan 024's clock if landed); a "Missing coverage" section that lists every MISSING row again as the explicit worklist, ordered by risk (attach/displace and PTY recovery first, per the recorded findings).

**Verify**: page renders concerns — `cargo xtask docs repo-links` passes (RepoFile links resolve); every cited test name greps to exactly one `fn` (spot-check all, script it: extract citations, `rg` each).

### Step 3: Mine + write `operator-console.mdx`

Same process for the console state machine: read `tui/state/manager.rs` (what states exist, what transitions the manager owns, screen-stack/navigation rules, how services report back into state) and the screens' update modules; mine `tui/model/tests.rs` + settings/op-picker test files for citations. Expect a higher MISSING ratio (manager.rs has no sibling tests) — that asymmetry is signal, not a defect of the spec; say it in the Missing-coverage section. ≥10 INV rows.

**Verify**: same checks as Step 2.

### Step 4: Register + gates

Add both pages to `specs/meta.json` and `specs/index.mdx` (match the existing list format). Roadmap Phase 3 item 2 → shipped (specs exist; missing-coverage worklists recorded). Ledger: strike the P3-08 entry; note that the MISSING rows now feed the Phase 2 daemon-decomposition characterization work.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` → all pass; post-015: `cargo run -p jackin-xtask -- docs specs` → OK with the new specs counted; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

Docs-only. The verification IS: every citation resolves (scripted grep), every gate green, and the MISSING inventory complete (cross-check: the recorded TEST-displace-policy and TEST-pty-recovery gaps MUST appear as MISSING rows — if your reading found tests for them, the ledger was stale; note it).

## Done criteria

- [ ] Both specs exist, registered in meta.json + index.mdx, gates green
- [ ] Daemon spec: ≥16 INV rows across the eight subsystems; console spec: ≥10 rows
- [ ] Every INV cites production code (file:line/symbol) AND a Tests cell (real fn or MISSING)
- [ ] Every cited test fn greps to exactly one definition
- [ ] Missing-coverage sections enumerate the MISSING rows as ordered worklists
- [ ] Roadmap + ledger updated; `plans/code-health/README.md` row updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

- You cannot determine an invariant from reading (ambiguous concurrent behavior in the daemon's event loop) — write NO row for it and list it under an "Undetermined behaviors" note instead; >5 such items means the spec needs a maintainer session, report.
- The specs directory format has changed from the INV-table shape (015 or later restructured it) — adopt the new shape, and if none is discernible, STOP.
- daemon/tests.rs turns out to cover a recorded-as-MISSING behavior extensively (ledger stale) — cite it, note the correction, continue.
- Writing either spec would exceed ~250 lines of MDX — over-specification; tighten to the highest-value invariants and note the cut.

## Maintenance notes

- These specs are the oracle for the Phase 2 daemon decomposition and the deferred sim-harness work (turmoil/proptest target the invariants named here); the MISSING worklists are the characterization-test queue.
- The spec↔test gate (015) keeps citations alive; new daemon/console behavior PRs should add or update INV rows — reviewers should ask "which INV covers this?".
- Reviewer should scrutinize: that Verify-by cells cite code, not intentions, and that the displaced-client and PTY-recovery rows match what the code actually does today (the two places fiction is most tempting).
