# Plan 017: Phase 7 — unified ratchet engine (`ratchet.toml`) and the defect→gate ledger

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- crates/jackin-xtask/src/lint.rs crates/jackin-xtask/src/test_layout.rs file-size-budget.toml test-layout-allowlist.toml suppression-budget.toml`
> Plans 010/011 are dependencies and will have changed jackin-xtask since
> `47dd5fca0` — that is expected. What must still match the excerpts below:
> the shrink-only semantics of `lint.rs:220-244` and `test_layout.rs:243-282`.
> On a mismatch there, STOP.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED (migrating two green production gates onto a shared engine must preserve exact pass/fail semantics)
- **Depends on**: plans/code-health/010 (metric functions in `health.rs`), plans/code-health/011 (`suppression-budget.toml` + gate)
- **Category**: tech-debt
- **Planned at**: commit `47dd5fca0`, 2026-07-09

## Why this matters

Phase 7 item 2: one declarative `ratchet.toml` plus one engine implementing shrink-only semantics once, powering every budget the roadmap names. Today the two live ratchets hand-roll the same algorithm against incompatible schemas — `lint.rs` (file size: `{production_cap, test_cap, [[production]]{path,lines}}`) and `test_layout.rs` (layout: `{files=[str]}`) — and after plan 011 a third copy exists (`suppression-budget.toml`). Every future budget family (doc tokens, suite wall time, perf/alloc, pub-item counts) would mean a fourth, fifth, sixth reimplementation; the audit counted 5 of 6 roadmap budget families with no metric provider at all. Fragmentation already produces drift (ci.yml's file-size-gate comment still says the 2000L cap; the real cap is 1850). Separately, Phase 7 item 1's defect→gate ledger — the mechanism that makes "every escaped defect becomes a gate" (roadmap line 36) real — does not exist, even though both panic hooks already capture escaped defects (`crates/jackin-usage/src/logging.rs:147-164` capsule, `crates/jackin-diagnostics/src/run.rs:917-928` host `run.error_typed("panic", …)`).

## Current state

- Shrink-only semantics to preserve exactly, verified by direct read:
  - `lint.rs:220-244` `check_budget_entry`: budgeted row for a **missing file** fails ("delete the stale budget row"); file **at/under cap** fails ("no longer needs grandfathering"); **measured < budgeted** fails — the gate force-tightens ("shrink the budget row to {measured}"); `measured == budgeted (>cap)` is the only steady state; growth is flagged by the counts loop (`lint.rs:188-192`) and unlisted-over-cap by `lint.rs:193-197`.
  - `test_layout.rs:243-282` `check`: allowlist row not in current violations = stale → fail ("remove the stale allowlist entry"); unlisted violation → fail; success line reports counts; failure text appends the fix + rerun command (`cargo xtask lint tests --print-allowlist`).
  - Refresh flows: `lint files --print-budget` (`lint.rs:40-48`, regenerates the TOML to stdout), `lint tests --print-allowlist`.
- Config files at `47dd5fca0`: `file-size-budget.toml` (`production_cap = 1850`, `test_cap = 10000`, one production row: `crates/jackin-runtime/src/runtime/image.rs` @1938); `test-layout-allowlist.toml` (`files = []`); post-011: `suppression-budget.toml` (per-crate bare-allow counts).
- Metric sources available per roadmap budget family (audit): file size — `lint.rs::measure()`; suppression counts — plan 010's `health.rs` scanner; doc token counts — plan 010's `health.rs` agent-doc byte counts; complexity thresholds — static `clippy.toml:10-13` (clippy itself enforces; the ratchet's job is only to record the configured values so the self-tightening lane can compare measured maxima later); suite wall time — plan 013's junit artifacts (CI-side; **not** locally computable — see design note in Step 1); perf/alloc — plan 014's criterion lane + dhat literals (not yet budgetable; out of scope); public-item counts — `health.rs` pub-surface proxy.
- Panic-capture points for the ledger: `crates/jackin-usage/src/logging.rs:147-164` (capsule hook writes `[jackin-capsule] PANIC: …` + backtrace to the multiplexer log and bridges to OTLP); `crates/jackin-diagnostics/src/run.rs:917-928` (host hook emits `run.error_typed("panic", …)` into the run JSONL/OTLP).
- Other ledger-like files deliberately **not** folded (audit P7-04): `deny.toml` license exceptions/`bans.skip` (cargo-deny owns their semantics; stale entries warn, by design), `.codebook.toml` (a growing allowlist, opposite direction). The engine's schema must still record that decision.
- Conventions: xtask module + sibling tests; gate failure text states fix + rerun command; no `mod.rs`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Xtask tests | `cargo nextest run -p jackin-xtask` | all pass |
| All gates | `cargo run -p jackin-xtask -- lint --strict` | every gate OK |
| Single gates (compat) | `cargo run -p jackin-xtask -- lint files` / `lint tests` / `lint suppressions` | OK each |
| New engine directly | `cargo run -p jackin-xtask -- lint ratchet` | `ratchet OK — N entries` |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-xtask/src/ratchet.rs` (create: engine) + `ratchet/tests.rs`
- `crates/jackin-xtask/src/lint.rs`, `test_layout.rs`, `suppressions.rs` (become thin adapters over the engine; CLI surface unchanged)
- `ratchet.toml` (create, repo root), replacing the three config files after migration
- `docs/defect-ledger.md` — wait: contributor-facing non-rendered docs live at repo root in this repo (TESTING.md, ENGINEERING.md). Create `DEFECT_LEDGER.md` at the repo root instead, and reference it from the roadmap page.
- `.github/workflows/ci.yml` file-size-gate job comment (fix the stale 2000L text while touching this area)
- Roadmap Phase 7 status
- `crates/jackin-xtask/README.md`

**Out of scope**:
- Perf/alloc budgets, suite-wall-time budget enforcement (their metric sources are CI artifacts, not local measurements — the schema reserves the families; wiring is a later wave)
- Self-tightening scheduled lane (auto-PR) and the agent-operated hygiene lane (need this engine first; recorded)
- deny.toml/.codebook.toml folding (decided against — record in the ledger schema comment)
- Any change to what currently passes/fails: this is a pure refactor of mechanism plus one new report-only family

## Git workflow

- Branch off `main`: `feat/unified-ratchet-engine`.
- Commit sequence matters: engine+tests first, then one migration commit per gate, then config swap — each commit green. `-s`, push per commit. PR to `main`; do not merge.

## Steps

### Step 1: Engine + schema

Create `crates/jackin-xtask/src/ratchet.rs`:

```toml
# ratchet.toml — unified shrink-only budgets (codebase-health-enforcement Phase 7).
# Each entry: a metric (computed by a named provider in jackin-xtask), a bound,
# and shrink-only semantics: measured > bound fails (growth); measured < bound
# fails telling you to tighten the bound; measured == bound is steady state.
# `kind = "presence"` entries are allowlists: listed keys must still be violations,
# unlisted violations fail. Regenerate any family: cargo xtask lint ratchet --print <family>.
# deny.toml exceptions and .codebook.toml are deliberately NOT folded here:
# cargo-deny/codebook own their semantics.

[[family]]
id = "file-size-production"
provider = "file_lines_production"   # provider fn name in ratchet.rs registry
cap = 1850                            # unlisted entries must be <= cap
[[family.entry]]
key = "crates/jackin-runtime/src/runtime/image.rs"
bound = 1938
```

Engine responsibilities: parse `ratchet.toml`; for each family call its provider (a `fn(&Path) -> Result<BTreeMap<String, usize>>` registered in a match/registry); apply the exact `check_budget_entry` semantics ported from `lint.rs:220-244` for numeric families and the `test_layout.rs:243-282` stale/new semantics for presence families; aggregate failures with per-family fix text + rerun command (`cargo xtask lint ratchet --print <family>`); `--print <family>` regenerates that family's entries to stdout (port of `--print-budget`/`--print-allowlist`). Port the semantics functions as pure `pub(crate) fn`s so tests drive them without cargo.

**Verify**: `cargo nextest run -p jackin-xtask` → new `ratchet/tests.rs` unit tests pass (write them in this step: growth-fail, shrink-force-fail, stale-row-fail, unlisted-over-cap-fail, presence stale/new, steady-state pass — port the assertions from the existing `lint/tests.rs` and `test_layout/tests.rs` cases so behavior is provably identical).

### Step 2: Migrate the file-size family

Move the two file-size families (production/test caps) into `ratchet.toml` with providers `file_lines_production` / `file_lines_test` (implementation moves from `lint.rs::measure`+`walk`). `lint.rs::run/enforce` becomes a shim calling the engine scoped to those families, keeping `cargo xtask lint files` and `--print-budget` working verbatim (`--print-budget` maps to the engine's print for both families). Delete `file-size-budget.toml` in the same commit its content lands in `ratchet.toml`. Keep every existing test in `lint/tests.rs` passing against the shim (adjust construction, not assertions — the assertions ARE the characterization).

**Verify**: `cargo run -p jackin-xtask -- lint files` → `file-size budget OK …` (same message shape); `cargo nextest run -p jackin-xtask` → all pass; temporarily grow a grandfathered bound in ratchet.toml → `lint files` fails demanding shrink; revert probe.

### Step 3: Migrate test-layout and suppressions families

Same treatment: `test-layout-allowlist.toml` → presence family `test-layout`; `suppression-budget.toml` (plan 011) → numeric family `bare-allow-per-crate` with provider delegating to the health.rs scanner. CLI compat: `lint tests` / `lint suppressions` keep working, `--print-allowlist` maps through. Delete both old TOMLs in their migration commits.

**Verify**: `cargo run -p jackin-xtask -- lint --strict` → all gates OK; `git ls-files | grep -E 'file-size-budget|test-layout-allowlist|suppression-budget'` → empty.

### Step 4: New report-only family: agent-doc tokens

Add family `agent-doc-bytes` (provider from plan 010's agent-doc measurement; entries = the byte counts of root AGENTS.md, crates/AGENTS.md, each per-crate AGENTS.md, each `crates/*/README.md`, and each crate's `lib.rs`/`main.rs` leading `//!` block — all three surfaces roadmap Phase 6 item 7 names, and all already measured by 010's Step 1.5 provider — seeded from `code-health-baseline.toml`). Mark it `mode = "report"` in the schema (new field: `enforce` default true; `report` families print deltas but never fail) — the roadmap says budgets start advisory. This proves the engine handles a third metric kind and gives Phase 6's context-economy budget its first data.

**Verify**: `cargo run -p jackin-xtask -- lint ratchet` output includes an `agent-doc-bytes (report-only)` section; gate still exits 0.

### Step 5: Defect→gate ledger

Create `DEFECT_LEDGER.md` at the repo root: purpose paragraph (one row per escaped defect — a bug that reached an operator or the installed panic hooks: capsule `crates/jackin-usage/src/logging.rs:147` / host `crates/jackin-diagnostics/src/run.rs:917`), then a table `| Date | Symptom | Root cause | Characterization test | Gate/lint/budget adopted (or reason none) |`. Seed it with the three first-wave defect plans as historical rows (004 resize frame-drop, 007 OSC-8 map growth, 008 finalization teardown — data from `plans/code-health/README.md`), each noting which Phase 1 lint family or gate covers its class. Append-only; reviewed when choosing the next lint adoption. Reference it from roadmap Phase 7 item 1 (repo-file link) and from the roadmap's "Every escaped defect becomes a gate" principle.

**Verify**: `cargo xtask docs repo-links` → pass (the roadmap link to the new file resolves).

### Step 6: Cleanup + roadmap

- Fix ci.yml's file-size-gate job comment (near line 447) still describing the 2000L cap — reference `ratchet.toml` and the 1850 value's home.
- Roadmap Phase 7: item 1 shipped (ledger), item 2 shipped (engine; families live: file-size ×2, test-layout, bare-allow, agent-doc report-only; reserved: perf/alloc, wall-time, pub-items), items 3-6 open. Update xtask README (ratchet.rs row; lint.rs/test_layout.rs descriptions now "adapter over ratchet engine").

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`; `actionlint .github/workflows/ci.yml` → exit 0.

## Test plan

- `ratchet/tests.rs`: the six semantics cases (Step 1) + config parse errors + report-only mode never fails + `--print` round-trip.
- Ported: every pre-existing assertion in `lint/tests.rs` and `test_layout/tests.rs` still passes (characterization of unchanged behavior).
- End-to-end probes in Steps 2-3 (grow/shrink/stale) executed and reverted.

## Done criteria

- [ ] `ratchet.toml` holds all four migrated/new families; the three old TOMLs deleted from git
- [ ] `cargo xtask lint files|tests|suppressions` CLI behavior unchanged (messages may name ratchet.toml as the config home)
- [ ] Engine semantics tests pass, incl. shrink-forcing and stale-row cases for both numeric and presence kinds
- [ ] `DEFECT_LEDGER.md` exists, seeded, linked from the roadmap
- [ ] ci.yml stale cap comment fixed
- [ ] `cargo xtask ci --fast` → `ci gate OK`; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Plans 010/011 have not landed (no health scanner / no suppression gate to port).
- Any pre-existing `lint/tests.rs` or `test_layout/tests.rs` assertion cannot be kept passing without changing its expected behavior — semantics drift is the one forbidden outcome.
- The engine design forces changing a committed budget number to migrate it (numbers must transfer verbatim).
- You are tempted to fold deny.toml/.codebook.toml after all — that decision is recorded as no.

## Maintenance notes

- Future budget families (suite wall time from plan 013's junit, perf from plan 014's lane, pub-item counts) are one provider fn + one `[[family]]` block each — that is the payoff; reviewers should reject any new standalone budget TOML from here on.
- The self-tightening scheduled lane (Phase 7 item 3) reads `--print <family>` output to propose tightenings — keep that output committable-verbatim.
- Reviewer should scrutinize: the migration commits' test diffs (assertions must be untouched) and the engine's failure-message parity with the old gates (agents parse these).
