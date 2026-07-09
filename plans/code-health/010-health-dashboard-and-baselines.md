# Plan 010: Phase 0 — code-health dashboard, suppression inventory, verification matrix, baseline reconciliation

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- crates/jackin-xtask/ TESTING.md "docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx" file-size-budget.toml`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `47dd5fca0`, 2026-07-09

## Why this matters

The codebase-health roadmap ([docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx](../../docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx), "Phase 0") requires measuring before changing code: a fresh suppression inventory, a report-only dashboard, a contributor verification matrix, and baselines for every budget family later phases ratchet. None of that exists: suppression counts are hand-derived and already drift between documents (a retired plan said 66/54/25; the first-wave audit counted 47 `type_complexity` allows; a naive single-lint grep yields 44), TESTING.md has no "which verification tier for which change" guidance, and three of five budget families (suppression counts, agent-doc tokens, public-item counts) have no recorded floor. Later plans (011 suppression ratchet, 017 unified ratchet engine) consume the numbers this plan makes durable. This plan also corrects a stale roadmap claim discovered during the audit.

## Current state

- `crates/jackin-xtask/src/main.rs` — the xtask CLI. The `Command` enum (lines 44-121) has arms `Ci`, `Construct`, `Pr`, `PtyFixture`, `Change`, `Docs`, `Research`, `Roadmap`, `SchemaCheck`, `Lint`, `ProfileMatrix`, `ReleaseVerify` — no report/health command. `LintCommand` (lines 123-139) is `Files | Tests | Agents | AgentLinks | Arch`. `run_all_lints` (lines 146-152) chains the gates:

  ```rust
  fn run_all_lints(strict: bool) -> anyhow::Result<()> {
      lint::enforce()?;
      test_layout::enforce()?;
      agent_files::enforce()?;
      agent_links::enforce()?;
      arch::check(strict)
  }
  ```

- `crates/jackin-xtask/src/lint.rs` — the file-size gate. Reuse its patterns: `measure()`/`walk()` (lines 98-125) walk `crates/` and map every `.rs` file to a line count; reporting goes through a scoped `emit` helper:

  ```rust
  #[expect(
      clippy::print_stdout,
      reason = "jackin-xtask is a CLI; the lint report is its output"
  )]
  fn emit(line: &str) {
      println!("{line}");
  }
  ```

- `TESTING.md` — currently: nextest install/run recipes (lines 1-57), capsule fixture recording (lines 59-73), operator `--debug` validation rules (lines 75-101). No verification matrix, no per-crate narrowest-command map.
- `file-size-budget.toml` — `production_cap = 1850`, `test_cap = 10000`, exactly **one** grandfathered entry: `crates/jackin-runtime/src/runtime/image.rs` at `lines = 1938`. `test-layout-allowlist.toml` — empty (`files = []`).
- Roadmap page `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx`:
  - Line 23 claims "The file-size and test-layout ledgers were burned down to zero grandfathered entries" — **stale**: the file-size ledger has the `image.rs` entry above (the cap was tightened 2000→1850 after the burn-down, re-exposing it).
  - Line 87 says "The existing xtask grep that blocks `allow`/`expect` without `reason =`" — **no such gate exists** (the `LintCommand` enum above is exhaustive). Line 66 correctly says "Add an xtask or grep gate".
- Audit-measured inventory (2026-07-09, HEAD `47dd5fca0`) that the dashboard must reproduce mechanically — verify your implementation's totals against these within ±2 (they drift as commits land, hence the tolerance):
  - Suppression attributes under `crates/**/*.rs`: 348 `#[allow(`, 21 `#![allow(`, 150 `#[expect(`, 32 `#![expect(` (551 total). All 182 `expect`s carry `reason =`; 235 of 369 `allow`s are bare. 53 distinct lint names. Top bare-allow crates: jackin-console (~120), jackin-usage (~26), jackin-runtime (~25).
  - Largest production files: `crates/jackin-runtime/src/runtime/image.rs` 1938, `crates/jackin-term/src/grid.rs` 1760, `crates/jackin-capsule/src/session.rs` 1656. Largest tests.rs: `crates/jackin-runtime/src/runtime/launch/tests.rs` 8857, `crates/jackin-capsule/src/daemon/tests.rs` 7742.
  - Agent-doc bytes: root AGENTS.md 6,102; crates/AGENTS.md 14,114; 26 per-crate AGENTS.md total 12,193; 26 per-crate README.md total 52,630.
  - Public-item proxy (`^\s*pub (fn|struct|enum|trait|type|const|mod|use)`): jackin-core 312 (39 `pub mod`), jackin-config 196, jackin-protocol 165, jackin-term 154, jackin-env 99, jackin-manifest 41.
- Repo conventions to match: Rust 2024, no `mod.rs`, tests in sibling `tests.rs` (see `crates/AGENTS.md`); xtask modules are flat files under `crates/jackin-xtask/src/` with a `pub(crate) fn run(args)` entry (exemplar: `crates/jackin-xtask/src/lint.rs`); comments state non-obvious WHY only.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Build xtask | `cargo check -p jackin-xtask` | exit 0 |
| Xtask tests | `cargo nextest run -p jackin-xtask` | all pass |
| Lint touched crate | `cargo clippy -p jackin-xtask --all-targets -- -D warnings` | exit 0 |
| Format | `cargo fmt` then `cargo fmt --check` | exit 0 |
| Docs gates | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | all OK |
| Full local gate (before PR) | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope** (the only files you should modify/create):
- `crates/jackin-xtask/src/health.rs` (create) and `crates/jackin-xtask/src/health/tests.rs` (create)
- `crates/jackin-xtask/src/main.rs` (register the new command)
- `crates/jackin-xtask/README.md` (structure table row for the new module)
- `code-health-baseline.toml` (create, repo root)
- `TESTING.md` (add verification matrix + narrowest-command map)
- `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` (correct two stale claims; mark Phase 0 progress)

**Out of scope** (do NOT touch):
- `crates/jackin-xtask/src/lint.rs`, `test_layout.rs`, `arch.rs` — plans 012/017 restructure them; only *call* their patterns, do not edit them.
- Any lint table change in root `Cargo.toml` (plan 011).
- Any production crate outside jackin-xtask.
- `file-size-budget.toml` — do not "fix" the image.rs entry; the roadmap text is corrected instead.

## Git workflow

- Branch off `main`: `chore/code-health-dashboard` (confirm with the operator if a session branch already exists — repo rule: stay on the active branch).
- Conventional Commits, signed, pushed immediately after every commit: `git commit -s -m "feat(xtask): add code-health dashboard lane" && git push`.
- Open a PR to `main` when done; do not merge it (repo rule: per-PR operator confirmation).

## Steps

### Step 1: Add the `health` xtask module (human report)

Create `crates/jackin-xtask/src/health.rs` with `pub(crate) struct HealthArgs` (clap `Args`: `--format <human|json>` defaulting to human, `--write-baseline` flag) and `pub(crate) fn run(args) -> anyhow::Result<()>`. Compute, by walking `crates/` (reuse the recursive-walk shape from `lint.rs:108-125` — write your own copy in `health.rs`, do not modify `lint.rs`):

1. **Largest production files** (top 15, excluding `tests.rs`) and **largest tests.rs** (top 10), with line counts.
2. **Modules over 300 lines with no sibling tests**: production `.rs` > 300 lines where no `<stem>/tests.rs` exists next to it (this is the Phase 3 coverage-map report; report-only).
3. **Suppression inventory**: scan `.rs` sources for `#[allow(`, `#![allow(`, `#[expect(`, `#![expect(`. Parse the lint name list inside the parens (multiple lints per attribute possible; attribute may span lines — read the file content, not line-by-line regex) and whether `reason =` is present. Aggregate: totals per kind, per lint name, per crate; bare (reason-less) counts per crate.
4. **Public-surface proxy**: per crate, count of lines matching `^\s*pub (fn|struct|enum|trait|type|const|mod|use)` and the `pub mod` subset.
5. **Agent-doc byte/token counts**: byte size (and bytes/4 as the token approximation) for root `AGENTS.md`, `crates/AGENTS.md`, every `crates/*/AGENTS.md` and `crates/*/README.md`, plus each crate's `lib.rs`/`main.rs` leading `//!` block.

Print a sectioned human report through an `emit` helper carrying the same `#[expect(clippy::print_stdout, reason = ...)]` carve-out as `lint.rs:66-72`. Every section header names the roadmap phase that consumes it (e.g. `## Suppressions (Phase 1 ratchet input)`).

Register in `main.rs`: add `mod health;` and a `Health(health::HealthArgs)` arm to `Command` with doc comment `/// Report-only code-health dashboard (codebase-health-enforcement Phase 0).` — do **not** add it to `run_all_lints` (it is a report, not a gate).

**Verify**: `cargo check -p jackin-xtask` → exit 0; `cargo run -p jackin-xtask -- health | head -40` → sectioned report; suppression totals within ±2 of: allow 369, expect 182, bare-allow 235.

### Step 2: Add `--format json`

With `--format json`, emit one JSON object (serde_json, already a workspace dep used by xtask) with keys `largest_production_files`, `largest_test_files`, `untested_large_modules`, `suppressions` (with `by_lint`, `by_crate`, `bare_by_crate`), `pub_surface`, `agent_docs`. No prose on stdout in json mode. This is the first machine-readable xtask lane (the roadmap's "diagnostics are prompts" principle); keep the shape flat and stable.

**Verify**: `cargo run -p jackin-xtask -- health --format json | python3 -m json.tool > /dev/null` → exit 0.

### Step 3: Write the committed baseline

With `--write-baseline`, write `code-health-baseline.toml` at the repo root: a dated header comment (`# Generated by cargo xtask health --write-baseline. Phase 0 baseline; Phase 7's ratchet engine consumes these floors.`) and the aggregate numbers only (totals per budget family: suppression counts per lint per crate, bare counts per crate, doc byte totals per file, pub-item counts per crate, largest-file counts). No per-finding detail. Run it and commit the generated file.

**Verify**: `cargo run -p jackin-xtask -- health --write-baseline && git status --short` → shows only `code-health-baseline.toml`; the file parses: `python3 -c "import tomllib;tomllib.load(open('code-health-baseline.toml','rb'))"` → exit 0.

### Step 4: Tests

Create `crates/jackin-xtask/src/health/tests.rs` (declared from `health.rs` as `#[cfg(test)] mod tests;` — hard rule: tests in sibling file, never inline). Model after `crates/jackin-xtask/src/lint/tests.rs`. Cover: (a) suppression parsing on a multi-line, multi-lint attribute with and without `reason =` (feed a temp dir with fixture `.rs` files); (b) the 300-line/no-sibling-tests classifier; (c) json output parses and contains the five top-level keys.

**Verify**: `cargo nextest run -p jackin-xtask` → all pass, including the new tests.

### Step 5: TESTING.md verification matrix

Add a `## Verification matrix` section to `TESTING.md` (after the nextest recipes, before fixture recording) with a table: change surface → narrowest command → when to use. Rows, exactly these tiers:

| Change surface | Command | When |
|---|---|---|
| One module | `cargo nextest run -E 'test(/module::tests/)'` | inner loop |
| One crate | `cargo nextest run -p <crate>` + `cargo clippy -p <crate> --all-targets -- -D warnings` | before commit |
| Cross-crate Rust | `cargo xtask ci --fast` | before PR |
| Full non-Docker gate | `cargo xtask ci` | merge readiness |
| Container/runtime behavior | `cargo xtask ci --e2e` (Docker running) | capsule/runtime PRs |
| Docs/roadmap | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | any docs edit |
| TUI snapshots | `cargo nextest run -p jackin-capsule -p jackin-console` (insta snapshots live only in these two crates today) | TUI render changes |

Below the table add a short per-crate note: every crate is verified by `cargo nextest run -p <crate>`; exceptions worth naming — `jackin` E2E tests need `--features e2e --profile docker-e2e`, doctests need `cargo test --doc --workspace --locked`.

**Verify**: `grep -c "Verification matrix" TESTING.md` → 1.

### Step 6: Roadmap corrections

In `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx`:
1. Line 23: replace the "zero grandfathered entries" claim with the measured truth, e.g. "…the test-layout ledger was burned down to zero grandfathered entries, and the file-size ledger is down to one (`runtime/image.rs`, re-exposed when the cap tightened from 2000 to 1850)."
2. Line 87: change "The existing xtask grep that blocks…" to future tense ("A reason-gate xtask lane (Plan 011) becomes redundant once these two lints deny…") so the page stops describing a gate that does not exist.
3. In the Phase 0 section, mark items 1/3/4/5 as shipped by this change (inventory command, dashboard, TESTING matrix, baselines) and item 2 (codebase-map reconcile) as still open, referencing the first-wave `DOCS-codebase-map` finding.

Do not hard-wrap prose (repo rule: one paragraph = one line).

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` → all pass.

### Step 7: README + final gate

Add a `health.rs` row to the Structure table in `crates/jackin-xtask/README.md` (this is the "update the README on structural change" hard rule from `crates/AGENTS.md`). Then run the full local gate.

**Verify**: `cargo xtask ci --fast` → `ci gate OK`; `cargo run -p jackin-xtask -- health --format json | python3 -m json.tool > /dev/null` → exit 0.

## Test plan

- New: `crates/jackin-xtask/src/health/tests.rs` — suppression-attribute parsing (multi-line, multi-lint, reason detection), untested-large-module classifier, json shape. Pattern: `crates/jackin-xtask/src/lint/tests.rs`.
- Regression: full `cargo nextest run -p jackin-xtask` stays green.

## Done criteria

- [ ] `cargo run -p jackin-xtask -- health` prints all five sections; totals within ±2 of the audit numbers above
- [ ] `cargo run -p jackin-xtask -- health --format json` emits valid JSON with the five keys
- [ ] `code-health-baseline.toml` committed and parseable
- [ ] TESTING.md contains the verification matrix table
- [ ] Roadmap page no longer claims zero grandfathered entries nor an existing reason-gate
- [ ] `cargo xtask ci --fast` → `ci gate OK`
- [ ] `git status` clean except intended files; `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The `Command` enum in `main.rs` no longer matches the excerpt (another lane landed in the meantime) **and** a `health`/`dashboard`/`report` command already exists.
- Your suppression totals differ from the audit numbers by more than 25 in any direction (your parser is probably wrong — do not commit a bad baseline).
- `cargo xtask roadmap audit` fails for a reason unrelated to your edit.
- You find yourself wanting to edit `lint.rs`/`test_layout.rs` — that is plan 017's job.

## Maintenance notes

- Plan 011 (suppression ratchet) and plan 017 (unified ratchet engine) consume `code-health-baseline.toml`; keep its keys stable.
- The dashboard is report-only by design; do not wire it into `run_all_lints` or CI as a failing gate — Phase 7 decides what becomes a budget.
- Reviewer should scrutinize: the suppression parser's multi-line attribute handling (the known trap: `#[expect(\n    clippy::a,\n    clippy::b,\n    reason = "…"\n)]` counts as one attribute, two lints, reasoned).
