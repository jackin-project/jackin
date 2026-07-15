# Plan 012: Advisory CI honesty — Miri per crate, hakari decision, Dylint pilot closure

> **Executor instructions**: Follow step by step; verify each step; STOP conditions binding. Update status row in `plans/codebase-health/README.md` when done.
>
> **Drift check (run first)**: `git diff --stat 846038946..HEAD -- .github/workflows/hygiene.yml crates/jackin-lints/`
> Mismatch with "Current state" = STOP.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW (advisory lanes; no merge gating changes unless decided)
- **Depends on**: none
- **Category**: dx (CI evidence integrity)
- **Planned at**: commit `846038946`, 2026-07-14

## Why this matters

Roadmap Rust-enforcement item 7: "Run and report Miri independently for core, config, manifest, and term. Perform an exit-status-checked hakari analysis with a before/after timing comparison and record an explicit adopt/no-adopt decision. … every promised result must be visible and distinguish tool failure from a clean result." Today the Miri step masks failures — on any failure of the combined core/config/manifest invocation, a `||` fallback re-runs only jackin-core, so a real UB finding in config or manifest goes green. The hakari step swallows its exit status (`|| true`), produces only a "before" timing artifact, and no adopt/no-adopt decision is recorded anywhere. Documentation-integrity item 3 additionally requires closing the Dylint pilot: the advisory `render_thread_purity` lint has no recorded false-positive rate and no promotion/retirement decision.

## Current state

- Miri, `.github/workflows/hygiene.yml:534-538`:

```yaml
      - name: miri pure crates
        run: |
          cargo +nightly miri test -p jackin-core -p jackin-config -p jackin-manifest --no-default-features --locked || \
            cargo +nightly miri test -p jackin-core --no-default-features --locked
          cargo +nightly miri test -p jackin-term --no-default-features --locked
```

  No `$GITHUB_STEP_SUMMARY` write; no artifact.
- Hakari, `hygiene.yml:624-638`: `cargo hakari init --dry-run 2>&1 | tee hakari-dry-run.txt || true`, then one `cargo build --workspace --timings --locked` uploaded as `cargo-timings-hygiene-baseline`. No "after" run; research docs still list hakari as Open (`docs/content/docs/reference/research/ci/rust-tooling/rust-ci-tooling.mdx:153`, `.../ci/performance/ci-performance-analysis.mdx:65`).
- Dylint: `crates/jackin-lints/src/lib.rs:42-44` — `render_thread_purity`, level Warn; runs only in `hygiene.yml:641-687` `dylint-advisory` with `continue-on-error: true` and `|| true`; absent from PR CI. No FP tally or decision in `crates/jackin-lints/README.md`, `AGENTS.md`, `TODO.md`, `DEFECT_LEDGER.md`, or the roadmap. UI test corpus: `crates/jackin-lints/ui/`.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Workflow lint | `actionlint .github/workflows/hygiene.yml` (installed via mise; else careful review) | no errors |
| Local Miri spot | `rustup toolchain install nightly --component miri && cargo +nightly miri test -p jackin-config --no-default-features --locked` | pass (slow; optional locally) |
| Dylint run | see `hygiene.yml:641-687` for the exact invocation to replicate | lint output |
| Docs gates | `cargo xtask roadmap audit && cargo xtask docs repo-links` | exit 0 |

## Scope

**In scope**: `.github/workflows/hygiene.yml` (miri + hakari + dylint jobs); a decision record for hakari (research doc update: `rust-ci-tooling.mdx` status flip) and for the Dylint pilot (`crates/jackin-lints/README.md` + roadmap item note); `crates/jackin-lints/README.md`.

**Out of scope**: making any advisory lane blocking (that's a severity decision recorded for the operator, not taken unilaterally); adding new Dylint lints (only the pilot closure); Miri for crates beyond the four named.

## Git workflow

Branch `ci/advisory-honesty`; Conventional Commits (`ci:` type); `git commit -s`; push per commit.

## Steps

### Step 1: Split Miri into four independent, reported invocations

Replace the compound step with four steps (or a `matrix: crate: [jackin-core, jackin-config, jackin-manifest, jackin-term]`): each runs `cargo +nightly miri test -p <crate> --no-default-features --locked` with NO `||` fallback, captures its exit status, and appends `miri <crate>: PASS|FAIL(<code>)` to `$GITHUB_STEP_SUMMARY`. Keep the job advisory if it is today (check the job's `continue-on-error`/dependents before changing semantics) — the requirement is independent visibility, not blocking.

**Verify**: `actionlint` clean; YAML review confirms no fallback and per-crate summary lines.

### Step 2: Exit-checked hakari with before/after timing

Restructure: (a) baseline `cargo build --workspace --timings --locked` → artifact `cargo-timings-hakari-before`; (b) `cargo hakari init` (real, in the CI checkout only) + `cargo hakari generate` per cargo-hakari docs, capturing the real exit status — a non-zero status marks the step failed-visible (advisory job may continue but the summary must say TOOL FAILURE, distinguishing it from a clean run); (c) `cargo build --workspace --timings --locked` again → `cargo-timings-hakari-after`; (d) summary step computes/echoes the delta of total build wall-time from the two `--timings` HTML/JSON artifacts (cargo emits `target/cargo-timings/cargo-timing-*.html` and with `-Zunstable-options` JSON — if JSON unavailable on stable, record wall-clock of the two build steps as the comparison and say so in the summary).

**Verify**: `actionlint` clean; dry-read confirms exit status propagates to the summary.

### Step 3: Record the hakari decision

After one observed run of the new lane (trigger `workflow_dispatch` if permitted, else note in PR that the first scheduled run supplies the data): write the adopt/no-adopt decision with the measured delta into `docs/content/docs/reference/research/ci/rust-tooling/rust-ci-tooling.mdx` (flip its hakari row from Open) and cross-reference in the roadmap item. If the run cannot be triggered from this plan's PR, the decision step is recorded as a follow-up checkbox in the PR body and the plan status becomes DONE-except-decision — say so in the status row.

**Verify**: `cargo xtask docs repo-links` + `cargo xtask roadmap audit` → exit 0.

### Step 4: Close the Dylint pilot

Run the dylint lane's invocation locally (or via workflow_dispatch); tally findings: true positives vs false positives against the render-thread-purity rule's intent (blocking calls on render threads — the `ui/` tests document intended positives). Record in `crates/jackin-lints/README.md`: FP rate, corpus date, and the decision — promote to a pinned CI lane (if FP≈0 and signal real) or retire (if noise). If promoting: add the pinned lane (non-`|| true`, pinned toolchain per existing job) in the same PR. If retiring: remove the advisory job and note the retirement rationale. Either way the roadmap Documentation-integrity item 3's ask is now answered in writing.

**Verify**: `cargo nextest run -p jackin-lints` (or its UI test harness — check `crates/jackin-lints/README.md` for the test command) → pass; docs gates green.

## Test plan

CI-config plan: verification is actionlint + observed-run evidence + docs gates. No unit tests.

## Done criteria

- [x] Four independent, parallel Miri matrix invocations, no `||` fallback,
  per-crate summary lines
- [x] Hakari lane: real exit status visible, before/after timing artifacts, tool-failure vs clean distinguishable
- [x] Hakari adopt/no-adopt decision recorded (or explicitly staged as first-run follow-up)
- [x] Dylint pilot: FP rate + promote/retire decision recorded; CI matches the decision
- [x] `actionlint` + docs gates green; status row updated

## STOP conditions

- `cargo hakari` cannot run without mutating checked-in files in a way the repo forbids committing — it runs only in the CI workspace; if the tool insists on manifest edits that break subsequent steps, capture the log and report.
- Dylint tally is ambiguous (rule intent unclear for half the findings) — report the split rather than inventing a threshold.
- Workflow permissions block `workflow_dispatch` testing — note it; do not force-push CI experiments to main-adjacent branches.

## Maintenance notes

- Any future advisory lane must follow the same pattern: real exit status captured, result visible in the step summary, tool failure ≠ clean.
- If hakari is adopted, `cargo hakari verify` joins PR CI and the workspace-hack crate becomes checked in — that's a separate implementation PR.

## Execution notes

- The 2026-07-15 full-workspace Dylint run reported zero
  `render_thread_purity` findings and zero false positives. Together with the
  positive/negative/spawn-boundary UI corpus, that evidence promoted the
  pinned 6.0.1 Hygiene lane to exit-status enforcement.
- The upstream 6.0.1 prebuilt embeds its release-builder `dylint_driver` path;
  the lane source-builds the same pinned version under `target/dylint-tools`
  and invokes it explicitly.
- Hygiene run `29397057453` completed the exit-checked Hakari experiment on
  2026-07-15. Clean locked workspace builds took **152 seconds before** and
  **152 seconds after** `init`, `generate`, and `manage-deps` (0 seconds / 0%).
  The explicit decision is **no-adopt**: the generated workspace-hack and its
  dependency maintenance have no measured build-time return.
- Hygiene run `29397482618` proved the original sequential Miri invocations
  could consume most of the shared 60-minute job timeout before `jackin-term`.
  The final lane uses a fail-fast-disabled four-crate matrix, preserving each
  exit status and summary while preventing a slow crate from starving another.
  Run `29401172187` then showed the full 118-test `jackin-term` invocation
  exceeds 60 minutes under Miri, so each isolated matrix job has a 180-minute
  ceiling rather than weakening the crate invocation to a smoke subset.
