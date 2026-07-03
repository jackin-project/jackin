# Plan 018: Slim jackin-tui's lib.rs — extract the ANSI banner/rain module to its proper homes

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-tui/src/lib.rs crates/jackin-tui/src/animation.rs crates/jackin-tui/src/components/brand_header.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: LOW (pure relocation; re-export discipline is the only trap)
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

`crates/jackin-tui/src/lib.rs` is 531 lines — an inline `pub mod ansi` (lines 251–490) carrying the raw-ANSI brand banner, `version_splash`, and a full xorshift-driven digital-rain `help_banner` generator lives in the crate root next to palette tokens and core types. The rain's age→color ramp duplicates the ramp `animation.rs` owns for the launch rain, giving one visual effect two homes that can drift. The repo just finished a Phase-2 file-split program with empty exception ledgers; the shared TUI crate's own root shouldn't be the counter-example. Pure relocation, no behavior change.

## Current state

`crates/jackin-tui/src/lib.rs` (verified):

- `:251` — `pub mod ansi {` inline module (~240 lines), containing:
  - `:268` — `pub const BRAND_BANNER: &str = "\n  \x1b[1m\x1b[48;2;0;255;65m…jackin❯…"` (raw-ANSI brand pill)
  - `:276` — `pub fn version_splash(version: &str) -> String`
  - `:300` — `pub fn help_banner(width: u16) -> String` — per-cell digital-rain ASCII art; carries its own too-many-lines-style allow justification mentioning "character-styling + xorshift-driven effects" (`:296`); inline `xorshift` closure at `:334,:363,:387`; an age→rgb ramp the code notes mirrors the launch rain's `age_to_color`
  - `:410+` — doc note pointing narrow terminals at `BRAND_BANNER`
  - `:447` — `pub fn rgb_fg_dyn(rgb: Rgb) -> String` and siblings (raw ANSI emitters)
- `animation.rs` (16.6K) — the crate's rain/effects module owning the launch rain ramp (`age_to_color`).
- `components/brand_header.rs` — the widget-form brand rendering (`BrandHeader`, `brand_header_line`).
- Consumers of `ansi::*` are CLI-output paths (help/version output in the root crate) — enumerate: `rg -n 'jackin_tui::ansi|ansi::(help_banner|version_splash|BRAND_BANNER|rgb_fg)' crates/`.

Repo rules that bind: module moves update `docs/.../reference/getting-oriented/codebase-map.mdx` same PR; comments carry non-obvious WHY only.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-tui` then full | pass |
| Lookbook | `--check` | exit 0, zero diffs |

## Scope

**In scope**:
- `crates/jackin-tui/src/lib.rs` — the `ansi` module body moves out; `lib.rs` keeps: palette token consts, `Rgb`, `ModalOutcome`, `PointerShape`-class types, module declarations, re-exports
- New `crates/jackin-tui/src/ansi.rs` — the module lands here verbatim first (smallest correct move: `pub mod ansi;`)
- `crates/jackin-tui/src/animation.rs` — second commit: `help_banner`'s rain reuses `animation.rs`'s ramp (`age_to_color`) instead of its private copy IF the two ramps are value-identical (compare them; if they differ numerically they are two designs — leave both, note it, done)
- `docs/.../codebase-map.mdx` — module entry

**Out of scope**:
- `ansi_text.rs` (different module — SGR parsing; untouched)
- Behavior of banner/splash/rain output — byte-identical (help/version output may be snapshot-tested in the root crate; those tests must not change)
- Moving `version_splash`/`BRAND_BANNER` into `brand_header.rs` — considered and rejected: they are raw-ANSI CLI strings, not ratatui widgets; keep the CLI/widget boundary clean in `ansi.rs`

## Git workflow

Branch (operator confirm): `refactor/jackin-tui-ansi-module`. `git commit -s` + push; two commits (move, then ramp dedup).

## Steps

### Step 1: Mechanical move

Cut `lib.rs:251-490` into `src/ansi.rs`; declare `pub mod ansi;` in `lib.rs`. Fix `use` paths inside the moved code (`crate::Rgb` etc. — the module already namespace-qualifies; adjust as the compiler demands). No signature or re-export changes: anything previously reachable as `jackin_tui::ansi::X` must remain reachable identically.

**Verify**: `cargo nextest run` (workspace — root-crate help/version tests included) → pass, zero changes; `rg -n 'pub mod ansi \{' crates/jackin-tui/src/lib.rs` → 0; `wc -l crates/jackin-tui/src/lib.rs` → ≈290.

### Step 2: Ramp single-source (conditional)

Compare `help_banner`'s age→rgb ramp values with `animation.rs`'s `age_to_color`. If value-identical: expose the ramp from `animation.rs` and call it from `ansi.rs`; delete the private copy. If they differ numerically: leave both, add one WHY comment in `ansi.rs` naming the difference ("ramp intentionally differs from launch rain: <values>") so the next audit doesn't re-flag it.

**Verify**: `cargo nextest run` → pass; if unified, output-affecting tests unchanged (byte-identical ramp).

### Step 3: Map + sweep

Update codebase-map for the new module. Full sweep: fmt, clippy, nextest, lookbook `--check`.

## Test plan

- No new tests — relocation. Root-crate help/version output tests + lookbook zero-diff are the proof.
- If NO test currently pins `help_banner`/`version_splash` output (`rg -n 'help_banner|version_splash' crates/ --glob '*test*'`), add one smoke test in `jackin-tui` asserting `version_splash("0.0.0")` contains the version and `BRAND_BANNER` contains `jackin` — cheap drift canaries.

## Done criteria

- [ ] fmt / clippy / `cargo nextest run` exit 0; lookbook `--check` zero diffs
- [ ] `lib.rs` ≈290 lines; contains no `fn` bodies over ~10 lines
- [ ] `jackin_tui::ansi::*` paths unchanged for consumers (`rg 'jackin_tui::ansi' crates/` all compile)
- [ ] Codebase-map updated
- [ ] `plans/README.md` updated

## STOP conditions

- The `ansi` module references `lib.rs`-private items that would need `pub(crate)` widening beyond 2–3 items — report the list.
- Root-crate output snapshots change in Step 2 — the ramps were not identical; revert to the leave-both branch.

## Maintenance notes

- Reviewer: check `lib.rs` retains ONLY tokens/types/re-exports; check the allowlist doc comment near `rgb_fg`-style emitters (`lib.rs:444` today) moved intact.
- Deferred: whether raw-ANSI CLI banners belong in `jackin-tui` at all vs a CLI-output crate — bigger boundary question, out of scope.
