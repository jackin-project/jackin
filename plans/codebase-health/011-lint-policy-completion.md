# Plan 011: Lint policy completion — `allow_attributes`, full measured census, restriction decisions, blocked-surface inventory

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- Cargo.toml clippy.toml crates/jackin-core/src/lib.rs crates/jackin-config/src/lib.rs`
> Mismatch with "Current state" = STOP. Land plan 010 first (trustworthy measurement).

## Status

- **Priority**: P2
- **Effort**: M (census) + rolling L (restriction adoption)
- **Risk**: MED
- **Depends on**: plans/codebase-health/010-suppression-parser-syn.md
- **Category**: tech-debt (lint policy)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Rust-enforcement items 1–4 remain partially open on the policy side: `allow_attributes = "deny"` is not set (only `allow_attributes_without_reason`), so reasoned broad `#[allow]` never self-retires the way `#[expect]` does; the lint census covers ~7 lints with count+date while the roadmap explicitly names `format_push_string`, `option_option`, `struct_field_names` (all currently bare `allow`, no evidence) and requires a revisit trigger on every entry ("The census must include … and every other current or newly introduced allow"); the slice/index restriction family is denied in only 4 pure crates with no per-crate adopt/defer record; `map_err_ignore` and the lossy-cast tier have no recorded decision; the whole-category restriction-allowlist pilot was never attempted; and `clippy.toml` has a reasoned methods inventory but no types/macros inventory nor the recorded Mutex/Rc-in-async/ring-buffer/primitive-payload evaluations.

## Current state

- `Cargo.toml:177` — `allow_attributes_without_reason = "deny"`; no `allow_attributes` key anywhere.
- Census with evidence (count + 2026-07-12 date): `needless_pass_by_value` (`Cargo.toml:201-202`), `large_futures` (`:205-206`), `assigning_clones` (`:220-221`), `match_same_arms` (`:222-223`), `drop_non_drop` (`:224-225`); `unused_self` (`:231`) and `unused_async` (`:237`) carry "0 hits" notes. Bare allows with NO evidence: `format_push_string` (`:226`), `option_option` (`:232`), `struct_field_names` (`:239`), plus `implicit_hasher`, `similar_names`, `items_after_statements`, `unnecessary_wraps`, `verbose_bit_mask`, `unreadable_literal`, `unnecessary_debug_formatting`, `must_use_candidate`, `module_name_repetitions`, `future_not_send`, `cast_possible_truncation/_wrap/_precision_loss`, `missing_errors_doc`, `missing_panics_doc` (`:218-258`). No entry has a revisit trigger.
- Restriction family denied only in: `crates/jackin-core/src/lib.rs:9-16`, `crates/jackin-config/src/lib.rs:6-13`, `crates/jackin-manifest/src/lib.rs:8-13`, `crates/jackin-protocol/src/lib.rs:8-13` (`string_slice`, `indexing_slicing`, `get_unwrap`, `unwrap_in_result`, `panic_in_result_fn`, `unchecked_time_subtraction`). `clippy.toml:8-9` marks the wider rollout pending. `map_err_ignore`: zero occurrences. `clippy::restriction` category posture: zero occurrences. `assertions_on_result_states = "deny"` already at `Cargo.toml:182`; `cast_sign_loss = "deny"` at `:247`.
- `clippy.toml:22-27` — 4 `disallowed-methods`, each reasoned with replacement; no `disallowed-types`/`disallowed-macros` sections. Indirect protections exist: `await_holding_lock`/`await_holding_refcell_ref` denied (`Cargo.toml:183-184`); print/dbg/todo/unimplemented denied via lint levels (`:178,186-187,192-193`).
- `unfulfilled_lint_expectations = 11` recorded at `code-health-baseline.toml:212` — conversion to `expect` needs per-site verification.
- Where the census should live: the workspace `Cargo.toml` comments are the current form; the roadmap wants "a justified policy, not a mechanically low count" — a generated inventory is the durable form (plan 027's providers can render it; here a checked table suffices).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Measure a lint | `cargo clippy --workspace --all-targets --all-features --locked -- -W clippy::<lint> 2>&1 \| grep -c "warning: .*<lint>"` (or `-A` others; see note) | count |
| Full clippy | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Tests | `cargo nextest run` (targeted crates) | pass |
| Gates | `cargo xtask ci --fast` | exit 0 |

Note on measuring: to count hits for an allowed lint without failing, run clippy with that single lint at `warn` (e.g. append `-W clippy::format_push_string`) and count distinct primary warnings. Do this per lint; record count + today's date.

## Scope

**In scope**: `Cargo.toml` `[workspace.lints]` tables + comments, `clippy.toml`, per-crate `lib.rs` lint headers where restriction adoption lands, narrow `#[expect]` conversions those changes force, one new census/decision document (put it in `docs/content/docs/reference/` developer reference — e.g. a `lint-policy.mdx` under the appropriate reference group — since prose policy belongs in contributor docs; update the matching `meta.json`).

**Out of scope**: the suppression parser (010); Dylint (012); fixing every site a newly-warned lint flags — adoption decisions may be "defer with trigger", which is a valid census outcome.

## Git workflow

Branch `chore/lint-policy-census`; Conventional Commits; `git commit -s`; push per commit. Commit the census document separately from any lint-level flips.

## Steps

### Step 1: Full census

For EVERY `allow` entry in `[workspace.lints.clippy]` (enumerate from `Cargo.toml:201-258` — list them all, not a subset): measure current hit count (method above), record `(lint, level-decision, count, date, rationale, revisit trigger)`. Revisit triggers are concrete: "re-measure when count source changes >20%", "revisit at next toolchain major", or "revisit after <named refactor plan> lands". Produce the census table in the new reference doc; compress `Cargo.toml` comments to point at it.

**Verify**: doc exists; every `allow` in the workspace table appears in it (script check: extract lint names from Cargo.toml, grep each in the doc — all found).

### Step 2: `allow_attributes = "deny"`

Add it beside `allow_attributes_without_reason` in `[workspace.lints.clippy]`. Sweep surviving reasoned `#[allow(...)]` sites: convert to `#[expect(...)]` where the lint verifiably fires (build proves it — `unfulfilled_lint_expectations` is deny-by-default under `-D warnings`, so a wrong conversion reddens CI); keep `allow` ONLY for the roadmap's legitimate carve-outs (cfg/target-dependent firing, test-only, generated code), each converted to a narrow site with reason naming the instability + removal trigger, and enumerate those carve-outs in the census doc.

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; `cargo xtask lint suppressions` → exit 0; carve-out list in doc matches remaining `#[allow]` census (plan-010 parser output).

### Step 3: Restriction decisions

(a) Slice/index family: per-crate actionability census (count non-test indexing/slicing sites per crate: run clippy per crate with the family at warn); record adopt/defer per crate in the doc; adopt where the roadmap's bar ("production signal is actionable") is met — at minimum evaluate `jackin-term` (hot parsing loops, panics = render crash) and `jackin-env`. Adoption = deny at crate root + fix or narrowly expect each site + keep test escape valves (`clippy.toml:4-10` knobs).
(b) `map_err_ignore`: fresh census (warn-run count), explicit adopt/defer decision recorded.
(c) Lossy-cast tier (`cast_possible_truncation`/`_wrap`/`_precision_loss`): fresh census, decision recorded (the existing `cast_sign_loss` deny is precedent).
(d) Category pilot: on ONE pure crate (`jackin-protocol` is smallest), set `#![warn(clippy::restriction)]` locally, triage output, and record the verdict on whole-category posture (adopt allowlist-style / reject with reasons) in the doc. Revert the experimental attribute unless adopted.

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; decisions present in doc for (a)–(d).

### Step 4: Blocked-surface inventory

In `clippy.toml`: add `disallowed-types` / `disallowed-macros` sections where a real ban is warranted, each with reason + canonical replacement; for surfaces deliberately NOT banned (std Mutex/RwLock on async paths — covered by `await_holding_lock`; `Rc<RefCell<_>>` in async — `await_holding_refcell_ref` + Dylint; ring buffers; bare primitive payloads), record the evaluation verdict in the census doc so the roadmap's "evaluate … rather than banning indiscriminately" is a written decision, not silence.

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; `cargo xtask ci --fast` → exit 0; doc contains the four evaluations.

## Test plan

No new unit tests; the gates are the verification. The census doc is checked by the docs gates (`cargo xtask ci --fast` includes docs lane; run `cargo xtask docs repo-links` + `bun` link checks if the doc adds links).

## Done criteria

- [ ] `allow_attributes = "deny"` set; workspace builds clean
- [ ] Census doc: every workspace `allow` has count/date/rationale/revisit trigger; carve-outs enumerated
- [ ] Slice/index per-crate decisions + at least the evaluated crates recorded; map_err_ignore + lossy-cast + category-pilot verdicts recorded
- [ ] `clippy.toml` types/macros inventory or recorded not-banned evaluations
- [ ] `cargo xtask ci --fast` exits 0; status row updated

## STOP conditions

- Step 2 conversion surfaces >30 unfulfilled-expectation sites (means many "reasoned allows" never fired — that's a finding in itself; report the list before mass-deleting).
- A restriction adoption requires >50 site fixes in one crate — defer decision to operator with the measured count.
- Census doc placement conflicts with docs-audience rules (it's contributor content — must live under the Internals sidebar group; if unsure where, STOP and propose).

## Maintenance notes

- New `allow` entries require a census row in the same PR (reviewer rule).
- Plan 027 can later generate the count column mechanically; keep the table format machine-friendly.
