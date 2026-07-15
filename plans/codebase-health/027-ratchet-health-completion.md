# Plan 027: Ratchet & health completion — suite-time/public-surface providers, per-main trends, JSON diagnostics, doc budgets, fs ordering

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-xtask/src/ratchet.rs crates/jackin-xtask/src/health.rs ratchet.toml .github/workflows/`
> Mismatch with "Current state" = STOP. Land plan 010 first (trustworthy suppression numbers); plan 019 ships the public-surface provider — if it landed, skip that slice here.

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED (new gates start advisory)
- **Depends on**: plans/codebase-health/010; overlaps 019 (public-surface) — reconcile
- **Category**: dx (self-measuring maintenance)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Self-measuring-maintenance items 1 and 3 plus feedback-loop leftovers: the ratchet program lists "suppression and complexity ceilings, suite time, documentation/context size, public surface, and eligible performance budgets" — suite-time and public-surface providers don't exist, complexity is a static clippy threshold rather than a measured ceiling, and the `agent-doc-bytes` family is report-only ("never fails"), so AGENTS/README token budgets are observed but unbounded. Health trends: `cargo xtask health` is invoked by NO workflow, the baseline is a one-time snapshot, and no per-main series or tightening proposal exists — the shrink-only program has no headroom signal. Feedback-loop: gates other than `health` have no `--format json`, no GitHub problem matcher exists, and nothing enforces deterministic filesystem ordering in gate code (unsorted `read_dir` iteration can make gate output platform-dependent).

## Current state

- Providers (`crates/jackin-xtask/src/ratchet.rs:362-384`): `file_lines_*`, `bare_allow_per_crate`, `expect_per_lint_crate`, `agent_doc_bytes`, `export_volume_constants`, `perf_dhat_budgets`, `test_layout_violations`. Shrink-only + rerun messages work (`ratchet.rs:263-278`).
- `agent-doc-bytes`: `ratchet.toml:365-370` `mode = "report"`; measurement at `ratchet.rs:478`.
- Suite time: `code-health-baseline.toml:3-4` "pending first PR CI run"; nextest junit/wall-time data published by `rust-nextest.yml:134-166` (slowest tests + shard wall-time).
- Health: report sections at `health.rs:672-756` (incl. public-surface report `:711`, verification map `:630`); `--format json` exists (`main.rs:122`); not in any workflow (`grep health .github/workflows/` → comments only).
- JSON diagnostics: only `health` has `--format json`; `lint files`/`lint agents` have `--format json|github` per TESTING.md:71-73 — VERIFY which gates already support it (`grep -rn "format" crates/jackin-xtask/src/main.rs | head -30`) and close only the real gaps. No `.github/problem-matchers/`.
- fs ordering: `clippy.toml` disallowed-methods has no `read_dir`; unsorted iteration sites in gate code, e.g. `health.rs:533`, `docs.rs:829,1056,1067` (some callers sort downstream — the gap is enforcement).
- Float-equality + rust-analyzer stats + `.git-blame-ignore-revs` + crate headers: already DONE (audited) — don't redo.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| xtask tests | `cargo nextest run -p jackin-xtask` | pass |
| Ratchet | `cargo xtask lint ratchet` | exit 0 |
| Health | `cargo xtask health --format json` | valid JSON |
| Workflow lint | `actionlint` | clean |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `suite_time` provider (consuming the nextest junit artifact schema — read `rust-nextest.yml:134-166` for its artifact shape) + family (scheduled enforcement, tolerance-banded); `agent-doc-bytes` flip to enforce with seeded per-file bounds at current maxima; a scheduled `health-trend` job appending `health --format json` output to an artifact-based series + a trend section in the report + a tightening-proposal output ("N budgets with ≥20% headroom for ≥4 weeks"); shared `--format json` (file/line/message/fix/rerun) across `lint`/`docs` gates that lack it + one repo problem matcher; `read_dir_sorted` helper + enforcement (disallowed-methods entry or xtask self-check) for gate code.

**Out of scope**: public-surface provider if plan 019 shipped it (check first); the agent-legibility task suite (owned by its separate roadmap item).

## Git workflow

Branch `feat/ratchet-health-completion`; Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: `suite_time` provider + family

Provider parses the junit/wall-time artifact (or a locally-generated `cargo nextest run --profile ci` timing output — pick the input that exists deterministically; scheduled-lane enforcement avoids PR flakiness). Family seeded from current measured totals per shard, tolerance band ~15%.

**Verify**: provider unit tests with a fixture junit; `cargo xtask lint ratchet` → exit 0.

### Step 2: Enforce doc budgets

Flip `agent-doc-bytes` to `enforce`; seed per-file bounds at current measured maxima (shrink-only). Worst offenders get bounds equal to today's size — no content edits in this plan.

**Verify**: `cargo xtask lint ratchet` → exit 0; temporarily grow a doc locally to prove it trips (don't commit).

### Step 3: Health trend + tightening proposal

Scheduled workflow job: run `cargo xtask health --format json`, append (with commit SHA + date) to a rolling artifact (or a committed `health-history.jsonl` if the operator prefers repo history — default to artifact to avoid churn; note the choice); extend the health report with a trend delta section (current vs N-runs-ago) and a "tightening proposal" list (families whose measured value sits ≥20% under bound across the window).

**Verify**: `workflow_dispatch` run produces the artifact + report section; `cargo nextest run -p jackin-xtask` → pass.

### Step 4: JSON diagnostics + problem matcher

Census which xtask gates lack `--format json`; add the shared diagnostic struct (file/line/message/fix/rerun) and wire it into the gaps; add `.github/problem-matchers/xtask.json` + registration step so gate failures annotate PRs.

**Verify**: each converted gate: `cargo xtask <gate> --format json` emits valid JSON on a forced-failure fixture; `actionlint` clean.

### Step 5: Deterministic fs ordering

Add `read_dir_sorted` helper in xtask; convert gate-code `read_dir` iteration sites; enforce via a `clippy.toml` disallowed-methods entry for `std::fs::read_dir` scoped by narrow expects at the helper (or an xtask self-check if the clippy entry is too broad for non-gate code — record which; plan 011's inventory owns the final placement).

**Verify**: `cargo nextest run -p jackin-xtask` → pass; gate outputs stable across two runs (`diff <(cargo xtask health --format json) <(cargo xtask health --format json)` modulo timestamps).

## Test plan

Provider fixtures (junit sample, health-history sample), forced-failure JSON emission tests, ordering determinism check. Model providers on `measure_agent_doc_bytes`.

## Done criteria

- [x] `suite-time` family live (scheduled enforcement); `agent-doc-bytes` enforcing with seeded maxima
- [ ] Per-main health series + trend section + tightening proposal exist with an observed run
- [x] All first-party gates emit structured JSON; problem matcher registered
- [x] Gate code uses sorted directory iteration with an enforcement mechanism
- [ ] `cargo xtask ci --fast` exits 0; status row updated; measured-complexity family enforced

## STOP conditions

- Junit artifact schema unavailable/unstable across shards — report; suite-time may need its own emission step first.
- Health JSON output too unstable for trending (nondeterministic fields) — fix ordering first (step 5 before step 3) or report.
- Problem-matcher regex can't express a gate's output — restructure that gate's message format only with a compatibility note.

## Maintenance notes

- Tightening proposals are proposals: a human ratchets bounds down via the regenerate command, never automatically (roadmap: "after measured headroom and a safe update mechanism exist").
- New gates must ship with JSON output + rerun command from birth.

## Execution notes

Landed 2026-07-14 on `chore/codebase-health-plans`.

**Delivered**
- `suite-time` provider + family at `mode=enforce` with `junit_total_ms` ceiling; **always measures** (0 when no junit.xml) so the family never skips; growth-only hard fail (headroom Shrink is advisory).
- `agent-doc-bytes` flipped to `enforce` with seeded maxima.
- Scheduled `health-trend` job in `hygiene.yml` (health JSON artifact + step summary).
- Health report `trend` section + tightening proposals (agent-doc headroom vs ratchet bounds).
- `fs_util::read_dir_sorted` + tests; brand gate and public-surface measure use it.
- Source walks exclude nested `target`, `node_modules`, and `.git` trees, so
  building an excluded first-party crate cannot change health or suppression
  measurements; a fixture proves generated Rust files stay out of the census.
- Problem matcher: `.github/problem-matchers/xtask.json` registered in `ci.yml` lint job and `docs.yml`.
- Measured `rust-function-complexity` provider parses production Rust with
  `syn`, records each crate's current maximum control-flow decision count, and
  enforces shrink-only growth with an unlisted cap of 20. Clippy retains the
  independent absolute cognitive-complexity threshold.
- Shared schema-1 diagnostics cover all ten lint gates plus `docs repo-links`,
  `docs brand`, `docs specs`, `docs map-check`, `research check`, and `roadmap
  audit`. Every violation contains `file`, nullable `line`, `message`, `fix`,
  and `rerun`; a forced-error reporter fixture asserts all five keys.

**Index deviation (audit 2026-07-15)**: demoted from DONE to IN PROGRESS — health multi-run series + sorted-`read_dir` enforcement incomplete; see implementer audit rollup.
