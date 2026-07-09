# Plan 015: Phase 5 — brand-prose lint, spec↔test citations, README-freshness gate

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- crates/jackin-xtask/src/docs.rs crates/jackin-xtask/src/agent_files.rs docs/content/docs/reference/developer-reference/specs/ APPLE_CONTAINER_RESEARCH.md`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW-MED (the brand fixes rewrite prose sentences; meaning must be preserved)
- **Depends on**: none
- **Category**: docs
- **Planned at**: commit `47dd5fca0`, 2026-07-09

## Why this matters

Roadmap Phase 5 items 8-10 make three documentation rules mechanical instead of review-enforced. Measured state: (1) the `jackin❯` brand rule (RULES.md) has ~39 real violations today — 37 concentrated in `APPLE_CONTAINER_RESEARCH.md` as `jackin'` possessives, 2 in research MDX — and no gate, so new ones land freely; (2) the three behavioral specs cite production symbols in their "Verify by" columns but zero test ids, so specs can rot into fiction with nothing failing (the roadmap's spec↔test linkage gate has literally nothing parseable to check); (3) the "update the README on any structural change" hard rule in `crates/AGENTS.md` is enforced by nothing — `cargo xtask lint agents` checks only that `AGENTS.md` exists and `CLAUDE.md` is a symlink; it never looks at `README.md`, not even for presence.

## Current state

- Brand rule (`RULES.md`): product name is always `jackin❯` in prose; never `jackin'`, `Jackin`, `Jackin'`; bare `jackin` only for code identifiers/commands/paths. Measured violations (audit, verified by direct read):
  - `APPLE_CONTAINER_RESEARCH.md` — 37 hits, e.g. line 9: "…Apple just validated the entire jackin' architecture… That is jackin's four-layer model… jackin' is building the product version…"; line 36 table header "Why it matters to jackin'".
  - `docs/content/docs/reference/research/jackin-context-engine/02-architecture.mdx` and `06-routing-and-fleet-economics.mdx` — 1 each.
  - False positives that must stay legal: `RULES.md:17`, `AGENTS.md:12`, `CLAUDE.md:12` list the forbidden spellings inside backticks as rule examples. Capital `Jackin` has 0 real hits (3 rule-file mentions only).
- Specs (`docs/content/docs/reference/developer-reference/specs/`): `runtime-launch.mdx` (7 INV rows), `op-picker.mdx` (4), `auth-source-folder-sync.mdx` (8), plus `index.mdx`/`meta.json`. Citation format (verified, `runtime-launch.mdx:28-36`): a three-column table `| INV | Description | Verify by |` where "Verify by" holds backticked **production** symbols, e.g. INV-1's "``confirm_trust_for_test`` closure runs before ``build_agent_image`` in ``load_role_with``". No test-function ids anywhere.
- `crates/jackin-xtask/src/agent_files.rs` `check()` (verified, lines ~71-91): per `crates/*/` dir it asserts `AGENTS.md` is a file and `CLAUDE.md` is a symlink (with target check); `README.md` is never mentioned.
- `crates/jackin-xtask/src/docs.rs` (33.5K) hosts the docs gates: `run_docs` (repo-links), `run_research`, `run_roadmap`, invoked from main.rs arms `Docs`/`Research`/`Roadmap`. A new brand check belongs beside them as a new `DocsCommand` variant (read the existing enum in docs.rs before adding — match its clap structure).
- Docs conventions that bind this plan (from `docs/CLAUDE.md` + RULES.md): never hard-wrap prose; roadmap changes must keep `cargo xtask roadmap audit` green; possessive-awkward sentences are rewritten, not apostrophized.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Xtask build/tests | `cargo check -p jackin-xtask` / `cargo nextest run -p jackin-xtask` | exit 0 / all pass |
| New gate | `cargo run -p jackin-xtask -- docs brand` | `brand gate OK …` |
| Spec gate | `cargo run -p jackin-xtask -- docs specs` | `spec gate OK …` |
| Docs gates | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | pass |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-xtask/src/docs.rs` (two new subcommands) + its `docs/` test module (follow where existing docs tests live — check for `crates/jackin-xtask/src/docs/tests.rs`)
- `crates/jackin-xtask/src/agent_files.rs` (README presence assertion) + its tests
- `.github/workflows/docs.yml` OR `ci.yml` — wire `docs brand` + `docs specs` where the other `cargo xtask docs`/`roadmap` gates already run (find the existing invocation in the workflows and add alongside; if they only run locally/in docs.yml, follow that precedent)
- `APPLE_CONTAINER_RESEARCH.md`, the two research MDX files (brand fixes)
- The three spec MDX files (add Tests citations)
- `crates/jackin-xtask/README.md`, roadmap Phase 5 status

**Out of scope**:
- The documented-command drift gate (206 `jackin …` invocations vs the clap tree) — recorded next wave; extraction/normalization needs its own design.
- The crate-README→Fumadocs extraction pipeline and Codebase-Map slimming (next wave).
- Rewriting spec *content* beyond adding the Tests column/lines.
- Renaming/moving any file.

## Git workflow

- Branch off `main`: `feat/docs-gates-brand-specs`.
- Conventional Commits, `-s`, push after every commit. PR to `main`; do not merge.

## Steps

### Step 1: `cargo xtask docs brand`

Add a `Brand` variant to the docs command enum in `docs.rs`. Implementation:
- Scan prose surfaces: `**/*.md` at repo root, `crates/*/README.md`, `crates/*/AGENTS.md`, `docs/content/**/*.mdx` (skip `node_modules`, `target`).
- Strip code regions before matching: fenced blocks (``` … ```), inline code spans (`` `…` ``), and URLs (`http…` tokens). This is the false-positive shape that matters — the three rule files carry forbidden spellings only inside backticks, so after stripping they need no allowlist. Keep an explicit allowlist mechanism anyway (a small const list of `path:substring` pairs) for future rule-example prose, starting empty.
- Fail on any remaining match of `jackin'` / `Jackin` / `Jackin'` with file:line, the matched text, and the fix instruction ("write `jackin❯`; for possessives rewrite the sentence — RULES.md").
- Report success as `brand gate OK — N files scanned`.

**Verify**: `cargo run -p jackin-xtask -- docs brand` → currently FAILS listing ~39 hits in the three files named above (that failing list is your Step 2 worklist); unit tests pass.

### Step 2: Fix the existing brand violations

- `APPLE_CONTAINER_RESEARCH.md` (37): rewrite each sentence. Possessives (`jackin's four-layer model`) become rewrites ("the jackin❯ four-layer model" or restructure); bare `jackin'` references become `jackin❯`. Preserve technical meaning exactly; these are research-record sentences — change only the brand token and whatever minimal grammar the rewrite forces.
- The two research MDX files: same treatment (1 hit each).

**Verify**: `cargo run -p jackin-xtask -- docs brand` → `brand gate OK`; `git diff --stat` shows only the three prose files; spot-read 5 rewritten sentences for preserved meaning.

### Step 3: Spec Tests citations + `cargo xtask docs specs`

1. Format: in each spec's INV table add a fourth column `Tests`, each cell either one or more backticked test paths in the greppable form `crate::module::tests::fn_name` (e.g. ``jackin_runtime::runtime::launch::tests::trust_confirmation_runs_before_build``) or the literal `MISSING`. To fill it: for each INV row, search the crate's `tests.rs` for tests exercising the cited production symbol (`rg -l 'confirm_trust_for_test' crates/jackin-runtime/src/runtime/launch/tests.rs` then read the matching test fns). Cite the strongest 1-2 matches; where genuinely none exists, write `MISSING` — do not write new tests in this plan.
2. Gate: add a `Specs` variant — for every `*.mdx` under the specs dir, parse INV-table rows; fail if a row lacks a `Tests` cell; for each cited `crate::path::tests::fn` verify the test function exists (map `crate` to `crates/<crate-with-dashes>/src/<path>/tests.rs` and grep for `fn <fn_name>`); count and report `MISSING` cells as a warning line (not a failure), so coverage debt is visible while broken citations fail.

**Verify**: `cargo run -p jackin-xtask -- docs specs` → `spec gate OK — 19 INV rows, N cited tests verified, M MISSING`; corrupt one citation locally (edit a fn name), rerun → fails naming the spec, row, and missing fn; revert the probe.

### Step 4: README presence in the agents gate

In `agent_files.rs` `check()`, alongside the `AGENTS.md` assertion, add: `README.md` must exist as a file in every `crates/*/` member, failure message "missing README.md (crates/AGENTS.md hard rule: every crate carries README.md + AGENTS.md + CLAUDE.md)". Extend its tests with the missing-README case. (The *freshness* half — "src/ changed ⇒ README touched" — needs a diff-aware CI check; record it as still-open in the roadmap note rather than half-building it here.)

**Verify**: `cargo nextest run -p jackin-xtask` → all pass; `cargo run -p jackin-xtask -- lint agents` → OK (all 26 crates have READMEs today).

### Step 5: Wire into CI + docs

- Find where `cargo xtask docs repo-links` / `roadmap audit` run in `.github/workflows/` (search `docs.yml` first) and add `cargo xtask docs brand` + `cargo xtask docs specs` beside them.
- `crates/jackin-xtask/README.md`: note the two new docs subcommands.
- Roadmap Phase 5: mark items 9 (brand gate — shipped) and 10 (spec linkage — shipped, with the citation-format decision recorded: `crate::module::tests::fn` + `MISSING` sentinel) and item 8 (README gate — presence shipped; freshness-vs-diff still open); do not hard-wrap.

**Verify**: `actionlint .github/workflows/docs.yml` (or ci.yml if wired there) → exit 0; `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- docs.rs tests (place beside existing docs tests): brand scanner — fenced-block stripping, inline-code stripping, URL skipping, a real violation detected, allowlist honored; specs gate — row without Tests cell fails, bad citation fails, MISSING warns.
- agent_files tests: missing-README failure case.
- Full: `cargo nextest run -p jackin-xtask` green.

## Done criteria

- [ ] `cargo run -p jackin-xtask -- docs brand` → OK; `rg "jackin'" APPLE_CONTAINER_RESEARCH.md` → 0 matches
- [ ] All 19 INV rows across the 3 specs have a Tests cell; `docs specs` gate verifies citations and passes
- [ ] `lint agents` asserts README presence
- [ ] Both new gates run in CI beside the existing docs gates
- [ ] Roadmap Phase 5 statuses updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Brand-scanner stripping still leaves >5 false positives after implementing fence/inline/URL handling (the approach needs rethinking, not a growing allowlist).
- More than 8 of the 19 INV rows end up `MISSING` (spec-coverage debt bigger than expected — the operator should decide whether to block on writing tests first).
- A rewritten APPLE_CONTAINER_RESEARCH.md sentence would change technical meaning (e.g. a quoted upstream name that genuinely contains an apostrophe) — leave it, allowlist it, and note it.
- The specs' INV-table format differs from the excerpt (someone restructured the specs).

## Maintenance notes

- Plan 013 adds no specs but the two roadmap-required new specs (capsule daemon, operator console) are recorded next-wave; when written, they must use the Tests-column format this plan establishes.
- The `MISSING` count in `docs specs` output is a natural future ratchet entry (plan 017).
- Reviewer should scrutinize: the APPLE research rewrites (meaning preservation) and the brand scanner's MDX component handling (JSX attributes are prose-ish; make sure `<RepoFile path="…jackin…">` style code-bearing attributes don't false-positive — they contain bare `jackin`, which is legal anyway).
