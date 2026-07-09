# Plan 022: Phase 1/4 — scoped feature-powerset in PR CI, beta-toolchain clippy canary, `cargo xtask ci` partitions

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md`.
>
> **Drift check (run first)**: `git diff --stat e80d5cc0a..HEAD -- .github/workflows/ci.yml .github/workflows/hygiene.yml crates/jackin-xtask/src/ci.rs`
> Plan 011 (if landed) adds a doctest step to ci.rs and ci.yml — expected.
> On any other mismatch with the "Current state" excerpts, treat it as a STOP
> condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW-MED (CI-config only; the powerset lane adds PR minutes — measure before requiring)
- **Depends on**: none (coordinate with plan 011 on ci.rs/ci.yml conflicts)
- **Category**: dx
- **Planned at**: commit `e80d5cc0a`, 2026-07-09

## Why this matters

Three roadmap items, one CI-shaped PR. (1) Phase 1 PR-gate item 1: feature-combination breakage is invisible until the nightly hygiene cron — a PR that breaks an optional-feature build merges green and reddens `main` for up to 24h, violating the repo's own PR/main-parity hard rule; the roadmap wants a *scoped* powerset lane in PR CI, "starting with crates that actually expose optional behavior". (2) Phase 1 toolchain-canary item 1: the workspace is deny-by-default on a pinned `1.96.1`; with no beta lane, every toolchain bump is a surprise red — new/renamed lints should surface weeks early as a scheduled advisory. (3) Phase 4 item 8: `cargo xtask ci` is a monolith — the audit's lane map shows only E2E (flag) and partial lint/docs are individually addressable; policy/tests/MSRV require the full run, so agents cannot run "the smallest correct command" mirroring a CI partition.

## Current state

Verified at the planning commit.

- Feature powerset runs ONLY in the scheduled lane: `.github/workflows/hygiene.yml:86` `cargo hack check --workspace --feature-powerset --all-targets --locked` (cron `23 11 * * *` + dispatch). No `cargo hack` in ci.yml. `cargo-hack` is mise-pinned (`mise.toml`: `"cargo:cargo-hack" = "0.6.45"`).
- Crates that actually declare features (census of `crates/*/Cargo.toml` `[features]`, verified):
  - Real optional behavior: `jackin` (`default = ["otlp"]`, `e2e`, `otlp`, `test-support`), `jackin-diagnostics` (`otlp`), `jackin-capsule` (`dhat-heap`, `codex-app-server-authority`), `jackin-agent-status` (`codex-app-server-authority`), `jackin-term` (`dhat-heap`), `jackin-runtime` (`daemon-spike`, `test-support`).
  - test-support-only: `jackin-config`, `jackin-console`, `jackin-env`, `jackin-instance`, `jackin-isolation`.
  - The scoped PR set = the six real-optional-behavior crates (`-p` flags); the workspace-wide powerset stays scheduled.
- No beta/nightly toolchain reference in any workflow; `rust-toolchain.toml` pins `channel = "1.96.1"` (components clippy+rustfmt). Workflow tooling rules (`.github/CLAUDE.md`, hard rules): all tools via `jdx/mise-action`; mise reads the toolchain from `rust-toolchain.toml` via `idiomatic_version_file`, so a beta lane must override explicitly (e.g. `install_args: "rust@beta"` — verify mise accepts a channel; fallback: `rustup toolchain install beta` after mise, since rustup is present once mise installs rust). Read the existing `scheduled-hygiene` job (hygiene.yml:49-92) and mirror its checkout/cache/mise steps.
- `crates/jackin-xtask/src/ci.rs` (read in full at `47dd5fca0`): `CiArgs { fast, e2e, base }` (lines 11-22); `build_steps` (lines 101-165) returns one flat `Vec<Step>`: actionlint, fmt, clippy, check, nextest, audit, deny, schema-check, `lint --strict`, shear, msrv, then powerset unless `--fast`; `run` (lines 68-99) executes all steps then optionally the e2e step. `Step` is `{name, program, args, env}` (lines 24-58). Tests exist at `ci/tests.rs`.
- CI partitions the roadmap names: lint, policy, tests, snapshots, docs, MSRV, E2E. Mapping onto the existing steps: **lint** = fmt + clippy + `lint --strict` (+ actionlint); **policy** = audit + deny + shear + schema-check; **tests** = nextest (+ doctest if plan 011 landed); **msrv** = the MSRV check; **e2e** = the existing flag; **docs** = `cargo xtask roadmap audit` + `docs repo-links` + `research check` (today NOT in ci.rs at all — a real parity gap worth including); **snapshots** = no dedicated lane exists (insta snapshots run inside nextest) — expose it as an alias for the two snapshot-bearing crates' nextest run.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Scoped powerset locally | `cargo hack check -p jackin -p jackin-diagnostics -p jackin-capsule -p jackin-agent-status -p jackin-term -p jackin-runtime --feature-powerset --all-targets --locked` | exit 0 (time it) |
| Xtask tests | `cargo nextest run -p jackin-xtask` | all pass |
| Partition smoke | `cargo run -p jackin-xtask -- ci --only lint` | runs only the lint steps, `ci gate OK` |
| Workflow lint | `actionlint .github/workflows/ci.yml .github/workflows/hygiene.yml` | exit 0 |
| Full local gate | `cargo xtask ci --fast` | `ci gate OK` |

## Scope

**In scope**:
- `.github/workflows/ci.yml` (one new `feature-powerset-scoped` job)
- `.github/workflows/hygiene.yml` (one new `beta-clippy-canary` job; keep the existing full powerset)
- `crates/jackin-xtask/src/ci.rs` + `ci/tests.rs` (partition selection)
- `TESTING.md` (extend the plan-010 verification matrix rows with the partition aliases — if plan 010 hasn't landed, add a short standalone note instead)
- Roadmap Phase 1 + Phase 4 item 8 status
- `plans/code-health/README.md` row

**Out of scope**:
- Making the beta canary blocking (it is advisory forever until the roadmap says otherwise)
- cargo-llvm-cov / miri / sanitizers / mutants / hakari lanes (separate ledger items)
- Changing what the full `cargo xtask ci` runs (except adding the docs partition — see Step 3; `--fast`/`--e2e` semantics unchanged)
- rust-nextest.yml

## Git workflow

- Branch off `main`: `ci/powerset-canary-partitions`.
- One commit per step, `-s`, push each. PR to `main`; do not merge. Push-only/scheduled jobs can't run on PR — smoke the canary via `gh workflow run hygiene.yml --ref <branch>` per `.github/CLAUDE.md` before merge-readiness, and say so in the PR body.

## Steps

### Step 1: Scoped powerset PR job

Time the scoped command locally first (Commands table). If it exceeds ~10 minutes cold, drop `jackin` (the binary crate with the biggest closure) from the PR set and note it — the point is catching optional-dep gaps in the leaf crates cheaply. Add a `feature-powerset-scoped` job to ci.yml modeled on an existing cargo job's checkout/mise/rust-cache steps (copy `bench-build`'s skeleton, lines 855-900; mise `install_args` must add `cargo:cargo-hack`), gated on `needs.changes.outputs.rust == 'true'`, running the scoped command. Wire it into the `ci-required` needs list (find `ci-required:` at ci.yml:1068 and add the job name) — it is a required PR gate, that is the roadmap's ask. Keep hygiene.yml's full `--workspace` powerset unchanged (it covers the test-support-only crates).

**Verify**: local scoped command exit 0; `actionlint .github/workflows/ci.yml` → exit 0.

### Step 2: Beta clippy canary (scheduled, advisory)

Add a `beta-clippy-canary` job to hygiene.yml mirroring `scheduled-hygiene`'s setup steps, with `continue-on-error: true` (advisory: it must never fail the workflow), that installs the beta toolchain (`rustup toolchain install beta --component clippy` after the mise rust step — mise pins the repo toolchain; rustup is available once rust is installed) and runs:

```yaml
      - run: cargo +beta clippy --workspace --all-targets --all-features --locked 2>&1 | tee beta-clippy.log || true
      - name: Summarize beta clippy
        run: |
          echo "### Beta clippy canary" >> "$GITHUB_STEP_SUMMARY"
          grep -E '^(warning|error)' beta-clippy.log | sort | uniq -c | sort -rn | head -20 >> "$GITHUB_STEP_SUMMARY" || echo "clean" >> "$GITHUB_STEP_SUMMARY"
      - uses: actions/upload-artifact@<same pinned SHA used elsewhere in this file>
        with: { name: beta-clippy-log, path: beta-clippy.log }
```

**Verify**: `actionlint .github/workflows/hygiene.yml` → exit 0; then `gh workflow run hygiene.yml --ref <your branch>` and confirm the canary job completes (advisory) — record the run URL in the PR body.

### Step 3: `cargo xtask ci --only <partition>`

In `ci.rs`: tag each `Step` with a partition (extend `Step` with `partition: &'static str` or restructure `build_steps` to emit `(partition, Step)` — pick the smaller diff). Partitions: `lint` (actionlint, fmt, clippy, `lint --strict`), `policy` (audit, deny, shear, schema-check), `tests` (check, nextest, doctest-if-present), `msrv`, `powerset` (the non-fast step), `docs` (NEW steps: `cargo xtask roadmap audit`, `cargo xtask docs repo-links`, `cargo xtask research check` — adding these to the FULL run closes a real parity gap; they are cheap), `snapshots` (alias: `cargo nextest run -p jackin-capsule -p jackin-console --locked`). Add `--only <name>` (repeatable) to `CiArgs`: when present, run only matching partitions; `--fast`/`--e2e` semantics unchanged when `--only` is absent. Update the command's doc comment (main.rs `Ci` variant) listing the partitions. Extend `ci/tests.rs`: partition filter selects the right step names; `--only docs` runs exactly the three docs steps; default full list unchanged except the three appended docs steps.

**Verify**: `cargo nextest run -p jackin-xtask` → all pass; `cargo run -p jackin-xtask -- ci --only lint` → runs the four lint steps only, `ci gate OK`; `cargo run -p jackin-xtask -- ci --fast` → full non-powerset run still green.

### Step 4: Docs + roadmap

- TESTING.md: add the partition aliases to the verification matrix (rows: "One CI partition" → `cargo xtask ci --only <lint|policy|tests|snapshots|docs|msrv>`).
- Roadmap: Phase 1 PR-gate item 1 shipped (scoped set named; full powerset stays scheduled); toolchain-canary item 1 shipped (advisory); Phase 4 item 8 shipped (partitions listed). Note the parity fix: docs gates now in `cargo xtask ci`.

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → pass; `cargo xtask ci --fast` → `ci gate OK` (now including docs steps).

## Test plan

- `ci/tests.rs` additions per Step 3 (partition mapping, `--only` filtering, default-list stability).
- Workflow-level: actionlint both files; one dispatched hygiene run on the branch proving the canary executes.

## Done criteria

- [ ] ci.yml has the scoped powerset job, in `ci-required`; local scoped run exit 0 and timed in the PR body
- [ ] hygiene.yml has the advisory beta canary; dispatched run URL in the PR body
- [ ] `cargo xtask ci --only <partition>` works for all seven partitions; full run gains the three docs steps; `ci/tests.rs` covers the filter
- [ ] TESTING.md + roadmap updated
- [ ] `cargo xtask ci --fast` → `ci gate OK`; `plans/code-health/README.md` row updated

## STOP conditions

Stop and report back if:

- The scoped powerset exceeds ~10 minutes even after dropping `jackin` (scoping needs a rethink, not a slow required gate).
- `cargo hack` powerset on the scoped set FAILS today (pre-existing feature breakage — that is a bug find; report it, do not fix features in this plan).
- mise/rustup beta installation fights the pinned `idiomatic_version_file` in a way the `rustup toolchain install beta` fallback doesn't resolve.
- Restructuring `build_steps` would change any existing step's args/env (byte-identical steps are the contract; only grouping and additions are allowed).

## Maintenance notes

- New optional features on any crate ⇒ add the crate to the scoped powerset job (the roadmap's "crates that actually expose optional behavior" set — keep the job's crate list commented with that rule).
- When the beta canary flags a new lint, the fix lands ahead of the toolchain bump — that is its whole job; check its artifact before every `rust-toolchain.toml` bump PR.
- Reviewer should scrutinize: `ci-required` wiring (a missing needs-entry silently makes the powerset optional) and that `--only` cannot skip DCO-critical gates when used in scripts (document: `--only` is a local-dev tool; merge readiness remains the full `ci`).
