# Plan 034: Adopt the numeric-correctness and easy-to-avoid clippy lint families

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md` — unless a reviewer dispatched you and told
> you they maintain the index.
>
> **Drift check (run first)**: `git diff --stat 0971da66d..HEAD -- Cargo.toml clippy.toml crates/jackin-usage/src/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S-M
- **Risk**: LOW (config flips backed by a measured zero-candidate census; the only fix wave is bounded to sign-loss casts)
- **Depends on**: none (**soft conflict with plan 011**: both edit the root `Cargo.toml` `[workspace.lints.clippy]` table. Either order works; if 011 landed first, its silent-failure entries are already in the table — append yours after them, matching style.)
- **Category**: tech-debt (lint strictness)
- **Planned at**: commit `0971da66d`, 2026-07-09

## Why this matters

The codebase-health roadmap's Phase 1 lint program (`docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx`, "Panic-coverage and silent-failure restriction lints", lines 73-81) names four lint families. Plans 011 (silent-failure + async) and 019 (slice/index) cover two; this plan closes the remaining two: **"Don't do incorrect things with numbers"** (`float_cmp_const`, `lossy_float_literal`, `cast_sign_loss`, `invalid_upcast_comparisons`, re-evaluating `float_cmp = "allow"`) and **"Easy to avoid"** (`rc_mutex`, `debug_assert_with_mut_call`, `expl_impl_clone_on_copy`, `infallible_try_from`, `iter_not_returning_iterator`). A fresh census (2026-07-09) measured the workspace: the easy-to-avoid family has **zero candidates**, the float lints have **zero grep-visible candidates**, and the only real workload is `cast_sign_loss` (signed→unsigned casts concentrated in `jackin-usage`). Adopting deny now is nearly free and permanently blocks the whole class — the roadmap's "deny by default; an agent does not need a grace period" calibration.

## Current state

- Root `Cargo.toml` lint tables: `[workspace.lints.rust]` at line 118, `[workspace.lints.clippy]` at line 148. Group levels (lines 149-151):

```toml
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
cargo = { level = "warn", priority = -1 }
```

  CI runs `cargo clippy --timings --workspace --all-targets --all-features --locked -- -D warnings` (`.github/workflows/ci.yml:507`), so **every pedantic lint not explicitly allowed is already a hard CI error**. Local builds don't bake `-D warnings` (deliberate; see `crates/AGENTS.md`).
- The five target allows in the clippy table (exact lines):

```toml
199  float_cmp = "allow"
206  cast_possible_truncation = "allow"
207  cast_possible_wrap = "allow"
208  cast_sign_loss = "allow"
209  cast_precision_loss = "allow"
```

  Entry style is flat `key = "value"` (no `{ level }` wrapper except the group headers).
- `clippy.toml` (20 lines): test valves `allow-expect-in-tests`/`allow-panic-in-tests`/`allow-print-in-tests`/`allow-unwrap-in-tests` (lines 1-4), complexity thresholds (10-13), `disallowed-methods` (14-19). **None of this plan's lints has an `allow-*-in-tests` valve** — clippy does not offer one for them; test-only exceptions need inline `#[expect(clippy::<lint>, reason = "…")]`.
- Census results (measured at `0971da66d`; re-verify via the dry run in Step 1):
  - `rc_mutex`: 0 candidates (zero `Rc<Mutex<…>>` in the workspace; the 9 `Rc<RefCell<…>>` sites in jackin-console/jackin-console-oppicker do NOT trigger it).
  - `debug_assert_with_mut_call`: 0 (all 23 `debug_assert*` sites call only `&self`/pure predicates — e.g. `crates/jackin-instance/src/naming.rs:55-56`, `crates/jackin-capsule/src/tui/layout.rs:593`). **Nursery lint** — see STOP condition 4.
  - `expl_impl_clone_on_copy`, `iter_not_returning_iterator`, `invalid_upcast_comparisons`: 0 candidates each, and all three are **pedantic** — already promoted to errors by CI today; adding explicit `deny` entries only makes local builds match CI and the intent auditable.
  - `infallible_try_from`: 0 (`Error = Infallible` appears nowhere).
  - `float_cmp` / `float_cmp_const` / `lossy_float_literal`: 0 grep-visible candidates. The codebase consistently uses epsilon comparisons (`crates/jackin-usage/src/usage/format.rs:206` `if value.fract().abs() < f64::EPSILON {`; `crates/jackin-capsule/src/daemon/resource_metrics.rs:58`). Residual risk: variable-to-variable float `==` is invisible to grep — the Step 1 dry run is the oracle.
  - `cast_sign_loss`: real candidates, concentrated in `jackin-usage` — e.g. `crates/jackin-usage/src/usage/zai.rs:214-216`:

```rust
limit
    .current_value
    .map(|value| compact_count(value.max(0) as u64)),
```

    Same `value.max(0) as u64` shape in `minimax.rs:261,276,306,309`, `kimi.rs:231,232`, plus f64→u8 percent casts (`.round().clamp(0.0, 100.0) as u8`) in `claude.rs`/`codex.rs`/`grok.rs`/`amp.rs`. Two pre-existing local suppressions mark other hotspots: `crates/jackin-launch-tui/src/tui/components/footer.rs:43` (`#[allow(clippy::cast_precision_loss)]`) and `crates/jackin-console/src/tui/layout.rs:98` (`#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]`).
- Scale context: 351 unsigned-target `as` casts workspace-wide, but most are unsigned→unsigned (truncation surface, NOT sign-loss). `cast_possible_truncation`/`cast_possible_wrap`/`cast_precision_loss` stay `allow` — the roadmap (line 78) treats them as an optional later tier.
- Suppression style (repo rule, `crates/AGENTS.md`): prefer fixing; survivors get narrow `#[expect(clippy::<lint>, reason = "…")]`, never blanket `#[allow]`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Dry run / lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Format | `cargo fmt` | exit 0 |
| Usage-crate tests | `cargo nextest run -p jackin-usage` | all pass |
| Merge-readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope** (the only files you should modify):
- `Cargo.toml` (root — lint table entries only)
- `crates/jackin-usage/src/**` (sign-loss cast fixes)
- Any file where the Step 1 dry run surfaces a hit for the newly-denied lints (fix or narrow `#[expect(…, reason)]`)
- `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` (one status note, Step 5)

**Out of scope** (do NOT touch):
- `cast_possible_truncation` / `cast_possible_wrap` / `cast_precision_loss` — stay `allow` (optional later tier; flipping them is a different, much larger sweep).
- `clippy.toml` — no keys needed for these lints.
- The two pre-existing local cast suppressions (`footer.rs:43`, `layout.rs:98`) — they cover lints that stay allowed (`cast_precision_loss`, `cast_possible_truncation`); leave them. The `cast_sign_loss` half of `layout.rs:98` becomes load-bearing when you flip the workspace allow — verify it still suppresses (it does; local allow overrides workspace deny) and leave it as-is unless the dry run says otherwise.
- Plan 011's silent-failure lint entries (if present) and plan 019's slice/index entries — different families, different plans.

## Git workflow

- Branch: current active branch if the operator designates one; otherwise propose `chore/numeric-easy-lint-families` and wait for confirmation (never commit `main`).
- Conventional Commits, signed, push after every commit:
  `git commit -s -m "build(lints): deny numeric-correctness and easy-to-avoid clippy families"` → `git push`.

## Steps

### Step 1: Measured dry run (the roadmap's mandatory gate before any strict adoption)

Add the new entries to `[workspace.lints.clippy]` (after line 211, keeping alphabetical-ish grouping with the cast block; match the flat `key = "value"` style):

```toml
float_cmp_const = "deny"
lossy_float_literal = "deny"
invalid_upcast_comparisons = "deny"
expl_impl_clone_on_copy = "deny"
iter_not_returning_iterator = "deny"
infallible_try_from = "deny"
rc_mutex = "deny"
debug_assert_with_mut_call = "deny"
```

Do **not** touch lines 199/208 yet. Run the dry run and count diagnostics.

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0 expected (census says zero candidates). If it fails with >10 diagnostics across these lints, STOP (condition 2). For ≤10: fix each (they'll be genuine) or add narrow `#[expect(…, reason = "…")]`.

### Step 2: Flip `float_cmp` allow → deny

Change line 199 `float_cmp = "allow"` to `float_cmp = "deny"`. Dry-run again — this catches variable-to-variable float equality invisible to grep.

**Verify**: same clippy command → exit 0, or a handful of hits. For each hit: rewrite as an epsilon comparison following the repo idiom (`(a - b).abs() < f64::EPSILON` — exemplar `crates/jackin-usage/src/usage/format.rs:206`) when the comparison is a tolerance check, or `#[expect(clippy::float_cmp, reason = "…")]` when exact bit-equality is genuinely intended. >10 hits → STOP (condition 2).

### Step 3: Flip `cast_sign_loss` allow → deny and fix the wave

Change line 208 to `cast_sign_loss = "deny"`. Dry-run. Expected hits: the jackin-usage cluster plus scattered singles. Fix pattern by shape:

- `value.max(0) as u64` (i64 source, already clamped) → `u64::try_from(value.max(0)).unwrap_or(0)` — or, better where the type allows, `value.try_into().unwrap_or(0u64)`; do NOT introduce `unwrap()`/`expect()` (workspace-denied). If the call site is hot-loop-free (all of these are formatting paths), `try_from(...).unwrap_or(0)` is the house-compatible shape.
- `f.round().clamp(0.0, 100.0) as u8` (f64→u8; sign_loss fires on the float→unsigned cast) → keep the clamp and add a one-line `#[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 above")]` at the smallest scope, OR restructure to `as u8` on a value provably ≥0 — prefer the `#[expect]` here; the clamp already IS the invariant and restating it in code adds nothing.
- Anything else: judge locally; fix if trivial, narrow `#[expect]` with a real reason otherwise.

**Verify**: clippy command → exit 0. Then `cargo nextest run -p jackin-usage` → all pass (the formatting funcs are test-covered; a wrong conversion breaks compact-count tests).

### Step 4: Re-run the suppression gate interaction

If plan 011's suppression-budget gate (`cargo xtask lint suppressions` or similar) has landed, regenerate/adjust its budget file for any new `#[expect]`s you added (011's ratchet counts per-lint suppressions). If 011 has not landed, skip — no budget file exists.

**Verify**: `cargo xtask lint --strict` → exit 0 (runs whatever gates exist today).

### Step 5: Roadmap status note

In `codebase-health-enforcement.mdx`, in the Phase 1 restriction-lints subsection (lines 73-81), append one sentence to the "Don't do incorrect things with numbers" bullet recording adoption: measured census (easy-to-avoid family zero candidates; float family zero; sign-loss fixed in jackin-usage), date, and that the truncation/precision/wrap trio remains the deferred optional tier. Then run the docs gates.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → both exit 0.

### Step 6: Full gates

**Verify**: `cargo fmt && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo xtask ci --fast` → all exit 0.

## Test plan

No new test files — this is lint-config + mechanical fixes. The regression net is: (a) the clippy gate itself (the lints are now permanent CI oracles); (b) `cargo nextest run -p jackin-usage` covering the compact-count/percent formatting paths the cast fixes touch. If any Step 3 fix changes an observable formatted value, the existing usage tests must catch it — a test failure there means the fix changed semantics: STOP condition 3.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `grep -n "float_cmp\|cast_sign_loss\|rc_mutex\|infallible_try_from\|float_cmp_const\|lossy_float_literal\|invalid_upcast_comparisons\|expl_impl_clone_on_copy\|iter_not_returning_iterator\|debug_assert_with_mut_call" Cargo.toml` shows: `float_cmp = "deny"`, `cast_sign_loss = "deny"`, and the 8 new entries all `"deny"`; `cast_possible_truncation`/`_wrap`/`cast_precision_loss` still `"allow"`
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0
- [ ] `grep -rn "\.max(0) as u64" crates/jackin-usage/src/` returns no matches
- [ ] Every new `#[expect(clippy::` added by this plan carries `reason =` (`grep -rn "expect(clippy::cast_sign_loss\|expect(clippy::float_cmp" crates/ | grep -v reason` → empty)
- [ ] `cargo nextest run -p jackin-usage` exits 0
- [ ] `cargo xtask ci --fast` exits 0
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

1. The `Cargo.toml` lint-table excerpts don't match (plan 011 or 019 may have restructured the table — reconcile entries manually per their landed state, but STOP if the group-level lines 149-151 changed meaning).
2. Any single lint from Step 1/2 produces **more than 10 diagnostics** — the census missed a cluster; report the lint and count so the wave can be re-scoped (the roadmap prefers a measured re-plan over a 50-`#[expect]` smear).
3. A Step 3 cast fix changes a formatted value under test (usage tests fail) — the existing behavior may itself be the bug; report instead of "fixing" the test.
4. `debug_assert_with_mut_call` (nursery) is rejected by the pinned clippy (1.96.1) or produces false positives — drop that single entry, note it in the roadmap sentence (Step 5), and continue; do not fight nursery instability.

## Maintenance notes

- After this plan, all four roadmap Phase 1 lint families are adopted (011: silent-failure + async; 019: slice/index on pure crates; 034: numeric + easy-to-avoid). The remaining numeric work is the optional truncation/precision/wrap tier — a deliberate later decision, recorded in the roadmap line 78.
- Plan 017's ratchet engine should pick up any `#[expect(clippy::cast_sign_loss)]` count as a suppression-budget family once both land.
- Reviewer scrutiny: the `try_from(...).unwrap_or(0)` conversions in jackin-usage — confirm `0` is the right saturation for negative provider-reported values (it matches the previous `.max(0)` semantics exactly).
- Future agents adding float comparisons will now get a hard error instead of silently-wrong `==` — the intended effect; the epsilon idiom exemplar is `format.rs:206`.
