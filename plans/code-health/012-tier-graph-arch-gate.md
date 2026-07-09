# Plan 012: Phase 2 — evolve `cargo xtask lint arch` from an empty edge list to a tier-graph gate

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- crates/jackin-xtask/src/arch.rs crates/*/Cargo.toml`
> If arch.rs or any crate manifest changed since this plan was written,
> re-derive the dependency graph (Step 1) before trusting the tier table below;
> on a structural mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M-L
- **Risk**: MED (a wrong tier order inverts the whole check; mitigated by deriving tiers from the measured graph and failing closed)
- **Depends on**: none (plan 016 depends on THIS plan's tier table)
- **Category**: tech-debt
- **Planned at**: commit `47dd5fca0`, 2026-07-09

## Why this matters

The dependency-direction gate exists and runs strict in CI, but checks nothing: `FORBIDDEN_EDGES` is empty, so `cargo xtask lint --strict` advertises architecture enforcement it is not performing (first-wave finding DX-arch-gate-noop). The roadmap (Phase 2, "Evolve the layered-architecture gate from an allowlist to a tier graph", line 144) wants every crate to get a rule automatically: declare each crate's tier, fail on any edge that points wrong, separate production from dev-dependency rules, and check for cycles. The audit found the blocking design fact: **the L-numbers currently written in `lib.rs` headers are not a topological order** — under them, 12 legitimate production edges point "upward" (application L1 crates depend on infrastructure L2 crates; two L2 crates depend on the L3-labeled `jackin-tui`), and the vocabulary is split across four inconsistent surfaces (headers say `L0 domain/L1 application/L2 infrastructure/L3 presentation/L4 entry-glue`; the `crates/AGENTS.md` template says `L0 leaf / L1 domain / L2 infrastructure / presentation / binary`; two crates declare no tier at all; `jackin-diagnostics/Cargo.toml:51` even calls the crate "(L1…)" while its own header says L2). This plan makes the gate's tier table the single machine source of truth, derived from the real graph, and turns the gate into a real check.

## Current state

- `crates/jackin-xtask/src/arch.rs` — the whole gate. Key excerpts (verified at `47dd5fca0`):

  ```rust
  const FORBIDDEN_EDGES: &[(&str, &str)] = &[];            // line 42
  ```

  The dep map is built from `cargo metadata` keeping **only non-dev edges** (line 113: `d.kind.as_deref() != Some("dev")` — note this also keeps `build` deps). Violations print `"{from} → {to}: forbidden (see codebases-health-enforcement W4)"` (line 135 — the doc slug is misspelled, `codebases-` should be `codebase-`). `--dump` prints the adjacency; `--strict` fails on violations; the umbrella `lint --strict` forwards strict (main.rs:168-175). Minimal serde structs `Metadata`/`Package`/`Dep` (lines 169-188) parse `cargo metadata`; `Dep` has `name` + `kind` but **not** the `target`/`optional` fields (fine for this plan).
- Measured production adjacency (from the audit's `cargo metadata` pass; re-derive in Step 1 — do not trust blindly):

  ```
  jackin              → config console core diagnostics docker env image launch-tui manifest protocol runtime tui
  jackin-agent-status → core protocol
  jackin-capsule      → agent-status core diagnostics protocol term tui usage
  jackin-config       → core
  jackin-console      → config console-oppicker core diagnostics env protocol tui
  jackin-console-oppicker → core diagnostics tui
  jackin-diagnostics  → core tui
  jackin-docker       → core diagnostics
  jackin-env          → config core diagnostics protocol
  jackin-host         → core diagnostics docker protocol tui
  jackin-image        → core diagnostics docker manifest
  jackin-instance     → config core diagnostics manifest
  jackin-isolation    → config core diagnostics docker protocol
  jackin-launch-tui   → core diagnostics tui
  jackin-manifest     → config core
  jackin-protocol     → core
  jackin-runtime      → config core diagnostics docker env host image instance isolation launch-tui manifest protocol
  jackin-tui          → core
  jackin-usage        → core diagnostics protocol
  ```

  Dev edges: `jackin → {config, env, runtime}` (downward), `jackin-isolation → jackin-runtime` (**upward — forms the one known prod+dev cycle with prod `runtime → isolation`**; first-wave DEBT-devdep-cycle), `jackin-runtime → jackin-tui` (upward, bench/test-only, allowed under the dev rule), `jackin-term → {core, diagnostics}` (upward for a leaf, dev-only, allowed). Build deps: six crates → `jackin-build-meta`.
- Longest-path depth derived from that adjacency (this is the tier table Step 2 encodes; Step 1 re-verifies it):

  | Depth | Crates |
  |---|---|
  | 0 | jackin-core, jackin-term, jackin-build-meta, jackin-pr-trailers, jackin-dev, jackin-xtask, jackin-tui-lookbook* |
  | 1 | jackin-config, jackin-protocol, jackin-tui |
  | 2 | jackin-manifest, jackin-diagnostics, jackin-agent-status |
  | 3 | jackin-docker, jackin-console-oppicker, jackin-launch-tui, jackin-usage, jackin-instance, jackin-env |
  | 4 | jackin-host, jackin-image, jackin-isolation, jackin-capsule, jackin-console |
  | 5 | jackin-runtime |
  | 6 | jackin (binary) |

  *`jackin-tui-lookbook`, `jackin-dev`, `jackin-xtask`, `jackin-pr-trailers` were not captured in the audit's adjacency (they may have internal deps the audit missed — e.g. lookbook almost certainly depends on `jackin-tui`). Step 1 resolves their true depths; the table above is the cross-check, not gospel, for those four.
- Tier vocabulary drift (evidence for the header normalization note in Step 5): `crates/jackin-core/src/lib.rs:7` "**Architecture Invariant:** L0 domain crate"; `crates/jackin-launch-tui/src/lib.rs:3` "a **presentation** crate" (no number); `crates/jackin-build-meta/src/lib.rs:1-5` no tier at all; `crates/jackin-diagnostics/Cargo.toml:50-52` comment "this crate (L1, depended on by 8 L0/L1/L2 crates) does not need to pull `jackin-tui` (L3)".
- Repo conventions: xtask module layout + sibling `tests.rs` (see plan 010's Current state); the gate's failure text must state the fix and rerun command (roadmap "Diagnostics are prompts", exemplar: `test_layout.rs:277-281`).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Ground-truth adjacency | `cargo xtask lint arch --dump` | prints `crate → deps` lines |
| Run the gate | `cargo run -p jackin-xtask -- lint arch --strict` | `arch gate OK …` |
| Xtask tests | `cargo nextest run -p jackin-xtask` | all pass |
| Clippy | `cargo clippy -p jackin-xtask --all-targets -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |
| Docs gates | `cargo xtask roadmap audit && cargo xtask docs repo-links` | pass |

## Scope

**In scope**:
- `crates/jackin-xtask/src/arch.rs` and `crates/jackin-xtask/src/arch/tests.rs`
- `crates/jackin-xtask/README.md` (row already exists — update the description)
- `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` (Phase 2 gate item status)

**Out of scope**:
- Changing any crate's actual dependencies (breaking the isolation⇄runtime dev cycle is first-wave DEBT-devdep-cycle, a separate code move).
- Rewriting the 26 `lib.rs` Architecture Invariant headers (plan 016 backfills headers and cross-checks them against THIS plan's table).
- `main.rs` wiring changes beyond doc-comment updates (the `Arch` subcommand and `--strict` forwarding already exist).
- Reconciling the `crates/AGENTS.md` tier-vocabulary template (note it in the roadmap edit; the template fix rides with plan 016).

## Git workflow

- Branch off `main`: `feat/arch-tier-graph-gate`.
- Conventional Commits, `-s`, push after every commit. PR to `main`; do not merge.

## Steps

### Step 1: Capture ground truth and reconcile the tier table

Run `cargo xtask lint arch --dump` and save the output. Compare against the adjacency block above. For the four uncaptured crates (`jackin-tui-lookbook`, `jackin-dev`, `jackin-xtask`, `jackin-pr-trailers`), read their `Cargo.toml` `[dependencies]` and place them at (1 + max depth of their internal deps), or depth 0 if none. If any OTHER crate's edge set differs from the block above, STOP and report the diff (the graph moved since planning).

**Verify**: a written-down tier table covering all 26 crates where every production edge goes from a higher-depth crate to a strictly lower-depth crate. Mechanical check: for each `from → to` line in the dump, `depth(from) > depth(to)`.

### Step 2: Encode the tier model in arch.rs

Replace `FORBIDDEN_EDGES` with:

```rust
/// Architecture tiers. Lower = more foundational. A production dependency
/// must point at a strictly lower tier; dev-dependencies may point anywhere
/// except into a production+dev cycle. Derived from the measured dependency
/// graph (2026-07-09); `lint arch --dump` prints the live graph.
const TIERS: &[(&str, u8)] = &[
    ("jackin-core", 0), ("jackin-term", 0), ("jackin-build-meta", 0), /* … all 26, from Step 1 … */
];
```

Rules implemented in `check`/`run`:
1. **Completeness**: every workspace member must appear in `TIERS`; a missing crate fails with `"{name}: no tier declared — add it to TIERS in crates/jackin-xtask/src/arch.rs (pick 1 + max tier of its internal deps)"`. This is how "a new crate gets a rule automatically".
2. **Production rule**: for every non-dev internal edge `from → to`: `tier(to) < tier(from)`, else fail with the offending edge, both tiers, and the fix: `"jackin-foo (T2) → jackin-bar (T3): production dependencies must point at a strictly lower tier; either re-tier jackin-foo above T3 in TIERS (and justify in the commit) or remove the dependency"`.
3. **Dev rule**: parse dev edges too (drop the `kind != "dev"` filter into a partition instead). Dev edges are allowed upward, but a dev edge that closes a cycle with production edges fails, with one grandfathered exception table: `const DEV_CYCLE_ALLOWLIST: &[(&str, &str)] = &[("jackin-isolation", "jackin-runtime")];` — each entry carries a comment naming the tracking item (DEBT-devdep-cycle) and the gate fails on stale allowlist rows (cycle no longer present ⇒ remove the row), mirroring the shrink-only stale-row semantics of `test_layout.rs:243-250`.
4. **Cycle check**: explicit DFS/Kahn cycle detection over production edges (should be impossible if rule 2 holds — keep it as a distinct error message anyway, it fires first and names the cycle path).
5. Treat `build` deps like production deps **except** edges to `jackin-build-meta`, which is tier 0 anyway — no special case should be needed; if one is, STOP and report.
6. Fix the misspelled slug: `codebases-health-enforcement` → `codebase-health-enforcement` (line 135's message and the module doc, lines 1-25, which still describe the empty-list model — rewrite the doc header to describe the tier model).

Keep `--dump` (now printing `name (T<n>) → deps…`) and `--strict` semantics unchanged.

**Verify**: `cargo run -p jackin-xtask -- lint arch --strict` → `arch gate OK — 26 crates tiered, N production edges checked, 1 grandfathered dev cycle`; `cargo run -p jackin-xtask -- lint arch --dump | head -5` shows tier annotations.

### Step 3: Negative-path tests

Extend `crates/jackin-xtask/src/arch/tests.rs`. The existing tests exercise the metadata parsing; add pure-function tests by refactoring rule evaluation into `fn evaluate(tiers, prod_edges, dev_edges) -> Vec<String>` so tests need no cargo invocation. Cover: missing-tier crate; upward production edge; dev cycle not in allowlist; stale allowlist row; clean graph. Assert each failure message contains its fix instruction (grep for "add it to TIERS", "strictly lower tier", "remove the stale").

**Verify**: `cargo nextest run -p jackin-xtask` → all pass, including ≥5 new tests.

### Step 4: Prove the gate bites

Temporarily add `jackin-runtime = { workspace = true }` to `crates/jackin-config/Cargo.toml` `[dependencies]` (a flagrant T1→T5 inversion), run `cargo run -p jackin-xtask -- lint arch --strict`, confirm it fails naming that edge with both tiers. Revert the probe (`git checkout -- crates/jackin-config/Cargo.toml`). Do NOT commit the probe.

**Verify**: probe run exits non-zero with the edge named; after revert, `--strict` passes and `git status --short` is clean.

### Step 5: Docs and roadmap

- Update the `arch.rs` row description in `crates/jackin-xtask/README.md` (tier-graph, not forbidden-edge list).
- Roadmap page, Phase 2 "Public API surface and dependency-direction gates" item 4: mark the tier-graph evolution shipped; record the **tier renumbering decision** in one paragraph — the gate's tier table is now the machine source of truth and uses graph-derived depths (infrastructure below application, `jackin-tui`/`jackin-diagnostics` as low-tier shared leaves), superseding the L0-L4 numbers currently written in `lib.rs` headers; header normalization to match is plan 016's job; the `crates/AGENTS.md` template vocabulary must be reconciled there too. Also update the "Shipped foundation" bullet (line 22) that says the forbidden-edge list is empty.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- New in `arch/tests.rs`: the five negative-path cases above via the extracted `evaluate` function; one test pinning the real `TIERS` table is complete against the member list baked into the test via the existing metadata fixtures (or a members-list constant).
- Full: `cargo nextest run -p jackin-xtask`; `cargo xtask ci --fast`.

## Done criteria

- [ ] `FORBIDDEN_EDGES` gone; `TIERS` covers all 26 workspace members
- [ ] `cargo run -p jackin-xtask -- lint arch --strict` passes on the real graph and fails on the Step 4 probe
- [ ] Dev-cycle allowlist has exactly one entry (isolation⇄runtime) with stale-row enforcement
- [ ] Failure messages name the edge, both tiers, and the fix; misspelled slug corrected (`rg codebases-health crates/` → no matches)
- [ ] `cargo nextest run -p jackin-xtask` green with new negative-path tests
- [ ] Roadmap Phase 2 item updated with the tier-model decision
- [ ] `cargo xtask ci --fast` → `ci gate OK`; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- Step 1's dump differs from the recorded adjacency for any crate other than the four uncaptured ones.
- You cannot produce a tier assignment where every production edge strictly descends (i.e. the production graph has a cycle) — that is a repo bug to report, not to allowlist silently.
- More than the one known dev cycle exists.
- The `Dep.kind` filter behaves unexpectedly for `build` deps (rule 5) — report rather than special-casing.

## Maintenance notes

- Adding a workspace crate now requires a `TIERS` row — the gate's missing-tier message tells the author what to do; keep that message accurate.
- Plan 016 backfills `lib.rs` ownership headers and cross-checks the header tier against `TIERS`; if you change tier numbering here later, 016's gate is the other side of the contract.
- When DEBT-devdep-cycle is fixed (test fakes moved out of jackin-runtime), the allowlist row goes stale and the gate itself will demand its deletion.
- Reviewer should scrutinize: the tier table (each number justified by the live `--dump`), and that `--strict` stays wired through the umbrella `lint --strict` (CI entry point).
