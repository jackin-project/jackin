# Plan 017: Decompose the root run_console event loop into named step functions

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin/src/console/tui/run.rs`
> On mismatch with "Current state": STOP.

## Status

- **Execution status**: DONE — `TerminalSession::suspend` now owns the token-generation suspend/resume path; `run_console` is a small coordinator backed by named step functions in `crates/jackin/src/console/tui/run/steps.rs`; codebase-map references were updated for the split; package/full verification passed locally except docker-e2e, which remains a merge-readiness gate.
- **Priority**: P3
- **Effort**: M
- **Risk**: MED–HIGH (owns raw-mode/alt-screen lifecycle; a mis-extraction can leave the operator's terminal broken on error paths)
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

`run_console` in `crates/jackin/src/console/tui/run.rs` runs from line 187 to the file end at 843 — one `async fn` owning the tick loop, event dispatch, mouse routing, quit-confirm, launch-prompt flow, and terminal suspend/resume for token generation. Fairness note: the function carries an explicit `#[allow(too_many_lines)]` with a written justification ("per-stage / per-event-arm nested dispatch … the per-stage console event-loop protocol") — the size is a known, documented tradeoff, so this is a soft finding: worth doing because untestable orchestration around terminal-state lifecycle is the risky kind, not because a lint says so. The goal is named, individually testable step functions and a hard guarantee that terminal restore runs on every exit path — with zero behavior change.

## Current state

- `crates/jackin/src/console/tui/run.rs` (843 lines total). `pub async fn run_console<H: InstanceActionHandler<jackin_core::Agent>>(config, paths, cwd, options, action_handler, runner) -> anyhow::Result<Option<ConsoleOutcome>>` at `:187`, preceded by the too_many_lines allow + justification (`:183-186`).
- The pure logic already extracted: the fn imports ~20 helpers from `jackin_console::tui::run` (see the `use` block near `:30-37` in the file head) — the *orchestration* is what remains monolithic.
- The quit-confirm overlay renders via shared `render_confirm_dialog` (`run.rs:353` area).
- The repo's architecture docs place terminal/IO ownership exactly here (root crate owns terminal; `jackin-console` owns presentation) — decomposition must not move terminal ownership out of this crate.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin` then full | pass |
| E2E (pre-merge, per repo gate) | `cargo nextest run -p jackin --features e2e --profile docker-e2e` | pass (needs Docker; run at merge-readiness) |

## Scope

**In scope**: `crates/jackin/src/console/tui/run.rs` (and new sibling files under `crates/jackin/src/console/tui/` following the repo's coordinator+siblings file-split convention); `docs/.../codebase-map.mdx` if files are added.

**Out of scope**:
- `crates/jackin-console/` — no logic moves across the crate boundary.
- Any behavior/keybinding/rendering change.
- The effects/services layer (`effects.rs`, `services.rs`).

## Git workflow

Branch (operator confirm): `refactor/run-console-decomposition`. `git commit -s` + push; one commit per extracted step.

## Steps

### Step 1: Read and map

Read `run.rs:187-843` fully. Produce (in the PR description) the loop's phase map: startup (terminal enter, initial state), per-tick (event poll → dispatch by kind → effects), modal-precedence order, suspend/resume block (token generate), shutdown (terminal restore). Identify every `return`/`?`/`break` — each is a terminal-restore obligation.

### Step 2: Terminal lifetime guard first

Before extracting anything, ensure terminal enter/restore is RAII (a guard struct whose `Drop` restores raw mode/alt screen) if it isn't already — read how the fn currently restores on error (`rg -n 'disable_raw_mode|LeaveAlternateScreen|restore' crates/jackin/src/console/tui/run.rs`). If restore is manual per-path, wrap it in a guard as its own commit; if a guard exists, note it and move on. This is the step that de-risks all later ones.

**Verify**: `cargo nextest run -p jackin` → pass; manual check: `rg -n 'return|break' crates/jackin/src/console/tui/run.rs` — each site either inside the guard's scope or after restore.

### Step 3: Extract step functions

One commit each, keeping `run_console` as the thin loop:
- `handle_terminal_event(...)` — keyboard/mouse/resize dispatch (the big per-event match)
- `route_mouse(...)` — mouse-layer routing if separable from the above
- `drive_launch_prompt(...)` — launch-prompt flow arms
- `handle_quit_confirm(...)` — quit-confirm overlay state + keys
- `run_token_generate_suspended(...)` — the suspend/resume mint block, built ON the Step 2 guard (suspend = drop/re-enter or explicit guard methods)

Signatures take `&mut`-state + the values the arm actually uses — no grab-bag context struct unless >5 params repeat, in which case one small `LoopCtx<'_>` struct. The `too_many_lines` allow shrinks or disappears; keep the justification comment on whichever fn still needs it.

**Verify after each extraction**: `cargo nextest run -p jackin` → pass, zero expectation changes.

### Step 4: File split if natural

If the extracted fns exceed ~300 lines together, move them to a sibling (`run/steps.rs`-style per repo convention, no `mod.rs`); update codebase-map.

**Verify**: fmt/clippy/full nextest exit 0.

## Test plan

- Existing `crates/jackin/src/console/tui/tests.rs` (31K) + `state/`/`input/` tests are the net; zero expectation changes.
- New: unit tests for 2–3 extracted steps that were previously untestable — quit-confirm key handling (open → N → closed; open → Y → outcome), launch-prompt arm dispatch. Model on existing tests in `tests.rs`.
- Before merge: the repo's merge-readiness suite including the docker-e2e profile (CONTRIBUTING.md).

## Done criteria

- [x] fmt / clippy / `cargo nextest run` exit 0; docker-e2e profile at merge time
- [x] `run_console` body ≤ ~150 lines (loop + step calls)
- [x] Terminal restore is structurally guaranteed (guard) — cite the type in the PR
- [x] ≥2 new unit tests on previously-inline logic
- [x] `plans/README.md` updated

## STOP conditions

- Restore/suspend semantics resist the guard pattern (e.g. suspend must interleave with an external blocking child in a way Drop can't order) — report the exact sequence; this step may need operator design input.
- Any test expectation change appears — behavior moved, not just code.
- An extraction needs state from `jackin-console` internals not currently exposed — do not widen that crate's API without reporting.

## Maintenance notes

- New event kinds land as arms inside the named step fns, not back in the loop.
- Reviewer: scrutinize error paths in the suspend/resume block hardest — that is where terminals get wedged.
- Deferred: porting this loop onto `jackin_tui::runtime` TEA contracts (bigger architectural step; not needed for testability).
