# Plan 019: Phase 1 — adopt the slice/index panic-coverage lints on the pure crates (wave 1)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat e80d5cc0a..HEAD -- Cargo.toml clippy.toml crates/jackin-protocol/ crates/jackin-config/ crates/jackin-manifest/ crates/jackin-core/`
> If plan 011 landed, root `Cargo.toml` will differ — expected. On any other
> mismatch with the "Current state" excerpts, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M-L
- **Risk**: MED (every hit is a hand-rewrite from panicking indexing to checked access; wire decoders are on the hot path)
- **Depends on**: none hard; plans/code-health/011 recommended first (same lint-table area — coordinate to avoid merge conflicts)
- **Category**: tech-debt
- **Planned at**: commit `e80d5cc0a`, 2026-07-09

## Why this matters

The roadmap's panic-coverage section (codebase-health-enforcement.mdx, Phase 1, "Don't panic (slice/index coverage)") names `string_slice`, `indexing_slicing`, `get_unwrap`, `unwrap_in_result`, `panic_in_result_fn`, `unchecked_time_subtraction` as the highest-value agent-defect lints: byte-boundary slicing and out-of-bounds indexing are the panic class coding agents introduce most often, and the workspace's existing `unwrap/expect/panic` denials do not catch `&s[a..b]` or `arr[i]`. The audit measured the fix surface: a scoped clippy dry run on `jackin-protocol` alone emitted **75 warnings** for a 5-lint subset (grep proxies undercount ~5-10×), so workspace-wide adoption in one PR is not realistic. This plan is wave 1: adopt the full family as `deny` on the four pure foundational crates — `jackin-protocol`, `jackin-config`, `jackin-manifest`, `jackin-core` — where the payoff is highest (protocol decoders parse untrusted bytes; a panic there kills the capsule control plane) and the code is pure enough to rewrite safely. Later waves extend per crate.

## Current state

- Root `Cargo.toml` `[workspace.lints.clippy]` (lines 148-211 at the planning commit): none of the six lints configured. Per-crate lint overrides are possible via each crate's `Cargo.toml` `[lints]` table — but every crate currently has only `[lints] workspace = true`, and **workspace lint tables cannot be partially overridden per crate for additional lints without replacing the whole table**. The adoption mechanism therefore is: crate-level inner attributes in each crate's `lib.rs`, e.g. `#![deny(clippy::indexing_slicing, clippy::string_slice, …)]` placed after the existing `//!` header block. This keeps the workspace table untouched until the family goes workspace-wide.
- `clippy.toml` (19 lines): has `allow-unwrap-in-tests`, `allow-expect-in-tests`, `allow-panic-in-tests`, `allow-print-in-tests`. The roadmap (line 81) requires adding `allow-indexing-slicing-in-tests` and `allow-dbg-in-tests` **in the same PR** the slice lints land, so tests stay ergonomic.
- Verified production hit examples (read at the planning commit; your dry run will find more — protocol especially has direct `buf[0]`-style numeric indexing the grep missed):
  - `crates/jackin-protocol/src/attach.rs:705` — `p[2..].copy_from_slice(&cols.to_be_bytes());`
  - `crates/jackin-protocol/src/attach.rs:942` — `payload.extend_from_slice(&chunk[..n]);`
  - `crates/jackin-protocol/src/attach.rs:1098` — `let bytes = payload[1..].to_vec();`
  - `crates/jackin-protocol/src/attach.rs:1396,1412` — `&self.payload[self.pos..]` / `&self.payload[self.pos..end]` (decoder cursor slicing)
  - `crates/jackin-core/src/path_text.rs:13` — `let rest = &path[home.len()..];` (string slice at a prefix length — byte-boundary panic if `home` is not a char boundary of `path`)
  - `crates/jackin-manifest/src/repo_contract.rs:52` — `&without_digest[..colon]`
  - `crates/jackin-config/src/editor.rs:831-832,867-868` — `&path[i]`, `&path[..i]`, `table.entry(&path[0])`, `walk(entry, &path[1..])`
- Audit calibration: `cargo clippy -p jackin-protocol --all-targets` with `-W clippy::string_slice -W clippy::indexing_slicing -W clippy::let_underscore_future -W clippy::map_err_ignore -W clippy::unused_result_ok` → 75 warnings (some test-side; the in-tests valve removes those). Expect the four-crate total for the six target lints to land in the 60-150 range.
- Rewrite conventions for hits (the roadmap's own guidance: route through `.get()`/`.split_at_checked()`/iterator APIs):
  - Decoder cursor patterns (`&payload[pos..end]`) → `payload.get(pos..end).ok_or_else(|| <the function's existing error type/anyhow context>)?` — attach.rs functions already return `Result`, so `?` is available; match the error style already used in the same function.
  - Fixed-layout writes (`p[2..].copy_from_slice(…)`) where the buffer was just allocated with a known size → keep the layout but make the invariant explicit: prefer restructuring (build with `extend_from_slice`) over `#[expect]`; if the code provably cannot panic (local `let mut p = vec![0u8; 4];` two lines above), a narrow `#[expect(clippy::indexing_slicing, reason = "fixed 4-byte frame allocated above")]` is acceptable.
  - String prefix slicing (`&path[home.len()..]`) → `path.strip_prefix(home)` (this is the exact API for it; `path_text.rs:13` becomes `let Some(rest) = path.strip_prefix(home) else { … }` — read the surrounding function for the fallback branch).
  - `unchecked_time_subtraction` hits → `checked_sub`/`saturating_sub` or `Instant::duration_since` alternatives per site.
- Repo conventions: suppressions must be `#[expect(<lint>, reason = "…")]` at the narrowest scope; CI runs clippy with `-D warnings` all-targets all-features; tests live in sibling `tests.rs` and are exempt via the new valves.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Dry run, one crate | `cargo clippy -p <crate> --all-targets -- -W clippy::string_slice -W clippy::indexing_slicing -W clippy::get_unwrap -W clippy::unwrap_in_result -W clippy::panic_in_result_fn -W clippy::unchecked_time_subtraction 2>&1 \| grep -c '^warning:'` | a count (your worklist size) |
| Crate gate after adoption | `cargo clippy -p <crate> --all-targets -- -D warnings` | exit 0 |
| Crate tests | `cargo nextest run -p <crate>` | all pass |
| Protocol fuzz smoke (regression net) | `cd crates/jackin-term && cargo fuzz build damage_grid_process` plus, if plan 009 landed, the protocol fuzz targets | build OK / no crash |
| Full clippy as CI | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `clippy.toml` (add `allow-indexing-slicing-in-tests = true` and `allow-dbg-in-tests = true`)
- `crates/jackin-protocol/src/**`, `crates/jackin-config/src/**`, `crates/jackin-manifest/src/**`, `crates/jackin-core/src/**` — the `#![deny(…)]` inner attribute in each `lib.rs` plus the hit rewrites
- Those four crates' `tests.rs` files ONLY if a test hit survives the valves (should be none for indexing/slicing; `get_unwrap` etc. may fire — fix or expect)
- Roadmap Phase 1 status note (panic-coverage family: wave 1 shipped on 4 crates)
- `plans/code-health/README.md` status row

**Out of scope**:
- All other crates (term, capsule, runtime, console, …) — later waves; do NOT add the inner attribute anywhere else
- The workspace lint table in root `Cargo.toml` (only when the family goes workspace-wide)
- Behavior changes: every rewrite must preserve observable behavior; a slice-panic path becomes an error-return path only where the function already returns `Result` and the panic was reachable with malformed input (that is a bug fix — note each such site in the PR body)
- `map_err_ignore` (separately judged: documented-allow; see plans/code-health/README.md rejected section)

## Git workflow

- Branch off `main`: `chore/slice-index-lints-pure-crates`.
- One commit per crate (protocol, core, manifest, config) so review is per-surface; `-s`, push after each. PR to `main`; do not merge.

## Steps

### Step 1: Valves + dry-run counts

Add the two valves to `clippy.toml` (with a one-line comment: they land with the first slice-lint adoption per the roadmap). Then run the dry-run command for each of the four crates and record the counts in the PR description draft. If any single crate exceeds 120 warnings, STOP (wave needs splitting).

**Verify**: `cargo clippy -p jackin-manifest --all-targets -- -W clippy::indexing_slicing …` runs and prints a finite count for each crate.

### Step 2: Adopt on `jackin-manifest` (smallest first)

Append to `crates/jackin-manifest/src/lib.rs` after the `//!` header:

```rust
#![deny(
    clippy::string_slice,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn,
    clippy::unchecked_time_subtraction
)]
```

Fix every hit per the rewrite conventions above (e.g. `repo_contract.rs:52` → `without_digest.get(..colon)` with the function's error/None path, or `split_at_checked`). Narrow `#[expect(…, reason = "…")]` only where the index is provably in-bounds by local construction.

**Verify**: `cargo clippy -p jackin-manifest --all-targets -- -D warnings` → exit 0; `cargo nextest run -p jackin-manifest` → all pass.

### Step 3: Adopt on `jackin-config`, then `jackin-core`

Same attribute, same process. Known sites: `config/editor.rs:831-868` (TOML-path walking — indexing a `&[&str]` path; rewrite with `split_first()` / `.get(i)`; the recursion in `walk(entry, &path[1..])` becomes `let Some((first, rest)) = path.split_first() else { … }`), `core/path_text.rs:13` (→ `strip_prefix`).

**Verify**: per crate: clippy `-D warnings` exit 0 + `cargo nextest run -p <crate>` all pass.

### Step 4: Adopt on `jackin-protocol` (largest, wire-critical)

Same attribute. This is the crate where rewrites change panic-on-malformed-input into error returns — the actual point. Work function-by-function through `attach.rs` (decoder cursor sites at 1396/1412, payload sites at 942/1098, writer site at 705) and `control.rs` if it fires. Every decoder rewrite must keep the same `Ok` results for well-formed input (the existing `tests.rs` plus plan 009's truncation tests, if landed, are the oracle). Where a genuinely unreachable index remains (length checked on the previous line), prefer restructuring so the check and use are one expression; `#[expect]` is the last resort and needs the locality stated in its reason.

**Verify**: `cargo clippy -p jackin-protocol --all-targets -- -D warnings` → exit 0; `cargo nextest run -p jackin-protocol` → all pass; if plan 009's fuzz targets exist, `cargo fuzz run <protocol-target> -- -max_total_time=60` → no crash.

### Step 5: Workspace green + roadmap

Full workspace clippy + nextest (the four crates' rewrites must not break dependents). Update the roadmap Phase 1 panic-coverage subsection: wave 1 (4 pure crates) shipped, remaining crates listed as the next waves, valves in place.

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; `cargo nextest run --workspace --all-features --locked` → all pass; `cargo xtask roadmap audit` → pass; `cargo xtask ci --fast` → `ci gate OK`.

## Test plan

- No new test files required; the existing per-crate `tests.rs` suites are the behavior oracle and must stay green un-edited (edits allowed only where a test itself violates a lint and the valve does not cover it — expect zero).
- For each decoder site whose panic path became an error return, add one malformed-input test in the crate's existing `tests.rs` for that module proving the error (not panic) — pattern: existing decode-error tests in `attach.rs`'s tests. Minimum: 3 such tests in jackin-protocol.
- Count of new `#[expect]`s across all four crates must be ≤ 15; each carries a locality-stating reason.

## Done criteria

- [ ] Four `lib.rs` files carry the six-lint `#![deny(…)]` block
- [ ] `clippy.toml` has both new valves
- [ ] Per-crate and workspace clippy `-D warnings` exit 0
- [ ] ≥3 new malformed-input error tests in jackin-protocol; all suites green
- [ ] ≤15 new `#[expect]`s total, all reasoned (`rg -c 'expect\(\s*clippy::(string_slice|indexing_slicing)' crates/jackin-{protocol,config,manifest,core}/src` ≤ 15)
- [ ] Roadmap Phase 1 updated; `plans/code-health/README.md` row updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`

## STOP conditions

Stop and report back if:

- Any single crate's dry-run count exceeds 120 warnings.
- A rewrite would change behavior for well-formed input (a test asserts the old panic, or an error return changes a public API's contract visibly to dependents).
- `unwrap_in_result`/`panic_in_result_fn` fire on macro-generated code you cannot attribute (report the macro).
- You need more than 15 `#[expect]`s — the "provably in-bounds" bar is being lowered; report the sites instead.
- The `#![deny]` inner-attribute mechanism conflicts with something in the crate (e.g. an existing `#![…]` ordering issue you cannot resolve trivially).

## Maintenance notes

- Wave 2 candidates (in order): `jackin-term` (12 range-slices, pure), `jackin-env`, `jackin-diagnostics`, then the big ones (capsule 31, tui 28, runtime 24, xtask 22). Each wave is this plan re-instantiated per crate; when the last crate lands, move the six lints into the workspace table and delete the per-crate attributes in the same PR.
- The suppression-count ratchet (plan 011/017) tracks the `#[expect]`s this plan adds; keep them scarce.
- Reviewer should scrutinize: decoder rewrites in attach.rs byte-for-byte (wire compatibility), and that no `#[expect]` reason merely restates the lint name.
