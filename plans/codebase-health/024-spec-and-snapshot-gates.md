# Plan 024: Spec gate — syntax-aware citations, close `MISSING` entries, snapshot review policy

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-xtask/src/docs/specs.rs docs/content/docs/reference/developer-reference/specs/ .github/workflows/ci.yml`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: L (11 characterization tests dominate)
- **Risk**: MED (gate-flip sequencing)
- **Depends on**: none
- **Category**: tests (spec enforcement)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Characterization item 3: "Add characterization tests for every `MISSING` entry, then make missing coverage fail the gate. Replace line-text citation matching with Rust syntax parsing: resolve the canonical suite, require a recognized executable test attribute, reject comments, strings, ordinary helper functions, and cfg-disabled tests, and reconcile citations with the test list produced by the runner." Today 13 `MISSING` cells remain (10 in `operator-console.mdx`, 1 in `auth-source-folder-sync.mdx`, plus re-count at execution time), the gate only WARNS on MISSING, and citation verification is line-prefix matching (`has_fn` accepts any same-named function — a helper fn with no assertions "proves" coverage). Item 4 adds: "absence of `*.pending-snap` files does not prove that snapshot changes were reviewed" — an author can hand-edit an accepted `.snap` to match buggy output and CI stays green; a reviewed-snapshot signal or `cargo insta` diff policy must be decided and enforced.

## Current state

- Gate: `crates/jackin-xtask/src/docs/specs.rs:58-61,75-77` — MISSING → `emit_warn`; broken citation → `bail!` (`:79-85`). Citation check `has_fn` at `:209-233` — line scan for `fn `/`pub fn `/`async fn ` prefixes; `verify_citation` (`:158-207`) never reconciles with the runner's test list.
- MISSING cells: `docs/content/docs/reference/developer-reference/specs/operator-console.mdx:21,32,33,39,40,41,47,48,54,55`; `auth-source-folder-sync.mdx:48`. `operator-console.mdx:72` self-describes the console manager as "under-specified in tests".
- Pending-snap gate (done): `.github/workflows/ci.yml:850-853`. Insta usage: `jackin-console`, `jackin-capsule` (18 committed `.snap`); shared normalizers `crates/jackin-test-support/src/snapshot.rs:13,42`.
- Coverage-map report exists (report-only, acceptable per roadmap wording): `crates/jackin-xtask/src/health.rs:256-263,685-688`.
- `cargo nextest list --message-format json` can enumerate real runnable tests (reconciliation source).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Spec gate | `cargo xtask docs specs` | exit 0 |
| Console tests | `cargo nextest run -p jackin-console -p jackin-instance` (auth-sync home — verify which crate owns it) | pass |
| Runner list | `cargo nextest list --message-format json > /tmp/tests.json` | JSON list |
| xtask tests | `cargo nextest run -p jackin-xtask` | pass |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `specs.rs` (+ tests), the 11+ characterization tests in the crates the spec cells point at (console `ManagerState`-family invariants, auth env-selected-path resolver), the spec MDX cells (MISSING → citations), a snapshot-review CI step, TESTING.md note for the snapshot policy.

**Out of scope**: writing new spec pages; the coverage-map fail-mode ("then consider failure" — a later decision); pending-snap gate (done).

## Git workflow

Branch `test/spec-gate-completion`; Conventional Commits; `git commit -s`; push per commit. Sequencing matters: tests → cells → gate flip (never flip first).

## Steps

### Step 1: Close the MISSING entries

Re-enumerate (`grep -rn "MISSING" docs/content/docs/reference/developer-reference/specs/`). For each: read the invariant row, write the characterization test in the owning crate (console invariants → `jackin-console` manager/state suites; auth-source-folder-sync → its resolver's suite), replace the cell with the `crate::module::tests::fn` citation per the existing citation format in those files.

**Verify**: `cargo nextest run` for owning crates → new tests pass; `grep -rn "MISSING" docs/content/docs/reference/developer-reference/specs/` → none; `cargo xtask docs specs` → exit 0.

### Step 2: Syntax-aware citation verification

Rewrite `has_fn`/`verify_citation`: parse the cited file with `syn`, resolve the cited path to an item, require a recognized test attribute (`#[test]`, `#[tokio::test]`, `#[rstest]` — enumerate what the repo actually uses: `grep -rhn "#\[\(tokio::\)\?test" crates --include='*.rs' | head`), reject matches in comments/strings (syn gives this for free), reject `#[cfg(...)]`-disabled-for-all-configs items where detectable, and reconcile: the cited test name must appear in `cargo nextest list` output (gate invokes it or consumes a generated artifact — pick the cheaper integration consistent with how other xtask gates shell out; `cmd.rs` helper exists).

**Verify**: `cargo nextest run -p jackin-xtask` → new negative fixtures pass (citation to helper fn → fail; to commented-out test → fail; to cfg-disabled → fail; to real test → pass); `cargo xtask docs specs` → exit 0 on the real tree (fix any loose citations it now exposes — expected churn).

### Step 3: Flip MISSING to failure

Change `emit_warn` → error for MISSING cells (or a ratcheted max-0 count). Safe now: step 1 emptied the set.

**Verify**: `cargo xtask docs specs` → exit 0; add a fixture test proving a MISSING cell fails the gate.

### Step 4: Snapshot review policy

Decide + implement the automated form (the decidable default): a CI step that lists changed `.snap` files against the PR merge-base (`git diff --name-only $(git merge-base origin/main HEAD) -- '*.snap'`) and (a) posts/echoes the list into the step summary so review is explicit, and (b) runs `cargo insta test --unreferenced=reject` (confirm flag support in the pinned insta version) to catch orphaned snapshots. Record the policy in TESTING.md ("changed .snap files are enumerated in CI; reviewer must acknowledge them — hand-edited snapshots are rejected in review").

**Verify**: `actionlint` clean; TESTING.md updated; `cargo xtask ci --fast` → exit 0.

## Test plan

Step 1's 11+ characterization tests (the deliverable); step 2's fixture negatives in `docs/specs` gate tests; step 3's failure fixture. Model characterization on the neighboring cited tests in the same suites.

## Done criteria

- [ ] Zero MISSING cells; every former cell cites a real, runner-reconciled test
- [ ] Citation gate is syn-based + runner-reconciled; negative fixtures prove rejection classes
- [ ] MISSING now fails the gate (fixture-proven)
- [ ] Snapshot-diff enumeration step + policy in TESTING.md
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- A MISSING invariant is untestable as stated (spec row ambiguous or the behavior doesn't exist) — do not write a vacuous test; report the row for spec correction.
- Runner reconciliation is too slow for the docs gate (>~30s added) — fall back to syn-only + a scheduled reconciliation lane, and record the tradeoff.
- Step 2 exposes >10 loose citations pointing at helpers — fixing them may require new real tests; if so report scope before writing them all here.

## Maintenance notes

- New spec rows must cite runner-known tests from birth; the gate now enforces it.
- The coverage-map fail-mode decision ("newly introduced high-risk modules") remains open — revisit once this gate has soaked.
