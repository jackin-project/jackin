# PR #495 Review — `refactor: split crates and rebuild TUI/capsule architecture`

- **PR:** https://github.com/jackin-project/jackin/pull/495
- **Branch:** `feature/tui-architecture` → `main`
- **Size:** 1,119 files, **+199,776 / −129,185**
- **Reviewed:** static analysis of the workspace layout, crate DAG, module trees, and a whole-repo orphaned-source sweep. No full `cargo build`/`clippy`/test pass was run (expensive on this tree); the dead-code findings are derived from authoritative module-system tracing and verified against `origin/main`.

The architectural direction (tiered Cargo workspace + typed Ratatui/component model) is sound. The issues below are concrete and fixable, ordered by impact.

---

## 1. ~32,430 LOC of orphaned, committed dead code (highest priority)

When modules were extracted into new crates, the originals were **copied + shimmed but never deleted**. The parent shim files wire children via inline `pub mod child { pub use jackin_runtime::… }` blocks, so Rust never loads the same-named `child.rs` on disk. There is no `#[path]` or `include!` rescuing them, and no external references. They are tracked in git but **not compiled**.

### Verification
- **Mechanism:** parents such as `crates/jackin/src/runtime.rs` and `crates/jackin/src/isolation.rs` are pure re-export shims using inline `pub mod` blocks; the same-named files on disk are shadowed and dropped from the module tree.
- **Positive control:** the resolver correctly *keeps* `isolation/tests.rs` (declared via `mod tests;`) while flagging `isolation/branch.rs` (shadowed by an inline block) — proving it distinguishes the two cases.
- **All 67 files are git-tracked** (committed, not local junk).
- **Drift already present:** e.g. `crates/jackin/src/runtime/launch.rs` (2,230 LOC) is a near-duplicate of the live `crates/jackin-runtime/src/runtime/launch.rs` (2,286 LOC) — only ~429 whitespace-stripped lines differ. A future fix could easily land in the dead copy.

### Introduced by this PR (not pre-existing)
On `origin/main`, `src/runtime/mod.rs` declared `mod launch; mod cleanup;` and `src/isolation/mod.rs` declared all its children — that code was **live**. The paths `crates/jackin/src/runtime/launch.rs` etc. do not exist on `main`. So 100% of this dead code is a byproduct of an incomplete "copy + shim, forgot to delete" extraction.

### Breakdown (67 files, all in the `jackin` crate)

| Subsystem | Prod LOC | Test LOC | Total |
|---|---:|---:|---:|
| `runtime/` | 8,588 | 10,199 | **18,787** |
| `isolation/` | 2,341 | 3,358 | **5,699** |
| `instance/` (auth, manifest, naming) | 1,781 | 2,739 | **4,520** |
| `derived_image/tests.rs` | 0 | 874 | 874 |
| `manifest/tests.rs` | 0 | 857 | 857 |
| `tui/` (animation.rs, output.rs) | 550 | 11 | 561 |
| `workspace/` tests | 0 | 528 | 528 |
| `version_check/tests.rs` | 0 | 229 | 229 |
| `agent_binary/tests.rs` | 0 | 208 | 208 |
| `binary_artifact/tests.rs` | 0 | 88 | 88 |
| `capsule_binary/tests.rs` | 0 | 70 | 70 |
| `operator_env/` stubs | 9 | 0 | 9 |
| **Total** | | | **32,430** |

The other 16 workspace crates are clean (no orphans).

**Recommendation:** delete all 67 orphaned files (the entire `runtime/` and `isolation/` trees and the stray `*/tests.rs` under `jackin/src/`), keeping only the shim `.rs` files that re-export from the new crates. Then confirm `cargo check -p jackin` / workspace still builds. Highest-value, lowest-risk cleanup in the PR.

---

## 2. Tests didn't travel with the extracted code

Config logic moved to `jackin-config`, but ~5,400 LOC of its tests stayed in the consumer crate (`crates/jackin/src/config/` — `editor/tests.rs`, `config/tests.rs`, `migrations/tests.rs`, etc.) exercising **re-exported shims**, while `jackin-config` itself has only **482 LOC** of its own tests.

Unit tests should live next to the code under test so that:
- `cargo test -p jackin-config` is meaningful in isolation,
- the extracted crate doesn't *look* under-tested,
- the monolith stays thin and compiles faster.

**Recommendation:** move each extracted module's unit tests into its owning crate.

---

## 3. Layering inversion: `jackin-diagnostics` → `jackin-tui`

`jackin-diagnostics` depends on the entire `jackin-tui` crate only for a few small helpers:

```
crates/jackin-diagnostics/src/run.rs:      jackin_tui::ansi_text::strip_bytes
crates/jackin-diagnostics/src/run.rs:      jackin_tui::prune_output::section / start
crates/jackin-diagnostics/src/terminal.rs: jackin_tui::shorten_home
```

A diagnostics/telemetry crate depending on a UI crate is backwards and bloats the dependency graph.

**Recommendation:** move `strip_bytes` / `shorten_home` (and the small prune-output helper, if needed lower in the stack) into `jackin-core` or a tiny util crate, and drop the `jackin-tui` dependency from `jackin-diagnostics`.

---

## 4. The crate split is begun, not finished

The `jackin` binary crate is still a **90,910 LOC** monolith (`console/` 37k, `runtime/` shims, `isolation/` 5.7k, `cli/`, `instance/`, `app/`, `config/` shims + tests). This is sanctioned incremental work per `crates/AGENTS.md` ("legacy `src/` files are tolerated only until each module is extracted"), but the PR title/description reads as "done."

**Recommendation:** frame this as Phase 1 of the split and track the remaining extractions (notably the `console` domain layer and `isolation`) as explicit follow-up roadmap items.

---

## 5. God files

Several single files in the 1.5k–2.3k LOC range warrant decomposition:

| File | LOC |
|---|---:|
| `crates/jackin-runtime/src/runtime/launch.rs` | 2,286 |
| `crates/jackin/tests/manager_flow.rs` | 3,715 |
| `crates/jackin/src/console/tui/input/global_mounts.rs` | 1,930 |
| `crates/jackin/src/console/tui/state.rs` | 1,789 |
| `crates/jackin-capsule/src/tui/components/dialog.rs` | 1,776 |
| `crates/jackin-console/src/tui/components/op_picker.rs` | 1,650 |
| `crates/jackin/src/console/tui/input/editor.rs` | 1,571 |
| `crates/jackin-term/src/grid.rs` | 1,530 |

**Recommendation:** split along clear seams (input handling vs. state vs. rendering) where it improves reviewability; not urgent, but these are review hotspots.

---

## 6. Scope / reviewability

This single PR bundles:
- a structural crate split (workspace tiering),
- a TUI/capsule rebuild,
- 3 new crates (`jackin-term`, `jackin-tui`, `jackin-tui-lookbook`),
- behavioral fixes (scroll hints, mouse-wheel handling, debug bar),
- a **`v1alpha6` schema bump** (config + workspace),
- new docs and ADRs.

For a solo maintainer whose pre-merge confidence comes from CI + agent review (per `AGENTS.md`), a 1,119-file change mixing structural and behavioral work is effectively unreviewable as a unit. A mechanical, behavior-preserving "split crates" PR followed by smaller feature/fix PRs would each be auditable.

**Recommendation:** for future work of this magnitude, land a no-behavior-change refactor first, then stack features on top.

---

## Suggested action order

1. **Delete the 32k LOC of orphaned files** (#1) — mechanical, high-value, low-risk; verify with `cargo check`.
2. **Relocate extracted-crate unit tests** into their owning crates (#2).
3. **Break the `diagnostics → tui` dependency** by relocating the shared helpers (#3).
4. Track remaining extraction + god-file decomposition as follow-ups (#4, #5).
5. Adopt a stacked-PR workflow for future large refactors (#6).

---

# Part 2 — Roadmap-vs-reality gap analysis

The PR is the executable surface for the **Post-restructure correctness & polish** program (Phase 2 epic, Defects 36–47). I re-read the roadmap design-of-record items and compared their stated goals to what actually landed. The recurring pattern: **many checklist items are marked `[x]` (done) while their own closing notes say "deferred", "Partial", "requires operator hardware", or "Benchmark numbers require a real session".** Those are the "cases we skipped" — work that is checked off but not actually complete or verified.

Sources: [post-restructure-fixes](/reference/roadmap/post-restructure-fixes/), [post-restructure-fixes-checklist](/reference/roadmap/post-restructure-fixes-checklist/), [terminal-emulation-crate](/reference/roadmap/terminal-emulation-crate/), [agent-runtime-trait](/reference/roadmap/agent-runtime-trait/), [auth-sync-source-folder](/reference/roadmap/auth-sync-source-folder/), [structured-tracing-metrics](/reference/roadmap/structured-tracing-metrics/).

## A. Already resolved — roadmap text is now stale (fix the docs)

These were verified by running the actual gates; the roadmap still describes them as blocked/skipped.

- **`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` PASSES** (verified locally, exit 0, 2m45s, zero warnings). The checklist (Defect 35 line ~600 and Defect 45 acceptance line ~1039) claims this gate is "blocked by pre-existing capsule integration test compile issues." That is no longer true. `cargo check -p jackin-capsule --tests --all-features` also compiles clean (exit 0). **Action:** update the two stale notes to reflect that the full gate is green.

## B. Charter goal partially met — closed-enum dispatch not fully retired

Charter goal #2 ("the `AgentRuntime`/`Provider` registry replaces 414 sites of closed-enum dispatch"). The acceptance gate (checklist line ~1130) claims "~17 arms remaining." Actual count today is **60 `Agent::Variant =>` / `Provider::Variant =>` arms across 6 files**:

| File | Arms | Nature |
|---|---:|---|
| `crates/jackin-config/src/schema.rs` | 20 | named-field accessors (Phase 4 "keep named fields" — *deliberate*) |
| `crates/jackin-config/src/app_config.rs` | 11 | named-field accessors (Phase 4 — *deliberate*) |
| `crates/jackin-core/src/manifest.rs` | 11 | accessor methods |
| `crates/jackin-capsule/src/daemon/multiplexer_utils.rs` | 8 | `token_for_provider` — *flagged exception* |
| `crates/jackin-runtime/src/instance.rs` | 5 | `AgentRuntimeState` construction — roadmap says "Agent A active" |
| `crates/jackin-image/src/agent_binary.rs` | 5 | *documented circular-dep exception* |

The ~17 figure is an undercount. The intent ("a sixth runtime = one adapter file + one row per accessor") is largely satisfied, but `agent_binary.rs` and `multiplexer_utils.rs` are still real closed-match exceptions, and the `instance.rs` construction site is incomplete by the roadmap's own admission. **Action:** decide which arms are genuinely justified (document them as `#[expect]`-style exceptions) and collapse the rest, or correct the gate's "~17" claim to the real number with rationale.

## C. Deferred sub-goals marked `[x]` (done where the criterion was not met)

1. **`jackin-term` performance acceptance was never measured.** Multiple acceptance items (checklist lines ~1020, ~1021, ~1036) state "**Done when:** measured numbers ≥ `vt100` on present-frame p99, bytes-on-wire, allocs/frame, N-pane RSS/CPU" — all marked `[x]`, all closing with "Benchmark numbers require the binary running in a real session" / "Deferred: requires a real running capsule session + dhat profiler." The criterion (numbers captured to a run id) was not satisfied. The PR description lists three `cargo bench` targets, but no captured results back the "performance is the reason to own it" claim. **Action:** wire `criterion`/`dhat` and capture real numbers, or downgrade these to honest `[~]`.

2. **Diagnostics JSONL is not span-sourced** (Defect 47 / structured-tracing-metrics). Verified: `RunDiagnostics::{compact,stage,debug}` still write the JSONL **directly**; `tracing` events are additive. The roadmap status line claims the JSONL is "now span-sourced with a real `span_id`", but its own deferred-enhancement note admits "currently JSONL written directly with tracing as additive." Only `span_id` is real (via one `#[instrument]` on `load_role_with`). The architectural inversion (spans authoritative, `JackinDiagnosticsLayer` emits the JSONL) is the actual Defect 47 goal and is unbuilt. **Action:** either build the layer inversion or correct the contradictory status line.

3. **Observability metrics surface (PR 5) not built.** structured-tracing-metrics PR 5 calls for stage-duration histograms + cache hit/miss counters. 47.5 added a `duration_ms` field to the JSONL detail, but the counter/histogram metrics surface does not exist. Legitimately deferrable, but it is part of the design and currently absent.

4. **`jackin-term` grid memory model (Phase 4/5 tail).** `Cell` uses `CompactString` (good), but the Ghostty `PageList` arena, `RefCountedSet` interning, multi-session slab, and `dirty_spans()` integration into the render emit path are all deferred (checklist lines ~1011, ~1016, ~1017, ~1019). "Zero per-frame allocation" is therefore only partially achieved — the `Vec<Vec<Cell>>` grid and the dirty-spans `Vec` still allocate. Deferral is reasonable (pending benchmarks from C.1), but the zero-alloc acceptance item is checked off prematurely.

5. **Real PTY conformance corpus deferred.** Differential harness runs against inline fixtures only; real `claude`/`codex`/`vim`/`htop`/asciinema captures are "the incremental next step" (checklist lines ~997, ~1012). The fixture directory exists but is largely empty.

## D. Mechanical-refactor debt deferred (behavior correct, DRY/architecture goal unmet)

The shared primitives were *built and exported* but the surfaces were **not migrated onto them** — the scattered ad-hoc state remains. Each is marked `[x]` with "deferred as a follow-up cleanup":

- **`HoverTracker<K>`** built but surfaces still use scattered `hovered_*` bools (lines ~301, ~517, ~595).
- **`FocusOwner<Tab>`** built but surfaces still use scattered focus bools (lines ~520, ~549, ~593).
- **`classify_click` / modal lifecycle** not adopted by the capsule (line ~522).
- **Shared scroll model** — console adopted; launch (`TailScroll`) and capsule (ANSI) not unified (lines ~524, ~754).
- **`SelectList` horizontal scroll** — only `…` truncation; full H-scroll deferred (lines ~212, ~534).
- **`clippy::disallowed_methods` guard** against blocking syscalls on render threads — deferred (line ~956).

This is the DRY charter goal (#3) left half-done: the primitive exists *and* the old duplicated path exists, which is arguably worse than before (two ways to do it). **Action:** finish the migrations so each primitive has exactly one code path, or explicitly accept the debt as a tracked follow-up rather than `[x]`.

## E. Capsule still renders via raw ANSI, not Ratatui primitives

Recurring constraint behind many D-items (lines ~522, ~549, ~599): the capsule chrome emits VT100/ANSI directly and bypasses the `jackin-tui` Ratatui primitives, so `FocusOwner`/`HoverTracker`/`classify_click`/shared-scroll cannot be adopted there. Visual parity is hand-maintained. This is the single biggest "two implementations of the same thing" risk left in the TUI layer and is not tracked as its own roadmap item.

## F. Verification rests on operator hardware that was never exercised

Many `[x]` items close with "requires operator hardware/environment" or "full smoke requires hardware": `cargo nextest run --workspace`, the capsule smoke build + real `--debug` multi-pane session (Defects 40, 44, 45, 46 B.5), symbolication end-to-end (line ~923), provider-picker end-to-end (Phase 0 close-out). None of these were run as part of marking the items done. They may well work, but "done" currently means "code looks right," not "exercised." **Action:** run the operator smoke pass (or label these `[~] code-complete, smoke pending`).

## G. The dead-code finding *is* the unfinished migration (ties to Part 1)

The roadmap (Defect 46 line ~1066) admits the monolith→crates migration was "still mid-flight: two console homes, capsule submodule moves." Part 1's 32,430 LOC of orphaned files in `crates/jackin/src/{runtime,isolation,instance,…}` **is** that unfinished migration residue — the moved-but-not-deleted originals. The "reconcile the half-migration" goal is therefore not actually complete; the shims hide it from the compiler but the dead trees remain. Deleting them (Part 1 recommendation) is what finishes this goal.

## Priority order for fixing in this PR

1. **Delete the 32k LOC orphaned trees** (Part 1 + G) — finishes the migration, removes drift risk. Mechanical, verify with `cargo check`.
2. **Fix the stale gate notes** (A) — the `--all-targets` clippy gate is green; the docs lie. One-line doc edits.
3. **Reconcile the dispatch-arm count** (B) — correct "~17" to 60 with per-file justification, collapse the unjustified exceptions (`agent_binary.rs`, `multiplexer_utils.rs`, `instance.rs` construction).
4. **Correct the contradictory diagnostics status** (C.2) — JSONL is additive, not span-sourced; either build the inversion or fix the claim.
5. **Honestly re-status the unmeasured/deferred `[x]` items** (C, D, F) as `[~]` with a tracked follow-up, OR complete them (benchmarks, primitive migrations, smoke pass).
6. **Open a tracked roadmap item for the capsule ANSI→Ratatui migration** (E).

---

# Part 3 — Did we actually achieve the goal? (verification audit)

Parts 1–2 were static analysis. Part 3 *runs* the project's own acceptance gates to answer the overarching question — **did this PR meet its stated definition of done?** The headline answer: **No — the central "Green everywhere" acceptance gate is objectively false.** The roadmap marks it `[x]` across Defects 44, 45, and 46, but the workspace test suite has a failing test.

## 🔴 Headline: `cargo nextest run --workspace` FAILS (1 test)

I ran the project's own gate. Result: **3287 passed, 1 failed, 0 skipped** (out of 3288).

```
FAIL  jackin-runtime  runtime::progress::tests::launch_container_info_renders_from_footer_chip_state
panicked at crates/jackin-runtime/src/runtime/progress/tests.rs:764:
  container info dialog must contain "jackin version"
```

- **It is deterministic, not environment-dependent.** The test renders into a fixed `TestBackend::new(100, 28)` with hardcoded strings — no home dir, no Docker, no env vars, no terminal-size dependence. It fails every run.
- **Root cause:** the test asserts the rendered "Debug info" dialog contains the literal `"jackin version"`, but the shared component [crates/jackin-tui/src/components/container_info.rs](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/crates/jackin-tui/src/components/container_info.rs#L124) renders the row labeled **`"jackin"`** (`ContainerInfoRow::new("jackin", version)`), with the version as the value. The string `"jackin version"` never appears. So either the test is stale (wrong expectation) or the launch surface should use a `"jackin version"` label and doesn't — either way the suite is red.
- **This proves the workspace test gate was never actually run green.** The roadmap repeatedly marks "**Green everywhere** — `cargo nextest run --workspace`" as `[x]` (Defect 45 acceptance line ~1039, Defect 46 acceptance line ~1134) while admitting `nextest --workspace` "requires operator hardware/environment." A single deterministic lib-test failure that nothing in the environment can explain means the gate was checked off without being run. This is the concrete confirmation of the Part 2 pattern (items marked done that were never verified).
- **Ties to Part 1:** the same failing test also exists in the orphaned dead copy [crates/jackin/src/runtime/progress/tests.rs](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/crates/jackin/src/runtime/progress/tests.rs) — which never runs, so it can't even fail. The live copy in `jackin-runtime` is the one that breaks.

**Action:** fix the test/label mismatch (decide the canonical label — `"jackin"` vs `"jackin version"` — and make the test and all surfaces agree), then re-run `cargo nextest run --workspace` to a clean pass before claiming the gate is green.

## 🟡 Minor: unjustified `unreachable!()` panics (charter goal #1)

Charter goal #1 is "never panic on data," and the best-practices guidance wants justified panics. There are **6 bare `unreachable!()`** calls (no explanation message) in non-test code, five of them in [crates/jackin/src/console/tui/state.rs](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/crates/jackin/src/console/tui/state.rs) (lines ~1401, 1422, 1446, 1475, 1494). Most `unreachable!()` in the tree do carry a justification string (good); these six should too, so a future reader knows why the state is truly impossible. Low severity, but it's the exact class (Defect 40 was a render-path panic) the charter calls out.

## ✅ What the PR *did* get right (fair credit)

So the review isn't one-sided — these were verified and are genuinely done:

- **`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passes** (exit 0, zero warnings). Everything compiles, including all integration tests and benches.
- **3287 of 3288 tests pass** — the vast majority of the suite is green; the failure is one isolated label-assertion, not a systemic breakage.
- **Schema-versioning hard rule is fully satisfied:** `CURRENT_CONFIG_VERSION`/`CURRENT_WORKSPACE_VERSION` bumped to `v1alpha6` (one bump), the `v1alpha5 → v1alpha6` `MigrationStep` is registered in `CONFIG_MIGRATIONS`, `from-v1alpha5` fixtures exist for config **and** workspace, the older fixtures (`from-v1alpha1` …) carry re-baked `after.toml`, and the `schema-versions.mdx` Timeline has the `v1alpha6` entry with a before/after example. All five required artifacts are present.
- **Module-layout hard rule respected:** zero `mod.rs` files under `crates/`.
- **No broken docs links to the deleted tree:** zero `<RepoFile path="src/...">` references to the removed root `src/` remain.
- **`TODO.md` was updated** to reflect post-restructure state.

## Overall verdict

The PR achieves the **bulk** of its ambition — the workspace split, the `jackin-term` model, the `#523` port, the schema work, and the lint gate are real and largely sound. But it **does not meet its own stated definition of done**:

1. The "Green everywhere" gate is red (1 failing test) — and was marked done without being run.
2. ~32k LOC of dead migration residue remains (Part 1).
3. A long tail of acceptance criteria are checked off but deferred/unmeasured/unverified (Part 2): performance never benchmarked, diagnostics not span-sourced, shared primitives not adopted by surfaces, capsule still raw-ANSI, operator smoke never run.

"Achieved" is the right word for the *architecture*; "not yet done" is the honest status for the *acceptance gates*. The gap between the two is exactly the set of items this PR should close before merge.

---

# Part 4 — More gates are red, and the lint posture is not what the docs claim

Continuing the verification, I checked the remaining acceptance gates and CI itself. **The PR is failing its own CI right now**, and a second mandatory gate (`cargo fmt --check`) is red — on top of the test failure from Part 3.

## 🔴 The PR is currently RED on CI

`gh pr checks 495` reports the required checks as **failing**, not pending or passing:

| Check | Status |
|---|---|
| `check` (fmt + clippy + tests) | **fail** |
| `ci-required` | **fail** |
| `DCO` (commit sign-off) | **fail** |
| `build-validator`, `matrix.platform`, `publish manifest` | skipped (gated behind `check`) |

This is objective ground truth: regardless of what the roadmap checklist says, the PR **cannot merge in its current state**. The roadmap's repeated "Green everywhere" `[x]` claims are contradicted by the project's own CI.

## 🔴 `cargo fmt --check` FAILS (the actual current CI blocker)

The `check` job dies in ~29s — too fast for the test run — because it fails at the **first** step, `cargo fmt --check`. I reproduced it locally: **5 unformatted locations across 3 files**:

- [crates/jackin-capsule/src/daemon/compositor.rs](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/crates/jackin-capsule/src/daemon/compositor.rs) (2 spots, ~lines 187, 197)
- [crates/jackin-tui/src/components/container_info.rs](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/crates/jackin-tui/src/components/container_info.rs) (~line 289)
- [crates/jackin-tui/src/components/dialog_layout.rs](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/crates/jackin-tui/src/components/dialog_layout.rs) (~line 237)
- [crates/jackin-tui/src/components.rs](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/crates/jackin-tui/src/components.rs) (~line 28)

The roadmap marks "`cargo fmt --check` ✓" on essentially every defect's acceptance. It is false. **Action:** `cargo fmt` (one command) fixes all five. That this trivial gate is red is strong evidence the final gate sweep was never run before the items were checked off.

> Combined with Part 3, **two of the three mandatory gates are red** (`cargo fmt --check` and `cargo nextest run --workspace`); only `cargo clippy` is green. CI runs `cargo nextest run --workspace --all-features` (ci.yml:148), so the Part 3 test failure would block CI even after fmt is fixed.

## 🟠 Lint config: the documented enforcement does not exist, and 17 tables have drifted

[crates/AGENTS.md](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/crates/AGENTS.md) states: *"`clippy::mod_module_files = "deny"` is enabled in the workspace `[lints.clippy]` table and enforced by CI. Any PR that introduces a new `mod.rs` will fail."* I verified this is **factually wrong**:

- **There is no `[workspace.lints]` table** in the root [Cargo.toml](file:///Users/donbeave/Projects/jackin-project/test/pr-495/jackin/Cargo.toml). Instead, all 17 crates each hand-maintain their own `[lints.rust]` + `[lints.clippy]` tables.
- **`mod_module_files = "deny"` is missing from 7 of 17 crates** — including the two largest by LOC, `jackin-capsule` (25k) and `jackin-console` (25k), plus `jackin-tui`, `jackin-launch`, `jackin-protocol`, `jackin-tui-lookbook`, `jackin-build-meta`. A new `mod.rs` in any of those would **not** fail CI, contrary to the documented guarantee.
- **The tables have drifted into 3 tiers:** `pedantic`/`nursery`/`dbg_macro`/`unimplemented` are enforced in only 7 of 17 crates; 3 crates have `mod_module_files` only; 7 have neither. The biggest, most complex crates (capsule, console) have the **weakest** lint profile — backwards from what you'd want.
- This is the **DRY charter violation (#3)** applied to build config: 17 duplicated, drifting copies instead of one `[workspace.lints]` table consumed via `lints.workspace = true`. It's also why the lint posture is inconsistent and the docs are now wrong.

**Action:** hoist a single `[workspace.lints]` table into the root manifest, set `lints.workspace = true` in every crate, and reconcile the union (decide pedantic/nursery policy once — the best-practices charter says *don't* enable them wholesale in libraries, so this is the moment to make that call deliberately). Then update crates/AGENTS.md to match reality.

## 🟡 A promised benchmark target does not exist

The PR description's "Performance checks" section tells reviewers to run:

```sh
cargo bench --bench present_frame -p jackin-term
```

There is **no `benches/` directory and no `[[bench]]` declaration in `jackin-term`** — that command fails immediately. (The other two benches, `console_frame -p jackin` and `pane_body -p jackin-capsule`, do exist.) This reinforces Part 2 C.1: `jackin-term` performance was never benchmarked, and the verification instruction for it is broken.

## 🟡 Minor: CI clippy invocation differs from the documented gate

CI runs `cargo clippy --all-targets --all-features -- -D warnings` (ci.yml:102) — **without `--workspace`** and without `--locked`, unlike the documented gate `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`. In a virtual workspace this lints the default members, but the inconsistency is worth aligning so the local gate and CI gate are identical.

## Updated overall verdict

Part 3 said the suite was red on one test. Part 4 shows the situation is broader: **the PR is failing CI on multiple required checks right now** (`check`, `ci-required`, `DCO`), with two of three mandatory code gates red (`fmt`, `nextest`). The "Green everywhere" acceptance criterion — marked `[x]` across the whole Phase 2 epic — is demonstrably false at every level: locally, and on the project's own CI.

None of these are hard to fix individually (`cargo fmt`; one test/label decision; commit sign-off). The concern is what their presence implies: **the final acceptance sweep that the roadmap repeatedly claims to have run was not actually run.** The items were marked done by inspection, not by execution. That is the throughline across Parts 2–4, and it is the thing to correct — both the specific failures and the practice of checking off gates without running them.

---

*Generated as a review summary for PR #495. Part 2: roadmap-vs-code gaps. Part 3: workspace test suite is red. Part 4: PR is failing CI — `fmt` red, lint config drifted, a promised bench missing.*
