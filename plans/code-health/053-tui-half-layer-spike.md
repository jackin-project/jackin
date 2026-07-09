# Plan 053: TUI runtime half-layer spike — prototype one shared View dispatcher, then finish or drop the trait

> **Executor instructions**: This is a DESIGN/SPIKE plan — the deliverable is
> a decision backed by a working prototype, not a finished migration. Follow
> the steps; the decision criteria in Step 3 are binding. If anything in
> "STOP conditions" occurs, stop and report. When done, update the status row
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat fabe88406..HEAD -- crates/jackin-tui/src/runtime.rs crates/jackin-console/src/tui/runtime.rs crates/jackin-capsule/src/tui/runtime.rs crates/jackin-launch-tui/src/tui/model.rs`
> Any change to these four files since the excerpts: compare before
> proceeding; a fourth View implementor or a new dispatcher appearing is a
> STOP (the decision landscape changed).

## Status

- **Priority**: P3
- **Effort**: S-M (spike; the follow-through migration or removal is sized BY the spike)
- **Risk**: MED contained — the spike itself changes one loop behind a flagless refactor or nothing at all; the risk it exists to measure is "a shared driver adds indirection for no gain"
- **Depends on**: none (soft: coordinate with plan 030 — it touches console view builders, disjoint from the loop/dispatch layer, but rebase whichever lands second)
- **Category**: tech-debt / direction
- **Planned at**: commit `fabe88406`, 2026-07-09

## Why this matters

Roadmap Phase 2 (TUI/console item 1): "Finish or remove the shared TUI runtime half-layer. The preferred path is to dispatch console, launch, and capsule loops through the shared `jackin_tui::runtime` traits so the abstraction is operational, not just type-level." The `View` trait has exactly three implementors and ZERO dispatchers — each loop still calls its own render directly — the textbook type-only abstraction the crate's own AGENTS rule warns against ("The `runtime` traits are the dispatch point console/launch/capsule loop through — keep them operational, not type-only"). Every month it sits half-built, new loop code entrenches one side or the other. A bounded spike answers the finish-vs-drop question with evidence instead of letting the item rot as "an operator decision" with no material to decide on.

## Current state

Verified at `fabe88406`.

- The trait — `crates/jackin-tui/src/runtime.rs:266-268`:
  ```rust
  pub trait View<Model> {
      fn render(&self, model: &Model, frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect);
  }
  ```
  Doc contract (:259-265): observational — reads the model, never mutates, never drives subscriptions.
- The three implementors (all thin):
  - `crates/jackin-console/src/tui/runtime.rs:23` — `impl jackin_tui::runtime::View<crate::tui::console::ConsoleState> for ConsoleView<'_>`
  - `crates/jackin-capsule/src/tui/runtime.rs:16` — `impl jackin_tui::runtime::View<CapsuleRatatuiFrame<'_>> for CapsuleView` (note: the "model" here is a FRAME-shaped type with a lifetime — the heterogeneity a shared driver must survive)
  - `crates/jackin-launch-tui/src/tui/model.rs:113` — `impl jackin_tui::runtime::View<LaunchView> for LaunchViewView<'_>`
- Zero dispatchers: no generic `fn drive<M, V: View<M>>` or equivalent exists; each loop (console event loop, capsule compositor, launch cockpit) renders via direct calls. The OTHER half of `runtime.rs` (`Subscription`, `UpdateResult`, `spawn_*_subscription` — read the rest of the 270-line file) IS operational and widely consumed — this spike touches only the `View`-dispatch half.
- Roadmap-recorded preference: finish (dispatch loops through the trait). The honest counter-hypothesis: three loops with genuinely different frame/model shapes (`&ConsoleState` vs `CapsuleRatatuiFrame<'_>` vs `LaunchView`) may share too little for a driver to pay for its generics.
- TUI rules that bind any real change: read `docs/content/docs/reference/tui/index.mdx` before touching loop behavior (repo hard rule); rendering behavior must be pixel-identical (snapshot tests exist in console via insta — they are the oracle).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Console tests (incl. snapshots) | `cargo nextest run -p jackin-console` | all pass, no pending snaps |
| TUI crate tests | `cargo nextest run -p jackin-tui` | all pass |
| Capsule + launch-tui tests | `cargo nextest run -p jackin-capsule -p jackin-launch-tui` | all pass |
| Clippy | `cargo clippy -p jackin-tui -p jackin-console --all-targets -- -D warnings` | exit 0 |
| Full gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `crates/jackin-tui/src/runtime.rs` (+ `runtime/tests.rs`) — the prototype driver (Step 1) and, on a DROP verdict, the `View` trait + its context-struct remnants' removal
- `crates/jackin-console/src/tui/` loop wiring — ONE loop converted (Step 2), console chosen (richest snapshot coverage = strongest oracle)
- On DROP: the three impl blocks deleted
- `docs/content/docs/reference/tui/index.mdx` — one paragraph recording the outcome (TUI cross-cutting behavior rule)
- Roadmap Phase 2 TUI item 1 status
- `crates/jackin-tui/README.md` if the public API changes either way

**Out of scope** (do NOT touch):
- Converting the capsule and launch loops (on FINISH, they are the follow-up plan the spike sizes; on DROP, nothing to do).
- The `Subscription`/`UpdateResult` half of runtime.rs — operational, untouched.
- Any visual/behavioral change — snapshots must not change in either verdict.
- Plan 030's view-model structs (different layer).

## Git workflow

- Branch off `main`: `refactor/tui-view-dispatch-spike`.
- Conventional Commits, `-s`, push per commit. PR to `main`; do not merge. If capsule files change (DROP verdict deletes its impl) → capsule smoke block.

## Steps

### Step 1: Prototype the shared driver

In `runtime.rs`, add the smallest useful dispatcher — target shape (adjust to what the console loop actually needs after reading it):

```rust
pub fn render_view<M, V: View<M>>(view: &V, model: &M, frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect) {
    view.render(model, frame, area);
}
```

…is trivially useless alone — the spike's real question is one level up: extract the console loop's per-frame sequence (begin frame → compute area(s) → render view(s) → cursor/overlay post-pass) into a generic `drive_frame` helper the OTHER two loops could also call. Read the console event loop's render section and the capsule compositor's `compose_ratatui_frame` FIRST; write the helper against their common denominator. Record (in the spike report, Step 3) every place the shapes diverge and what adapter glue each divergence costs.

**Verify**: `cargo check -p jackin-tui` → exit 0.

### Step 2: Convert the console loop

Route the console loop's frame rendering through the Step 1 helper + its existing `View` impl. Zero behavior change: every console snapshot test passes UNCHANGED (no `.snap` diffs, no pending snaps) — that is the hard gate for the conversion being real.

**Verify**: `cargo nextest run -p jackin-console` → all pass; `git status` shows no `.snap` modifications.

### Step 3: Verdict, by criteria fixed here

Compute and record in the PR body:
- **Glue cost**: net LOC added to console (helper + adapter) vs LOC removed from the loop.
- **Generality evidence**: for capsule + launch, from reading their loops (no code): does `drive_frame` fit without a trait redesign? Count the divergences (the `CapsuleRatatuiFrame<'_>` lifetime shape is the known hard case).
- **Type friction**: any `for<'a>`/HRTB or boxed-erased contortion the generic forced.

**Verdict rule (binding)**: FINISH if net glue ≤ ~40 LOC per loop AND no trait redesign needed for the other two AND zero HRTB contortions — then this PR keeps the console conversion and the README index gets a follow-up entry ("convert capsule + launch loops", sized by the measured console cost). DROP otherwise — then this PR instead deletes the Step 1 helper, the console conversion, the `View` trait (:266-268), its three impls, and any now-orphaned context structs in runtime.rs (compiler finds them: delete the trait, fix what reds), leaving the operational subscription half untouched.

**Verify**: whichever verdict — `cargo nextest run -p jackin-tui -p jackin-console -p jackin-capsule -p jackin-launch-tui` → all pass; console snapshots byte-identical.

### Step 4: Record the outcome

TUI reference page (`docs/content/docs/reference/tui/index.mdx`): one paragraph — what was decided, why, what the dispatch contract now is (or that per-loop rendering is the settled pattern). Roadmap Phase 2 TUI item 1 → resolved (either way, the item retires). jackin-tui README public-API section if the trait was removed or the driver was added. Update `crates/jackin-tui/AGENTS.md`'s "keep them operational, not type-only" rule to match reality (on DROP, the rule's View clause dies with the trait).

**Verify**: `cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask lint agents` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- No new tests on FINISH beyond what the helper needs (a unit test driving a trivial `View` impl through `drive_frame` in `runtime/tests.rs`).
- The console snapshot suite is the conversion oracle — unchanged snapshots or the spike fails its own bar.
- On DROP: the workspace compiling with the trait gone IS the test (plus the four crate suites green).

## Done criteria

- [ ] Spike report in PR body: glue LOC, divergence count for capsule/launch, type-friction notes, verdict per the binding rule
- [ ] FINISH → console loop dispatches through the shared helper, snapshots unchanged, follow-up entry filed in README index; DROP → View trait + impls + helper gone, subscription half untouched
- [ ] TUI docs paragraph + roadmap item resolved + AGENTS rule reconciled
- [ ] Four crate suites + clippy + `ci --fast` green
- [ ] `plans/code-health/README.md` row updated (with the verdict)

## STOP conditions

Stop and report back if:

- A console snapshot changes under the conversion (the driver altered rendering — the exact failure the spike must not paper over).
- The console loop's render section resists extraction without touching input/subscription handling (scope creep into the operational half).
- A fourth View implementor or an existing dispatcher appears at HEAD (drift check) — the decision inputs changed.
- The DROP path's compiler-guided deletion wants to remove anything in the subscription half.

## Maintenance notes

- FINISH path's follow-up (capsule + launch conversion) must reuse the measured console pattern verbatim — if the capsule's frame-lifetime shape forces a second driver variant, that is evidence the verdict was wrong; escalate rather than fork the abstraction.
- DROP path: the deleted trait's doc contract ("observational render") survives as an AGENTS rule if reviewers want it — behavior rules can outlive the type that carried them.
- Reviewer scrutiny: the verdict's arithmetic (glue LOC honestly counted) and snapshot byte-identity.
