# Goal prompt: finish code-health + launch-speed residuals

Copy everything below the line into `/goal` (or pass this file path).

**Out of scope for this goal:** all `plans/agent-status/` work (live goldens, pack rewrite, Codex app-server reader, remote packs). Do not touch agent-status packs/fixtures unless a code-health gate forces a mechanical fix.

---

## Goal statement

**Drain every remaining code-health residual and launch-speed residual on PR #759 branch `chore/rust-code-health-roadmap`.** Implement real code (prefer implement over pin). Commit with DCO (`-s`), push after every commit. Stay on this branch only.

### Branch lock

```sh
git branch --show-current   # must be chore/rust-code-health-roadmap
gh pr list --head chore/rust-code-health-roadmap
# PR #759 — all work on this branch
```

Never commit on `main`. No force-push without operator approval. Brand: `jackin❯` in prose; identifiers `jackin`.

### Authoritative sources

| Surface | SoT |
|---------|-----|
| Code-health unfinished multi-PR | `plans/code-health/RESIDUAL_LEDGER.md` |
| Launch-speed unfinished | `plans/launch-speed/README.md` (008c) |
| Roadmap context | `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` |
| Gates | `cargo run -p jackin-xtask -- lint --strict`; package tests for touched crates; `ci --fast` red only for documented env waivers |

### Definition of done (this goal)

1. Every row in `plans/code-health/RESIDUAL_LEDGER.md` is either **CLOSED** (in-tree) or still open only with a **hard external blocker** documented in the ledger (prefer CLOSED).
2. Launch-speed **008c** residual fully closed (see Wave 0) or proven impossible with operator pin.
3. `plans/code-health/RESIDUAL_LEDGER.md` + `plans/launch-speed/README.md` + this goal file match source.
4. Roadmap page freshness updated when phases advance.
5. `lint --strict` green; package tests green for touched crates.

---

## Inventory of remaining work

### Wave 0 — Launch-speed 008c (small, do first)

**Status today:** core reuse shipped; residual still open.

**Files:**

- `crates/jackin-runtime/src/runtime/launch/restore_resolve.rs`
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`
- `crates/jackin-runtime/src/runtime/launch/tests.rs`

**Already shipped (do not re-do):**

- Typed `EarlyCurrentRestoreScan` (`NotRun` / `Scanned { agent, current }`)
- Early scan before role repo / auth / image
- `resolve_restore_candidate_reusing_early` reuses selected empty scan
- Related-role restore still runs when current is none
- Predicate test `early_scan_skips_current_inspect_only_for_matching_empty_scan`

**Required to close 008c:**

| # | Work |
|---|------|
| 1 | When **unselected** early scan finds **no** candidate, still record `Scanned` with an unselected scope (or equivalent) so a later matching agent can skip a second current-role inspect when safe |
| 2 | When early scan returns a **non-empty** hit, reuse typed `Scanned.current` where possible instead of only short-circuiting via `early_restore_container` + re-inspect later |
| 3 | Integration regression: FakeDocker (or call-counting double) proves the **common path** does not double-`inspect` current-role candidates; multi-agent / related-candidate paths keep behavior |
| 4 | Preserve rejection diagnostics + timing events |
| 5 | Update `plans/launch-speed/README.md` → DONE or delete folder when criteria met |

**Verify:**

```sh
cargo test -p jackin-runtime --lib early_scan
cargo test -p jackin-runtime --lib restore
# plus any new inspect-count test name
```

---

### Wave 1 — Lint / docs strictness (parallel-friendly mechanical)

#### L1. R-047 maintainability promote

**Problem:** Seven maintainability lints still workspace `allow` after census:

- `needless_pass_by_value` (measured ~28; may stay documented-allow if intentional)
- `large_futures`, `assigning_clones`, `match_same_arms`, `drop_non_drop`, `unused_self`, `unused_async`

**Required:**

1. Re-measure hit counts (`clippy` JSON or workspace dry-run, one lint at a time or batch).
2. For each lint with low residual (target ≤15 or operator-chosen budget): fix or narrow `#[expect(..., reason = "…")]`, then promote to `warn` (CI uses `-D warnings`) or `deny` matching table style.
3. For each lint that stays allow: measured-count comment on the Cargo.toml line (`# allow: N hits measured YYYY-MM-DD, pattern …`).
4. Update residual ledger row **R-047-maintainability-promote** → CLOSED when every lint is promoted **or** honestly measured-allow.

**Files:** root `Cargo.toml` `[workspace.lints.clippy]`; hit sites across crates.

#### L2. R-allow-attributes-deny

**Problem:** Bare `#[allow]` still nonzero; ratchet `bare-allow-per-crate` caps debt; cannot flip `allow_attributes` / `allow_attributes_without_reason` to deny until floor ≈ 0.

**Required:**

1. Burn down bare allows → `#[expect(..., reason = "…")]` or fix root cause (crate-by-crate, largest first: console, runtime, capsule).
2. Shrink `ratchet.toml` bare-allow floors as debt drops.
3. When floor is 0 (or only justified exceptions), enable deny-level `allow_attributes` / `allow_attributes_without_reason` (or project-equivalent gate already used).
4. Ledger row CLOSED when deny is on or remaining floor is documented with single-digit intentional expects only.

#### L3. R-missing-docs-cascade

**Problem:** Only `jackin-protocol` has `#![deny(missing_docs)]`. Pattern from plan 021 should cascade pure crates.

**Required (one crate per commit preferred):**

1. Next pure crates in order (historical cascade): `jackin-manifest` → `jackin-env` → `jackin-term` → `jackin-config` → `jackin-core` (skip if already partial).
2. Each crate: `#![deny(missing_docs)]` + docs for public items; package tests / `cargo doc -p <crate> --no-deps` clean.
3. Ledger CLOSED when cascade list in ledger is empty or next crate is explicitly blocked with reason.

**Verify:**

```sh
cargo run -p jackin-xtask -- lint --strict
cargo clippy --workspace --all-targets --locked -- -D warnings
```

---

### Wave 2 — WorkspaceLabel (R-038)

**Problem:** Typed `WorkspaceName` frontier advanced (058–064 shipped), but dual semantics remain:

- `materialize_workspace(..., workspace_name: &str, ...)` in `crates/jackin-isolation/src/materialize.rs` uses path labels vs config stems
- TUI/CLI display still stringly in places

**Required:**

1. Design split: `WorkspaceName` (config stem / identity) vs `WorkspaceLabel` (path/display label) — or prove one type is enough with explicit conversion at the dual-semantics boundary.
2. Type `materialize_workspace` and callers; no silent string confusion.
3. Push typed names through remaining TUI/CLI display sites listed in residual notes (inventory with `rg 'WorkspaceName|&str'` on host/console/isolation).
4. Tests for path-label vs config-stem cases that previously relied on dual semantics.
5. Ledger **R-038-WorkspaceLabel** → CLOSED.

**Files (start):**

- `crates/jackin-core/src/workspace_name.rs` (extend or sibling type)
- `crates/jackin-isolation/src/materialize.rs`
- Call graph from materialize + console editor/host CLI

---

### Wave 3 — LaunchCore extract cluster (largest runtime block)

These three residuals share one extract; do **not** land suite A or full pipeline bench without seams.

| Residual | Depends on extract |
|----------|-------------------|
| **R-launch-typestate** / **R-typestate-general** | Phase contracts: e.g. `ValidatedProfile → … → RunningContainer` |
| **R-033-suite-a** | Cheap fixture for `run_launch_core` failure-path teardown (grant-failure ordering + mid-pipeline FailedSetup) |
| **R-014-launch-pipeline-bench** | Criterion (or harness) over FakeDockerClient micro-pipeline or post-extract LaunchCore |

**Current state:**

- `launch_core.rs` ≈ **1350 LOC** monolithic `run_launch_core`
- Suites B+C characterization exist; suite A only grant-helper floor in `launch_pipeline/tests.rs`
- Bench `launch_attach` is naming/path micro-ops only — not full pipeline

**Required sequence:**

1. **Characterization first** where possible without full extract (grant-failure helpers already exist — extend toward suite A).
2. **Extract launch phase modules / typestate** so tests inject FakeDocker at phase boundaries without ~20-crate graph.
3. Land **suite A** tests: grant-failure ordering + mid-pipeline FailedSetup teardown reaches cleanup.
4. Land **launch-pipeline bench** compile-check + measured lane (or criterion) over FakeDocker path.
5. Close ledger rows R-launch-typestate, R-typestate-general, R-033-suite-a, R-014-launch-pipeline-bench.

**Files:**

- `crates/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core.rs`
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs`
- `crates/jackin-runtime/src/runtime/launch/launch_pipeline/tests.rs`
- `crates/jackin-runtime/benches/launch_attach.rs` (+ new bench if needed)
- Specs: capsule/runtime behavioral specs if phase names change

**Verify:**

```sh
cargo test -p jackin-runtime --lib launch
cargo build -p jackin-runtime --benches --locked
```

---

### Wave 4 — Daemon decomp cluster

| Residual | Work |
|----------|------|
| **R-daemon-decomp** | Decompose capsule daemon per specs MISSING worklists (module/port seams: control, attach, status, persistence) |
| **R-daemon-char-remainder** | Characterization for session-lifecycle, status-publication, persistence/reattach, cleanup-outcomes beyond B+C |
| **R-sim-turmoil** | After ports: turmoil or proptest-state-machine sim lane for daemon/protocol |

**Current state:** Specs shipped (`capsule-daemon` INV rows); daemon control loop still largely monolithic; no turmoil harness.

**Required:**

1. Read `docs/content/docs/reference/developer-reference/specs/` capsule-daemon + any MISSING worklists.
2. Extract ports/modules with characterization tests each slice.
3. Land remaining char surfaces.
4. Add sim lane (turmoil preferred if crate fits; else documented proptest SM) with CI advisory or required step.
5. Close three ledger rows.

**Files:** `crates/jackin-capsule/src/daemon.rs` and submodules; tests under `daemon/tests.rs`.

**Capsule-touching:** smoke when Docker available; always run package tests.

---

### Wave 5 — Console edit-model convergence (R-edit-model-convergence)

**Problem:** Plan 030 view-models (`FieldRow` / `FormSection`) shipped; full settings/editor merge is still redesign-scale. Residue includes `state.rs` + auth handler complexity.

**Required:**

1. Inventory editor vs settings dual models (forms, save paths, validation).
2. Converge on one edit-model / shared save pipeline where product allows.
3. Remove remaining `type_complexity` / residue allows in console state/auth if fixed by merge.
4. Tests for save/launch paths that previously forked.
5. Ledger CLOSED or, if product forbids full merge this branch, document **operator pin** with measured remaining split — prefer implement.

**Files:** `crates/jackin-console/src/tui/` (editor, settings, state, form_model).

---

### Wave 6 — Perf platform (R-perf-platform)

**Problem:** Ratchet engine exists (`ratchet.toml`); no `[[perf]]` family; dhat budgets stay in-source literals; no iai-callgrind CI.

**Required:**

1. After Wave 3 benches stabilize, add numeric `[[perf]]` (or family) entries for stable hot paths.
2. Move dhat allocation literals into ratchet family + provider.
3. iai-callgrind for at least one hot path **or** document CLOSED-as-pinned if valgrind cannot run in project CI (prefer adopt with CI image support).
4. Optional: numeric build-time budget if Phase 6 baselines already mature (`hygiene` build-time-measure).

**Files:** `ratchet.toml`; `crates/jackin-xtask/src/ratchet.rs`; bench/dhat sites; `.github/workflows` if iai image needed.

---

## Recommended execution order

```text
Wave 0  launch-speed 008c          (small, unblocks honest “launch-speed closed”)
Wave 1  L1+L2+L3 lint/docs         (parallel crates; serialize root Cargo.toml)
Wave 2  WorkspaceLabel R-038       (types; medium)
Wave 3  LaunchCore cluster         (largest runtime; sequential slices)
Wave 4  Daemon cluster             (after or parallel to Wave 3 if file-disjoint)
Wave 5  Console edit-model         (console crate; parallel with 3/4 if careful)
Wave 6  Perf platform              (after Wave 3 benches)
```

Serialize: root `Cargo.toml` lint table, `ratchet.toml`, `ci.yml` / `ci.rs`.

Prefer many small commits with tests over one mega-commit. Capsule/runtime smoke when Docker available.

---

## Explicitly out of scope (do not expand)

| Item | Why |
|------|-----|
| **All agent-status plans** | Operator deferred — not ready / separate goal |
| Usage workspace-scoped CLI reintro | Intentional product surface (accounts/verify) |
| apple-container backend | Not shipping this program |
| Hello short-payload soft-default | Fail-closed by design |
| Optional zero-copy scrollback row | Perf-incident only |
| Optional db/docker metrics demotion | Optional volume work |

If you discover a residual that is actually already shipped, delete the ledger row and do not re-implement.

---

## Hard project rules

- Branch: `chore/rust-code-health-roadmap` only (PR #759)
- `git commit -s` + `git push` after every commit
- Brand `jackin❯` in prose; code identifiers `jackin`
- No silent host writes; container paths under `/jackin/` only
- Pre-release: breaking OK without migration shims (except versioned config schemas)
- Update residual ledger + roadmap when work ships
- Comments: non-obvious WHY only

---

## End-of-goal verification

```sh
git branch --show-current   # chore/rust-code-health-roadmap

# Launch-speed closed
rg -n 'Still open|residual remains' plans/launch-speed/   # expect empty or DONE only

# Code-health ledger drained or honest pins only
cat plans/code-health/RESIDUAL_LEDGER.md

# Gates
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/jackin-target-ch}"
cargo run -p jackin-xtask -- lint --strict
cargo test -p jackin-runtime --lib early_scan
cargo test -p jackin-runtime --lib launch
# plus package tests for any other touched crate

# No agent-status scope creep
git diff origin/main...HEAD --stat -- crates/jackin-agent-status/packs crates/jackin-agent-status/src/screen/fixtures
# expect empty unless forced by unrelated compile fix
```

---

## Success statement

> On `chore/rust-code-health-roadmap` (PR #759): launch-speed 008c residual closed; code-health residual ledger drained (LaunchCore typestate + suite A + pipeline bench, daemon decomp + char + sim, WorkspaceLabel, edit-model convergence, maintainability/bare-allow/missing_docs promotes, perf platform); gates green except documented env waivers; agent-status intentionally untouched.

When that is true, mark goal complete and stop.
