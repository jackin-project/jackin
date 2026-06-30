# Codebase health — remaining work to close the roadmap item

Clean, verified-against-code plan of **everything still open** under
[Codebase health: structure & reviewability](/roadmap/codebase-health-enforcement/).
This file is the live worklist. Slices A1–G3 of the original execution playbook
have all shipped (their mechanical per-slice log lives in git history); only the
open set below remains.

**Verification basis:** every item below was checked against the actual tree on
branch `feature/private-registry-auth` (HEAD at writing), **not** against roadmap
checkboxes. Where the roadmap `[x]` and the code disagree, the code wins and the
discrepancy is called out.

The invariant from the roadmap still governs every slice: **structure only — never
behavior.** No logic, control-flow, signature, or performance change. The existing
test suite + the `runtime-launch` / `op-picker` behavioral specs must pass
**unmodified**; a forced test edit means behavior changed → back out.

---

## Definition of done (when this roadmap item finally closes)

From the roadmap's critical-path closer: the item stays **open** until **all** of:

1. The last inverted edge `jackin-runtime → jackin-tui` is broken (R1) and the
   dependency-direction gate runs in **`--strict`** mode in CI (R2).
2. Every god-crate carve is complete — E1 finished (R3); E2 done.
3. The W5 file-size backlog is cleared: no production `.rs` over the 2000L cap (R4),
   including the editor/settings models once W6/unify lands (R5).
4. The W2 clippy grandfather backlog is burned down: zero
   `#[expect(clippy::…, reason = "tracked in codebase-health-enforcement")]` remain (R6).
5. Then — and only then — thresholds tighten toward target: files ≤ 1500L,
   fns ≤ 150 logical lines, clippy thresholds ratcheted down (R7).

R8–R11 are durability / hygiene / bookkeeping that ride alongside and must also land
before the umbrella tracker is checked off.

---

## Snapshot — what is DONE (no action; listed so the open set is unambiguous)

- **A0–A5** boundary fixes + `cargo-deny` workspace-dep hygiene + 19/19 crate
  Architecture-Invariant headers. `FORBIDDEN_EDGES` dropped 3 → **1**.
- **B1** `jackin-launch` → `jackin-launch-tui` (old crate dir gone, confirmed).
- **B2** all 19 binary shim modules deleted.
- **C1/C2** `jackin-host`, `jackin-usage` carved (crates exist).
- **D1–D4** image/env/op_cache/naming dedups.
- **E0** `lto = "thin"` + launch/attach baseline. **E2** `jackin-instance` carved.
- **E1 (partial)** — `branch`/`cleanup`/`materialize`/`state` moved into
  `jackin-isolation`; **`finalize.rs` + `git_inspect.rs` did NOT move → see R3.**
- **F1/F2** `app_config/` coordinator + TEA stem normalization.
- **G0–G3** shared `jackin-tui` Elm runtime + all four stacks migrated.
- **W5** `usage.rs` fully decomposed; `tui/model.rs` split (coordinator 101L).
- **W2** clippy lints flipped to `warn` with thresholds; **W3** file-size ratchet gate;
  **W4** arch gate + `cargo-deny` + `cargo-shear` trio (machete/udeps deliberately not adopted).

---

## The open set

> Legend per slice: **Goal · State (verified) · Touches · Steps · Verify · Done-when**.
> Standard Verify (run in order, stop on first red) unless a slice overrides:
> `cargo fmt --check` · `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
> · `cargo nextest run --all-features` · `cargo run -p jackin-xtask --locked -- lint`
> · behavioral specs `runtime-launch` + `op-picker` pass **unmodified**.

---

### R1 — Break the last inverted dependency: `jackin-runtime → jackin-tui`  ⟵ gate-keeper

- [ ] **Goal.** Remove every production use of `jackin_tui::*` from `jackin-runtime`
  so the L1→L3 edge disappears, per **D2** (trait ports for data flows; plain
  relocation for misplaced values). E0 LTO baseline shipped, so the perf gate that
  parked this is **lifted** — this is unblocked.

- **State (verified).** `FORBIDDEN_EDGES` in `crates/jackin-xtask/src/arch.rs:53`
  still lists `("jackin-runtime", "jackin-tui")`; the `//!` table at `arch.rs:10`
  flags it as the one open P2 inversion. Production uses (test files excluded):

  | File | Symbol | Kind | Target per D2 |
  |---|---|---|---|
  | `runtime/host_attach.rs` | `jackin_tui::url_text::redact_url_for_log` | pure fn | **relocate-repoint** — `url_text` already exists in `jackin-core` (A5 prep 5). Repoint to `jackin_core::url_text::redact_url_for_log`. Trivial. |
  | `runtime/progress.rs` | `jackin_tui::theme::DANGER_RED` | const color | **relocate** — move const to `jackin-core` (same pattern as A5 prep 1 lifted `Rgb`/PHOSPHOR palette). |
  | `runtime/progress.rs` | `jackin_tui::ansi::{POINTER_HAND, POINTER_DEFAULT}` | const str | **relocate** to `jackin-core` (pure escape-sequence constants, no ratatui). |
  | `runtime/progress.rs` | `jackin_tui::ansi::encode_osc52_clipboard_write` | pure fn | **relocate** to `jackin-core` (string transform, no ratatui dep). |
  | `runtime/progress.rs` | `jackin_tui::components::{ErrorPopupState, TextInputState, ConfirmState}` | UI state structs | **port or relocate** — these are presentation state. Decide: (a) relocate the plain-data state structs to `jackin-core` if they carry no ratatui types, or (b) hide construction behind a progress port the runtime already owns (A1/A2 established the `LaunchDiagnostics`/`LaunchHostTerminal` port pattern in `jackin-core/src/launch_progress.rs` — extend it). Inspect the structs' fields first. |
  | `runtime/launch.rs` | `jackin_tui::output::{step_fail, print_deploying}` | terminal side-effect | **port** — define `trait LaunchOutputSink` in `jackin-core` (`step_fail(&str)`, `print_deploying(&str)`); `jackin-tui` impls; `jackin` binary injects. Branch-by-Abstraction. |
  | `runtime/launch.rs` | `jackin_tui::animation::{warp_out, warp_end_caption}` | terminal animation | **port** — same sink trait or a sibling `LaunchAnimator` port; impl in `jackin-tui`, injected at entry. |

- **Touches.** `crates/jackin-core/src/` (new const/fn relocations + port trait(s)),
  `crates/jackin-runtime/src/runtime/{host_attach.rs, progress.rs, launch.rs, lib.rs}`,
  `crates/jackin-tui/src/` (port impls), `crates/jackin/src/` (port injection at the
  launch call site), `crates/jackin-xtask/src/arch.rs` (drop the edge — done in R2).

- **Steps.**
  1. **Quick win first:** repoint `host_attach.rs` to `jackin_core::url_text::redact_url_for_log`; delete the `jackin_tui::url_text` use. Verify the symbol is byte-identical in core.
  2. Relocate pure consts/fns (`DANGER_RED`, `POINTER_*`, `encode_osc52_clipboard_write`) into `jackin-core` (a `progress_tokens.rs` or extend an existing module); leave a `pub use` re-export in `jackin-tui` so its own callers compile unchanged (Parallel Change). Repoint `progress.rs` to the core path.
  3. Inspect `ErrorPopupState`/`TextInputState`/`ConfirmState` field types. If plain data → relocate to core + re-export from tui. If they embed ratatui types → keep in tui and hide the runtime's construction behind a port.
  4. Define the launch-output port: `pub trait LaunchOutputSink` in `jackin-core` covering `step_fail` / `print_deploying` / `warp_out` / `warp_end_caption` (or split into output vs animator ports if cleaner). `jackin-runtime` takes `&dyn LaunchOutputSink`; `jackin-tui` provides the impl; `jackin` injects it at the launch call site — mirror exactly how A1's `BuildLogSink` / A2's `LaunchDiagnostics` are wired.
  5. Remove the now-dead `jackin-tui` dependency line from `crates/jackin-runtime/Cargo.toml` (and the `lib.rs` doc-comment references to `jackin_tui::output`/`components`).
- **Verify.** Standard Verify **+** `cargo run -p jackin-xtask --locked -- lint arch --strict`
  now passes with the edge gone. Launch cockpit + progress panes render identically;
  run the `runtime-launch` spec. If a hot path crossed a new boundary, re-run the E0
  launch/attach benchmark and attach numbers (no regression).
- **Done-when.** `grep -rn 'jackin_tui' crates/jackin-runtime/src` returns **zero**
  production hits (test-only, if any, also removed); `jackin-tui` no longer in
  `jackin-runtime/Cargo.toml`.

---

### R2 — Flip the dependency-direction gate to `--strict` in CI

- [ ] **Goal.** Make the architecture machine-enforced: once R1 lands, the gate fails
  on any forbidden edge instead of merely reporting.
- **State.** `arch.rs:48` `FORBIDDEN_EDGES` has 1 entry (the R1 edge). Gate runs
  non-strict (`arch::check(strict)` with `strict=false` on the normal path;
  `arch.rs:63` comment: "exits 0 … while the inversions are still" present).
  `run_all_lints(strict)` in `crates/jackin-xtask/src/main.rs:111` threads the flag.
- **Touches.** `crates/jackin-xtask/src/arch.rs` (empty `FORBIDDEN_EDGES` to `&[]`),
  `crates/jackin-xtask/src/arch/tests.rs` (the `synthetic_graph_flags_only_listed_forbidden_edges`
  assertion — verify exact expected count after R1), `.github/workflows/ci.yml`
  (call `cargo xtask lint --strict` on the file-size-gate job).
- **Steps.**
  1. After R1: set `FORBIDDEN_EDGES = &[]` (or keep the synthetic-test fixtures and
     assert zero real violations).
  2. Update the arch synthetic-graph test to the post-R1 reality.
  3. Switch the CI invocation to `cargo xtask lint --strict` so a reintroduced
     inversion fails the PR.
- **Verify.** Standard Verify; `cargo xtask lint --strict` green; deliberately add a
  throwaway bad edge locally and confirm it now **fails** (then revert).
- **Done-when.** CI runs `--strict`; no production inversion remains.

---

### R3 — Finish E1: move `finalize.rs` + `git_inspect.rs` into `jackin-isolation`

- [ ] **Goal.** Complete the isolation carve so `jackin-runtime` owns no isolation code.
- **State (verified).** `crates/jackin-runtime/src/isolation/finalize.rs` and
  `…/isolation/git_inspect.rs` are still in `jackin-runtime`; the other four
  isolation sub-modules already live in `crates/jackin-isolation/src/`. The L1→L3
  inversion that originally parked them is **closed** (both now route dialogs through
  `jackin_core::exit_dialog_with_inspect` / `jackin_core::error_popup`, with
  `jackin_launch_tui::install_standalone_dialog_sink` installing the impl at CLI
  start-up — per roadmap E1 note). The only remaining blocker was the in-place tests
  depending on `jackin_runtime::test_support::FakeRunner`.
- **Touches.** `git mv` the two files + their `tests.rs` into `crates/jackin-isolation/src/`;
  `crates/jackin-isolation/Cargo.toml` (add a `[dev-dependencies]` on `jackin-runtime`
  with `features = ["test-support"]` to reach `FakeRunner`, OR move `FakeRunner` to a
  shared `jackin-isolation` test-support module); importer repoints in `jackin-runtime`;
  `crates/jackin-isolation/src/lib.rs` (`pub mod` + re-export); `PROJECT_STRUCTURE.md`
  + Codebase Map.
- **Steps.** Follow the crate-carve recipe (playbook stub references it):
  1. `git mv` `finalize.rs`, `git_inspect.rs`, and their sibling `tests.rs` verbatim.
  2. Resolve the `FakeRunner` access: prefer a `jackin-isolation` `[dev-dependencies]`
     edge on `jackin-runtime` (`features = ["test-support"]`) exposing
     `jackin_runtime::runtime::test_support::FakeRunner`. Run `cargo check --workspace`
     immediately — if Cargo rejects a cycle, fall back to a local fake in
     `jackin-isolation` test-support.
  3. Check `finalize.rs`'s old use of `crate::runtime::attach::JACKIN_STATUS_CMD`
     (`pub const`) and `parse_session_count` (`pub(crate)` — **not** reachable
     cross-crate). If still referenced, relocate those two to `jackin-core` in a
     prep step first; do **not** guess — verify against the file before moving.
  4. Repoint `jackin-runtime` importers to `jackin_isolation::{finalize, git_inspect}`;
     delete the now-empty `runtime/isolation/` shim if any remains.
- **Verify.** Standard Verify + `cargo nextest run -p jackin-isolation -p jackin-runtime`;
  E0 benchmark (isolation is on the launch hot path) — no regression vs baseline.
- **Done-when.** `crates/jackin-runtime/src/isolation/` is gone; `grep -rn 'mod finalize\|mod git_inspect' crates/jackin-runtime` is empty.

---

### R4 — W5 production file decompositions (bring every prod `.rs` under 2000L)

- [ ] **Goal.** Clear the file-size grandfather list so the cap holds with no exceptions.
- **State (verified, `file-size-budget.toml` + live `wc -l`).** Production files still
  over the 2000L cap, each a decomposition slice (split pattern: keep the file as the
  coordinator, move clusters to sibling `<module>/<name>.rs`, `pub(super)` what the
  coordinator/siblings call, tests in `<module>/tests.rs`, no `mod.rs`, no wildcard):

  - [ ] `crates/jackin-runtime/src/runtime/launch.rs` — **2834L**. Split the launch
        coordinator clusters (the +1-over-budget note in the manifest is here; this
        slice should drop it well under cap).
  - [ ] `crates/jackin-console/src/tui/screens/editor/view.rs` — **2389L**.
  - [ ] `crates/jackin-capsule/src/tui/components/dialog.rs` — **2265L**.
  - [ ] `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs` — **2213L**.
  - [ ] `crates/jackin-console/src/tui/screens/editor/model.rs` — **4176L** → **R5** (W6-gated).
  - [ ] `crates/jackin-console/src/tui/screens/settings/model.rs` — **3852L** → **R5** (W6-gated).
  - [ ] **Stale budget entry:** `crates/jackin-runtime/src/runtime/image.rs` is
        **1952L < 2000 cap** — it is *under* the cap yet still grandfathered. Per the
        ratchet rule ("delete an entry when its file drops under the cap"), **prune
        this `[[production]]` block from `file-size-budget.toml`** (bookkeeping, no
        code move). Roadmap W5 table also still prints it as `2811L` — see R11.

  Test files (`launch/tests.rs` 8603L, `daemon/tests.rs` 7612L) are **under** the
  10000L test cap — not this item's blocker; their real fix is tracked in
  [test-infra-behavioral-specs](/roadmap/test-infra-behavioral-specs/). Leave grandfathered.
- **Steps (per file).** Apply the proven split pattern; after each split refresh the
  ratchet: `cargo run -p jackin-xtask --locked -- lint files --print-budget` over
  `file-size-budget.toml`, prune the now-fixed entry. One file = one PR.
- **Verify.** Standard Verify; `cargo xtask lint files` green; the split file's
  `tests.rs` passes unmodified.
- **Done-when.** `file-size-budget.toml` `[[production]]` list is empty (only the two
  test entries, if still present, remain).

---

### R5 — editor + settings model collapse (W6 / unify-settings dependency)

- [ ] **Goal.** Bring `editor/model.rs` (4176L) and `settings/model.rs` (3852L) under
  cap by splitting the **unified** surface, not the duplicated pair.
- **State.** **Blocked** on
  [unify-settings-editor-surfaces](/roadmap/unify-settings-editor-surfaces/), which is
  "Partially implemented — structural unification of the two config-editing screens is
  open." Splitting the duplicated pair now would split work that's about to be merged.
- **Prerequisite.** unify-settings lands the single config-editing surface on the G0
  shared contract. Only then split.
- **Steps.** After unify lands: re-read the unified model file; split along its real
  cluster boundaries into coordinator + siblings; refresh the budget ratchet.
- **Done-when.** Both `editor/model.rs` and `settings/model.rs` (or their unified
  successor) sit under 2000L; W6 checkbox closes.

> This is the one open item that depends on a **separate** roadmap item. Track it as
> the long-pole; everything else (R1–R4, R6–R11) is independent.

---

### R6 — Burn down the clippy `#[expect]` grandfather backlog (58 sites)

- [ ] **Goal.** Zero `#[expect(clippy::…, reason = "tracked in codebase-health-enforcement")]`.
- **State (verified, `grep`).** 58 grandfathers remain:

  | Lint | Count | Fix shape |
  |---|---|---|
  | `clippy::struct_excessive_bools` | **40** | bundle the bool fields into a flags struct / state enum, or a typed config struct. Dominant backlog — biggest single win. |
  | `clippy::fn_params_excessive_bools` | **10** | replace bool params with an options struct or enum variants. |
  | `clippy::too_many_lines` | **4** | extract helpers (overlaps R4 file splits). |
  | `clippy::too_many_arguments` | **9 → arg threshold; 4 expects** | introduce a params/builder struct. |

  Carriers by crate: `jackin-console` ×13 files, `jackin-capsule` ×5, `jackin-term` ×4,
  `jackin-runtime` ×4, `jackin` ×3, `jackin-protocol` ×2, others ×1.
- **Steps.** Per `#[expect]`: refactor to satisfy the lint (structure-only — a bool
  bundle is a type change with identical behavior), then delete the `#[expect]`. If a
  site is genuinely irreducible, escalate to operator for a permanent narrow allow with
  a *different* reason string (so it leaves the burn-down set). Cluster by crate to
  keep PRs reviewable.
- **Verify.** Standard Verify; the touched crate's tests pass unmodified.
- **Done-when.** `grep -rn 'tracked in codebase-health-enforcement' crates --include='*.rs'`
  returns zero.

---

### R7 — Ratchet thresholds toward target (final tightening)

- [ ] **Goal.** After R4 + R6, tighten the gates so the *target* shape is enforced, not
  just today's maxima.
- **State.** `clippy.toml`: `too-many-lines-threshold = 450`,
  `cognitive-complexity-threshold = 100`, `excessive-nesting-threshold = 8`,
  `too-many-arguments-threshold = 9`. `file-size-budget.toml`: `production_cap = 2000`.
  Roadmap target: files ≤ **1500L**, fns ≤ **150** logical lines. ~19 production files
  currently exceed 1500L (the R4 set plus `cli/diagnostics.rs` 1964, `grid.rs` 1889,
  `session.rs` 1733, `settings/update.rs` 1717, `daemon.rs` 1670, `settings/view.rs`
  1635, `tui/model/modal.rs` 1587, `input/mouse.rs` 1558, `footer_hints.rs` 1526,
  `dialog_widgets.rs` 1510 …).
- **Steps.** Sequence **after** R4/R6 so nothing breaks on the day of the flip:
  1. Lower `production_cap` 2000 → 1500 incrementally; decompose the new over-cap set
     (the 1500–2000L files above) the same way as R4.
  2. Lower `too-many-lines-threshold` toward 150; refactor/expect as you go.
  3. Lower `cognitive-complexity`, `excessive-nesting` (8→5), `too-many-arguments`
     toward target; never blanket-`allow`.
- **Verify.** Standard Verify at each ratchet notch.
- **Done-when.** Caps at target with no new grandfathers; the budget/expect lists stay empty.

---

### R8 — Duplicate-version debt burn-down (`deny.toml`)

- [ ] **Goal.** Move `multiple-versions` from `warn` toward `deny` once the transitive
  duplicate tail is paid down, so version drift can't silently grow.
- **State (verified).** `deny.toml:111` `multiple-versions = "warn"`; **35** crates in
  the `[bans] skip` list, each tagged "Existing duplicate-version debt." This is
  acknowledged debt, not a regression.
- **Steps.** Periodically: `cargo tree -d` (or `cargo deny check bans`) → for each
  skipped crate, attempt to unify versions by bumping the lagging dependent; delete the
  skip entry when the duplicate resolves. When the list is small/irreducible (pure
  transitive forks), flip `multiple-versions = "deny"` and keep only the unavoidable
  skips with sharpened reasons.
- **Done-when.** `skip` list holds only genuinely-irreducible transitive dupes;
  `multiple-versions = "deny"`.

> Lower priority than R1–R6 (it's hygiene, not navigability), but part of "keep the
> map durable." Acceptable to land last among the enforcement items.

---

### R9 — W4 optional: publish the dependency graph as a CI artifact

- [ ] **Goal.** Make the layering visible/reviewable (the one unchecked W4 box).
- **State (verified).** No `cargo-modules` / graph step in `.github/`. Roadmap W4
  bullet is `[ ]` "Optional: a `cargo-modules` dependency graph published as a CI artifact."
- **Steps.** Add a non-blocking CI job that runs `cargo modules` (or
  `cargo depgraph`) and uploads an SVG/DOT artifact per PR. Informational only — does
  not gate. Reconcile tool choice so it adds no `deny.toml` license burden.
- **Done-when.** CI uploads a layering graph artifact; roadmap W4 optional box checked
  (or explicitly closed as "won't do" with a one-line why).

---

### R10 — D7 deferred decision: `jackin-config` persistence IO edge

- [ ] **Goal.** Settle the one still-open D7 sub-decision: does `jackin-config`'s file-IO
  persistence (`persist.rs`) split into an L2 adapter to keep the schema crate strictly
  IO-free, or does the schema crate keep a thin, documented IO edge?
- **State (verified).** `crates/jackin-config/src/persist.rs` exists alongside
  `app_config.rs`. The other two D7 deferrals are **already resolved**: (b) tool overlap
  — `cargo-shear` chosen, `cargo-machete`/`udeps` not adopted; (c) `FakeOpWriter` dedup —
  done into `jackin_env::test_support` (no duplicate remains in `jackin/src/app/tests.rs`).
  So only the persistence-IO ruling is open.
- **Steps.** Operator decision. If "split": carve `persist.rs` into an L2 adapter crate
  / module and leave `jackin-config` schema-only (recipe move). If "keep thin edge":
  document the exception in the crate's `//!` Architecture-Invariant header and record
  the decision on the roadmap so it's not re-litigated.
- **Done-when.** Decision recorded on the roadmap; if "split," the move has shipped and
  the arch gate reflects it.

---

### R11 — Doc & bookkeeping cleanup (no code)

- [x] **Retire the execution playbook.** *(done)* The 6870-line A1–G3 log
  `plans/codebase-health-playbook.md` was deleted; this file replaces it (full
  per-slice detail survives in git history).
- [x] **Fix the stale W5 line count in the roadmap.** *(done)* `runtime/image.rs`
  corrected `2811` → `1952L (under cap, D1)` in the roadmap.
- [x] **Reconcile the W4 `deny.toml` bullet wording.** *(done)* Reworded so the
  directional teeth read as the `cargo xtask lint arch` gate, not `deny.toml`
  (`cargo-deny [bans]` cannot express workspace-crate→crate edges).
- [ ] **Prune the under-cap budget entry.** Remove the `runtime/image.rs` `[[production]]`
  block from `file-size-budget.toml` (now 1952L < 2000 cap; covered operationally by R4).
- [ ] **Security-advisory tracking note.** `deny.toml` ignores `RUSTSEC-2023-0071` (rsa
  Marvin) + `RUSTSEC-2026-0173` with reasons. Not part of this item, but add a one-line
  "revisit on next sigstore/oci-client bump" tracking note so the ignore doesn't ossify.
- **Done-when.** Budget entry pruned + advisory note added (the playbook, W5 number,
  and W4 wording are already done). All under the roadmap's docs-freshness + repo-links
  gates (`bun run check:roadmap-sidebar`, `bun run check:repo-links`).

---

## Ordering / critical path

```
R1 ──▶ R2            (break runtime→tui, then flip arch --strict)
R3                   (finish E1 — independent, hot-path; needs E0 baseline = shipped)
R4 ──▶ R7            (clear 2000L backlog, THEN tighten caps to 1500L)
R6 ──▶ R7            (burn clippy backlog, THEN tighten thresholds)
R5                   (LONG POLE — blocked on external unify-settings item)
R8, R9, R10, R11     (independent hygiene/decision/docs — land anytime)
```

- **Closes the item:** R1+R2 (last inversion enforced) · R3 (E1 done) · R4+R5 (file
  backlog clear) · R6 (clippy clear) → **then** R7 (target thresholds). R8–R11 ride
  alongside.
- **Most-blocking unknown:** R5 waits on `unify-settings-editor-surfaces`. Everything
  else can proceed in parallel today.
- **Biggest single lever:** R1 — it's the one architectural edge the whole umbrella
  has been held open for; landing it + R2 converts the architecture from
  "reviewer-upheld" to "CI-enforced."
- **Biggest grind:** R6 (58 expects, 40 of them `struct_excessive_bools`) and R4/R7
  (file decompositions) — mechanical, parallelizable, one PR each.

## Per-PR checklist (every slice above)

- [ ] Scope = exactly one slice; structure-only (no logic/behavior/perf change).
- [ ] `cargo fmt --check` · clippy `-D warnings` · `cargo nextest run --all-features` green.
- [ ] Behavioral specs `runtime-launch` + `op-picker` pass **unmodified**.
- [ ] `cargo xtask lint` green; refresh the relevant ratchet + prune fixed entries.
- [ ] New/renamed crate: `[lints] workspace = true` + Architecture-Invariant `//!` header.
- [ ] Docs synced same PR: `PROJECT_STRUCTURE.md` + Codebase Map + the roadmap slice box.
- [ ] (carve / hot-path slices) E0 launch/attach benchmark shows no regression.
- [ ] DCO sign-off (`-s`); push immediately.
