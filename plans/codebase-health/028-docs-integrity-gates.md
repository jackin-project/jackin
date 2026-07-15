# Plan 028: Docs integrity gates — codebase-map structural audit, README-freshness CI wiring, config-key drift

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- crates/jackin-xtask/src/docs.rs crates/jackin-xtask/src/readme_freshness.rs .github/workflows/ci.yml docs/content/docs/reference/getting-oriented/codebase-map.mdx`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: docs (integrity gates)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Three Documentation-integrity items with concrete broken/missing wiring. Item 1: the codebase-map check only verifies each workspace member's name appears somewhere in the page — it cannot detect a stale entry naming a REMOVED crate, nor verify tier annotations or README links per entry; the dependency-visualization decision (structural table as sole interface vs generated graph) is implied but recorded nowhere. Item 2: the README-freshness step runs as the FIRST step of the clippy job — BEFORE `actions/checkout` and toolchain install — with `continue-on-error: true`, so it cannot meaningfully execute and its failure is masked; there is no observed-run proof the gate ever fires. Item 4: the config-key half of documented-surface drift is missing entirely — documented commands are parsed against the real clap tree (`docs_commands.rs`), but a documented config key that drifts from the schema passes CI.

## Current state

- Map check: `crates/jackin-xtask/src/docs.rs:166-197` `check_codebase_map` — member-name presence only. Docs-site side: `docs/scripts/gen-crate-pages.ts:268` `metaCompletenessError` (README↔meta.json parity, keyed on README presence not membership). Map page: `docs/content/docs/reference/getting-oriented/codebase-map.mdx:35-91` (tier table; ":this map stays the tier overview only"). Tier authority: `crates/jackin-xtask/src/arch.rs` TIERS.
- Freshness mis-wiring: `.github/workflows/ci.yml:476-483` — the `README freshness (advisory)` step precedes `actions/checkout` (`:485`) and `jdx/mise-action` (`:495`), `continue-on-error: true`. Gate logic itself is sound (`crates/jackin-xtask/src/readme_freshness.rs:170-183` merge-base handling; unit tests exist at `readme_freshness/tests.rs`).
- Command-half exemplar to mirror: `crates/jackin/tests/docs_commands.rs:15-16` — "every fenced `jackin …` invocation in the docs tree must parse against the real clap command tree."
- Config schema source: `crates/jackin-config/src` (serde structs; `deny_unknown_fields` sites at `auth.rs:21,89`, `schema.rs:94`); config docs live under `docs/content/docs/reference/` (internals — find the config-reference page(s): `grep -rln "config.toml" docs/content/docs/reference | head`).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| xtask tests | `cargo nextest run -p jackin-xtask` | pass |
| Docs gates | `cargo xtask docs repo-links && cargo xtask roadmap audit` | exit 0 |
| Config tests | `cargo nextest run -p jackin-config -p jackin` | pass |
| Workflow lint | `actionlint .github/workflows/ci.yml` | clean |
| Full | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope**: `docs.rs` `check_codebase_map` extension + tests; a one-paragraph recorded dependency-viz decision on the codebase-map page; `ci.yml` freshness job extraction (own job: checkout with `fetch-depth: 0`, mise install, run gate, publish result separately) + a synthetic-diff self-test proving the failure path; new config-key drift gate (`crates/jackin-config/tests/docs_config_keys.rs` or an xtask `docs config` lane — pick the docs_commands.rs-mirroring form) + fixing any drift it exposes.

**Out of scope**: brand gate (plan 029); content validation for README freshness ("add content validation only when…" — deferred by roadmap); generated dependency-graph tooling (the decision step likely REJECTS it — recording the decision is the deliverable).

## Git workflow

Branch `feat/docs-integrity-gates`; Conventional Commits; `git commit -s`; push per commit.

## Steps

### Step 1: Structural codebase-map audit

Extend `check_codebase_map`: (a) two-way diff — every workspace member has an entry AND every crate-shaped token in the map matches a member (fail on non-members, e.g. a deleted crate); (b) per crate entry, require a tier annotation consistent with `arch.rs` TIERS and a link to the crate page/README (`/reference/crates/<name>/` route). Parse the map's table structure (it's a stable MDX table — anchor on its known shape; fixtures in tests).

**Verify**: `cargo nextest run -p jackin-xtask -E 'test(/docs/)'` → new fixtures pass (non-member entry fails; missing tier fails; missing link fails); real tree passes after fixing any exposed drift.

### Step 2: Record the dependency-viz decision

Add the decision paragraph to the codebase-map page: the structural tier table + `cargo xtask lint arch` gate are the sole authoritative dependency interface; no generated graph is embedded (or the opposite if the operator has said otherwise — the default is the status quo made explicit).

**Verify**: `cargo xtask docs repo-links` → exit 0.

### Step 3: Fix README-freshness wiring

Extract into its own job: checkout (`fetch-depth: 0`), mise-action, `git fetch origin main`, run `cargo run -p jackin-xtask … lint readme-freshness --base origin/main`, result published as its own check (not buried in clippy). Decide severity: keep advisory via `continue-on-error` ONLY at the job level with the result still visible, per the roadmap's "measure false positives, decide CI severity" — advisory + visible is the correct first state; note it. Add the observed-run proof: a workflow-level self-test step (or a documented dispatch run in the PR) that constructs a synthetic rename diff (script: copy a tracked `src/*.rs` to a new name in a throwaway branch/worktree inside the runner, run the gate against that diff) asserting non-zero exit — i.e. the gate demonstrably fires end to end.

**Verify**: `actionlint` clean; dispatch/PR run shows the job executing AFTER checkout with real output; self-test step proves the failure path.

### Step 4: Config-key drift gate

Mirror `docs_commands.rs`: enumerate schema-described keys (serde field names from the config structs — either via a small `#[cfg(test)]` reflection using `schemars`-style derive if already present, or a curated `EXPECTED_KEYS` const generated from the structs' serde attrs with a unit test asserting the const matches serde output via serialization of a fully-populated fixture); scan the config-reference docs for documented key spellings; fail on both drift directions (documented-but-gone, schema-added-but-undocumented — the latter may start advisory if noisy; record the choice).

**Verify**: `cargo nextest run -p jackin-config` (or `-p jackin` if placed there) → gate passes on the real tree after fixing exposed drift; a fixture proves both failure directions.

## Test plan

Fixture-driven tests per gate (steps 1, 3-self-test, 4); real-tree green after drift fixes. Model on `docs_commands.rs` and existing `docs.rs` tests.

## Done criteria

- [x] Map audit: non-member entries, missing tier, missing README/crate-page link all fail with file-level messages; real tree green
- [x] Dependency-viz decision recorded on the map page
- [x] Freshness gate runs post-checkout in its own visible job; synthetic-diff proof exists; severity decision recorded
- [ ] Config-key drift gate live both directions (severity per direction recorded); real tree green
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- The map page's table shape resists parsing (free-form prose rows) — restructure of the page is a docs-owner call; report.
- Config-key enumeration can't be made reliable from serde structs (heavy `flatten`/custom deserializers) — report the specific structs; a schema-derive decision is the operator's.
- The freshness gate, once actually running, red-flags many crates (real accumulated staleness) — that's expected signal; keep advisory, list them in the PR, don't fix READMEs here.

## Maintenance notes

- Docs-site `metaCompletenessError` and the xtask map audit now overlap benignly (different failure surfaces); keep both, they check different artifacts.
- The severity decisions (freshness, undocumented-key direction) are recorded advisory-first; revisit after false-positive data accumulates.

**Index deviation (audit 2026-07-15)**: demoted from DONE to IN PROGRESS — Done criteria not fully met; see implementer audit rollup.
