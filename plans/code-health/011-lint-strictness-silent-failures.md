# Plan 011: Phase 1 strictness wave — silent-failure lints, rustdoc gates, doc tests in PR CI, suppression reason-gate

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat 47dd5fca0..HEAD -- Cargo.toml clippy.toml crates/jackin-xtask/ .github/workflows/ci.yml .github/workflows/hygiene.yml`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (lint flips can redden CI; every fix site is enumerated below to keep it bounded)
- **Depends on**: plans/code-health/010-health-dashboard-and-baselines.md (baseline numbers for the reason-gate)
- **Category**: tech-debt
- **Planned at**: commit `47dd5fca0`, 2026-07-09

## Why this matters

The roadmap's Phase 1 ("codebase-health-enforcement", lines 46-115) targets deny-by-default linting because the primary contributor is an autonomous agent: an agent can read a lint message and fix it in the same turn, so `warn` buys nothing. The audit measured the cheap, high-value subset: the **silent-failure family** (`let_underscore_future`, `let_underscore_must_use`, `unused_result_ok`, `assertions_on_result_states`) has only ~11 production hits total; the **async-deadlock pair** (`await_holding_lock`, `await_holding_refcell_ref`) has zero measured hits (the audit refuted the roadmap's "non-Send guards held across .await in places" claim — adopting it is a free regression guard); **rustdoc lints and `missing_debug_implementations = deny`** are pure config with a small known backlog. Meanwhile all 182 `#[expect]`s carry `reason =` but 235 of 369 `#[allow]`s are bare, and nothing blocks new bare suppressions — this plan adds the interim reason-gate the roadmap asks for (line 66). The expensive families (slice/indexing with 4-figure fix surface, `missing_docs`, `map_err_ignore` with 33 intentional-looking sites, full allow→expect conversion) are explicitly out of scope and recorded in the index for a later wave.

## Current state

- Root `Cargo.toml` `[workspace.lints.rust]` (lines 118-147): `missing_debug_implementations = "warn"` (line 145), `let_underscore_drop = "deny"` (line 144). No `[workspace.lints.rustdoc]` table exists anywhere in the workspace.
- Root `Cargo.toml` `[workspace.lints.clippy]` (lines 148-211): denies `unwrap_used`/`expect_used`/`panic`/`todo`/`unimplemented`/`print_stdout`/`print_stderr`/`dbg_macro`; none of `let_underscore_future`, `let_underscore_must_use`, `unused_result_ok`, `assertions_on_result_states`, `map_err_ignore`, `await_holding_lock`, `await_holding_refcell_ref`, `allow_attributes`, `allow_attributes_without_reason` appear.
- CI promotes warns: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` (ci.yml `clippy` job; same line in `crates/jackin-xtask/src/ci.rs:106-117`). **Therefore adding a lint at `warn` still fails CI on any hit — treat every adoption below as deny-equivalent and fix or `#[expect]` all hits in the same commit.**
- `clippy.toml` lines 1-4: `allow-expect-in-tests`, `allow-panic-in-tests`, `allow-print-in-tests`, `allow-unwrap-in-tests` — all `true`. (No new valves needed for this wave's lints; the slice-family valves land with that family, later.)
- Measured production fix sites (audit 2026-07-09; re-verify each still matches before editing):
  - `.ok();` discarding a `Result` (fires `unused_result_ok`): `crates/jackin-tui/src/prune_output.rs:119`, `crates/jackin-runtime/src/runtime/prune_output.rs:119` (byte-identical twin files), `crates/jackin-runtime/src/runtime/cleanup.rs:644`, `:757`, `:896`.
  - `let _ =` on must-use values (fires `let_underscore_must_use`): `crates/jackin-capsule/src/firewall.rs:215` (`writeln!` result), `crates/jackin-core/src/standalone_dialog.rs:41` (`OnceCell::set`), `crates/jackin-console/src/tui/terminal.rs:53` (nix `tcflush`), `crates/jackin/src/console/services.rs:313`. Expect a handful more when the lint runs — the audit's grep found 27 `let _ =` total, most of them non-must-use.
  - `let_underscore_future` / `assertions_on_result_states` / `await_holding_lock` / `await_holding_refcell_ref`: **zero expected hits** (audit verified: no file in jackin-capsule contains both `.lock()` and `.await`; the one lock-across-await in `crates/jackin-runtime/src/runtime/shared_runner.rs:49-58` is a tokio `Mutex`, which the lint does not flag).
- `missing_debug_implementations` gaps are already covered by ~11 `#[expect(missing_debug_implementations, …)]` sites (e.g. `crates/jackin-capsule/src/daemon.rs:159`, `crates/jackin-runtime/src/runtime/launch.rs:78`, `crates/jackin-instance/src/lib.rs:408`) — flipping warn→deny should require **no new fixes**, it only makes local builds fail like CI already does.
- Doc tests run only in the scheduled lane: `.github/workflows/hygiene.yml:87` `cargo test --doc --workspace --locked`; not in ci.yml, not in `ci.rs` `build_steps` (first-wave finding DX-doctests-pr). `TESTING.md:56` documents the command.
- Snapshot state: `insta = "=1.48.0"` workspace dev-dep; 18 `.snap` files (12 jackin-capsule, 6 jackin-console); no `*.pending-snap` check anywhere.
- Suppression baseline (from plan 010's `code-health-baseline.toml`; audit numbers: 369 allow / 235 bare, 182 expect / 0 bare): the reason-gate ratchets **bare `#[allow]` count per crate**.
- Repo conventions: every suppression you add must be `#[expect(<lint>, reason = "…")]` at the narrowest scope (crates/AGENTS.md "Suppression discipline"); Conventional Commits signed with `-s`, push after every commit; workflows install tools only via mise (`.github/CLAUDE.md` hard rule).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Full clippy as CI runs it | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| One-crate clippy | `cargo clippy -p <crate> --all-targets -- -D warnings` | exit 0 |
| Doc build for rustdoc lints | `cargo doc --workspace --no-deps --locked` | exit 0, no warnings |
| Doc tests | `cargo test --doc --workspace --locked` | all pass |
| Tests | `cargo nextest run --workspace --all-features --locked` | all pass |
| Workflow lint | `actionlint .github/workflows/ci.yml .github/workflows/hygiene.yml` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- Root `Cargo.toml` (lint tables only)
- The specific fix sites listed above plus whatever the new lints flag (expect a dozen-odd files; every edit must be a minimal fix or a reasoned `#[expect]`)
- `crates/jackin-xtask/src/suppressions.rs` (create) + `crates/jackin-xtask/src/suppressions/tests.rs` (create) + `main.rs` + `crates/jackin-xtask/README.md`
- `suppression-budget.toml` (create, repo root)
- `.github/workflows/ci.yml` (doc-test step + pending-snap check), `.github/workflows/hygiene.yml` (remove doc-test once promoted — or keep; see Step 5), `crates/jackin-xtask/src/ci.rs` (doc-test step)
- Roadmap page Phase 1 status notes

**Out of scope** (recorded for later waves — do not start them):
- `string_slice`/`indexing_slicing`/`get_unwrap`/`unwrap_in_result`/`panic_in_result_fn` family (4-figure fix surface; needs its own dry-run-driven wave plus `allow-indexing-slicing-in-tests` valve)
- `map_err_ignore` (33 sites needing case-by-case judgment)
- `missing_docs`, `allow_attributes`/`allow_attributes_without_reason` deny (needs the 235-site conversion first)
- Numeric-cast lints, feature-powerset PR promotion, beta-toolchain canary, dylint crate
- `clippy.toml` threshold changes

## Git workflow

- Branch off `main`: `chore/lint-strictness-silent-failures`.
- One commit per step below, signed (`-s`), pushed immediately.
- Open a PR to `main`; do not merge.

## Steps

### Step 1: Rustdoc lint table

Add to root `Cargo.toml`, after `[workspace.lints.rust]`:

```toml
[workspace.lints.rustdoc]
broken_intra_doc_links = "deny"
private_intra_doc_links = "warn"
redundant_explicit_links = "warn"
unescaped_backticks = "warn"
```

(The roadmap names `unescaped_quotes_in_doc_comment`; if `cargo doc` rejects that lint name on the pinned toolchain 1.96.1, use `unescaped_backticks` as above and note the substitution in the commit message.) Every crate already inherits via `[lints] workspace = true`. Then run the doc build and fix what fires: broken intra-doc links get corrected link targets; genuinely unlinkable references become plain code spans.

**Verify**: `cargo doc --workspace --no-deps --locked 2>&1 | grep -c warning` → 0, exit 0.

### Step 2: `missing_debug_implementations` warn → deny

In `Cargo.toml` line 145 change `"warn"` to `"deny"`. The ~11 existing `#[expect(missing_debug_implementations, …)]` sites keep compiling (an `expect` on a deny-level lint is fine). Fix or `#[expect]` any *new* gaps the build reveals.

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0.

### Step 3: Silent-failure + async-guard lint adoption

Add to `[workspace.lints.clippy]` (keep the table's existing grouping style — place next to the other correctness denials):

```toml
let_underscore_future = "deny"
let_underscore_must_use = "deny"
unused_result_ok = "deny"
assertions_on_result_states = "deny"
await_holding_lock = "deny"
await_holding_refcell_ref = "deny"
```

Fix the enumerated sites:
- The five `.ok();` statements: replace with explicit handling. In `cleanup.rs` the `row.ok();` sites discard per-row errors inside a loop — convert each to `if let Err(err) = row { … }` logging through the module's existing error path (`debug_log!` or the surrounding function's error collection — match whichever the enclosing function already uses; read the surrounding 30 lines first). The twin `prune_output.rs:119` files get the identical fix in both copies.
- The `let _ =` must-use sites: `firewall.rs:215` and `services.rs:313` — handle or comment-justify via `#[expect(clippy::let_underscore_must_use, reason = "…")]` only when the discard is genuinely intended; `standalone_dialog.rs:41` (`OnceCell::set` losing a race is fine) — expect with reason "second initialization is a benign race"; `terminal.rs:53` (tcflush on teardown) — expect with reason "best-effort terminal restore".
- Anything else the workspace clippy run flags: same policy — fix if trivial, narrow reasoned `#[expect]` otherwise. If total new suppressions exceed 10, STOP (the audit predicted ~11 hits; a flood means a measurement miss).

**Verify**: `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0; `cargo nextest run --workspace --all-features --locked` → all pass.

### Step 4: Suppression reason-gate + per-crate bare-allow ratchet

Create `crates/jackin-xtask/src/suppressions.rs`: a gate `cargo xtask lint suppressions` that
1. scans `crates/**/*.rs` for `#[allow(`/`#![allow(` attributes without `reason =` (reuse the parsing approach from plan 010's `health.rs`; if 010 landed, factor the scanner into a shared `pub(crate) fn` in `health.rs` and call it — do not duplicate the multi-line parser twice);
2. reads `suppression-budget.toml` (create it: header comment + `[[crate]]` entries `{ name = "jackin-console", bare_allow = <measured> }` for every crate with a nonzero count, PLUS the per-lint `#[expect]` ledger roadmap line 69 specifies — `[[expect]]` entries `{ lint = "clippy::too_many_lines", crate = "jackin-runtime", count = <measured> }` for every (lint, crate) pair with a nonzero `#[expect]` count, from the same scanner's per-lint aggregation — both generated via a `--print-budget` flag mirroring `lint.rs:40-48`);
3. enforces shrink-only semantics exactly like the file-size gate (`lint.rs:220-244`) over BOTH tables: count above budget → fail naming the crate (or lint+crate pair) and the delta; count below budget → fail telling the executor to shrink the row; row now at zero → fail telling them to delete it. Failure text must state the fix and the rerun command (`cargo xtask lint suppressions --print-budget`), matching the bar set by `test_layout.rs:277-281`.

Register as `LintCommand::Suppressions` in `main.rs` and chain it in `run_all_lints` after `agent_links::enforce()?`. Add the README structure row.

**Verify**: `cargo run -p jackin-xtask -- lint suppressions` → `suppression gate OK …`; `cargo nextest run -p jackin-xtask` → all pass (new tests included); temporarily add a bare `#[allow(dead_code)]` to any file, rerun → gate fails naming that crate; revert the probe.

### Step 5: Doc tests into PR CI, pending-snap check

1. `crates/jackin-xtask/src/ci.rs`: in `build_steps` (line 101-165), after the `nextest` step add `cargo("doctest", &["test", "--doc", "--workspace", "--locked"])`.
2. `.github/workflows/ci.yml`: in the `test` job, after the nextest run step, add `- run: cargo test --doc --workspace --locked`. Keep the hygiene.yml copy — the scheduled lane re-running doctests is harmless double cover and stays consistent with the powerset lane (PR/main parity rule: this *adds* a PR gate, so parity is preserved).
3. Same ci.yml `test` job, add a cheap snapshot-hygiene step:

```yaml
- name: Reject pending insta snapshots
  run: |
    pending=$(git ls-files --others --exclude-standard '*.pending-snap'; find crates -name '*.pending-snap' -print)
    if [ -n "$pending" ]; then echo "::error::pending insta snapshots found:"; echo "$pending"; exit 1; fi
```

**Verify**: `actionlint .github/workflows/ci.yml .github/workflows/hygiene.yml` → exit 0; `cargo test --doc --workspace --locked` → passes locally; `cargo xtask ci --fast` → `ci gate OK` (now includes the doctest step).

### Step 6: Roadmap status

Update the Phase 1 section of `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx`: mark the silent-failure family, the await-holding pair (note the audit finding: zero pre-existing hits, adopted as regression guard — correct the "non-`Send` guards held across `.await` in places" claim), rustdoc table, `missing_debug_implementations = deny`, doc-tests-in-PR, pending-snap check, and the reason-gate as shipped; leave the deferred families listed as open. Also record the open contradiction the audit found: the roadmap's `cargo-public-api` item (line 60) conflicts with the research dossier's Skip decision (`docs/content/docs/reference/research/ci/rust-tooling/rust-ci-tooling.mdx:148`, "a CLI workspace with no published library crate has nothing meaningful to diff") — add one sentence flagging that the decision must be reconciled before any snapshot work.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass.

## Test plan

- `crates/jackin-xtask/src/suppressions/tests.rs`: budget parse; over-budget fail; stale-row fail (crate at zero still listed); under-budget shrink-forcing fail; `--print-budget` output round-trips. Pattern: `crates/jackin-xtask/src/lint/tests.rs`.
- Full workspace: `cargo nextest run --workspace --all-features --locked` green; `cargo test --doc --workspace --locked` green.

## Done criteria

- [ ] `[workspace.lints.rustdoc]` present; `cargo doc --workspace --no-deps --locked` clean
- [ ] `missing_debug_implementations = "deny"` in Cargo.toml; workspace clippy clean
- [ ] The six new clippy lints deny; `rg 'unused_result_ok|let_underscore_must_use' Cargo.toml` shows them; the five `.ok();` sites are gone (`rg -n '^\s*\w+\.ok\(\);' crates/ -g '*.rs'` in production code → no matches outside tests)
- [ ] `cargo xtask lint suppressions` passes and is part of `cargo xtask lint`; `suppression-budget.toml` committed
- [ ] ci.yml runs doc tests + pending-snap check on PRs; `ci.rs` includes the doctest step
- [ ] `cargo xtask ci --fast` → `ci gate OK`
- [ ] Roadmap Phase 1 status updated incl. the await-holding correction and the cargo-public-api contradiction note
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back if:

- Step 1's doc build emits more than 40 rustdoc warnings (backlog bigger than measured — needs its own plan).
- Step 3 requires more than 10 new `#[expect]`s across the workspace.
- Any enumerated fix site's code no longer matches the description (file moved/refactored).
- The `await_holding_lock` adoption flags any real hit — the audit measured zero; a hit means either drift or a misread, and the fix (restructuring a lock across await) is not in this plan's scope.
- Plan 010 has not landed and you cannot generate the baseline for `suppression-budget.toml` mechanically — do not hand-count.

## Maintenance notes

- The reason-gate + `suppression-budget.toml` fold into plan 017's unified `ratchet.toml` engine; keep the TOML schema minimal (`name`, `bare_allow`) so the port is mechanical.
- When the slice/indexing family later lands, add `allow-indexing-slicing-in-tests = true` (and `allow-dbg-in-tests`) to `clippy.toml` in the same PR — the escape valves and the lint must ship together.
- Reviewer should scrutinize: every new `#[expect]`'s `reason =` string (must state why the suppression is legitimate, not restate the lint name), and the `cleanup.rs` error-handling conversions (behavior change from silent discard to logged error).
