# Plan 035: Scheduled advisory lanes — coverage, Miri, sanitized fuzz, mutation testing, hakari timing

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/code-health/README.md` — unless a reviewer dispatched you and told
> you they maintain the index.
>
> **Drift check (run first)**: `git diff --stat 0971da66d..HEAD -- .github/workflows/hygiene.yml mise.toml .config/nextest.toml crates/jackin-term/fuzz/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW (all lanes are scheduled/advisory — they cannot redden a PR; risk is CI-minutes waste, bounded by timeouts)
- **Depends on**: none (**file-conflict note**: plan 022 also edits CI workflows — it adds a beta-clippy canary to `hygiene.yml`. Either order works; append jobs rather than restructuring.)
- **Category**: dx (verification infrastructure)
- **Planned at**: commit `0971da66d`, 2026-07-09

## Why this matters

Roadmap Phase 1 "Advisory or scheduled lanes first" (`docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` lines 54-61) names six investigations: coverage as an artifact (item 1), a narrow Miri lane (2), scheduled sanitizers where fuzz targets exist (3), cargo-mutants on pure crates (4), and a hakari timing experiment (6) — item 5 (public-API report) was decided **Skip** by the operator on 2026-07-09 (dossier `rust-ci-tooling.mdx:148` wins; do not build it). Today none of the five exist: the workspace has zero coverage tooling, zero Miri, the single fuzz target runs `--sanitizer none` everywhere, and no mutation or workspace-hack tooling is pinned. The roadmap's ratchet principle — "add a strict gate only after a measured dry run proves the signal is actionable" — requires these advisory lanes to exist first: they ARE the measurement. This plan builds all five as scheduled jobs whose output is artifacts + step summaries, never PR failures.

## Current state

- **Scheduled lane today**: `.github/workflows/hygiene.yml` — cron `23 11 * * *` + `workflow_dispatch` (lines 4-6). Job `scheduled-hygiene` (line 49): checkout → rustup cache → `jdx/mise-action@e6a8b39… # v4.2.0` with `install_args: "cargo-binstall rust cargo:cargo-deny cargo:cargo-fuzz cargo:cargo-hack shellcheck"` (line 70) → cargo-registry + rust-cache composites → `cargo deny check advisories`, `cargo hack check --workspace --feature-powerset --all-targets --locked`, `cargo test --doc --workspace --locked`, shellcheck, then:

```yaml
- name: Long jackin-term fuzz run
  run: |
    cd crates/jackin-term
    cargo fuzz run --sanitizer none --target x86_64-unknown-linux-gnu damage_grid_process -- -max_total_time=300
```

  The job uploads **no artifacts** — results are step-summary only. A separate `native-macos` job (line 94) exists; leave it alone.
- **Artifact pattern to reuse**: composite `./.github/actions/sccache-stats/action.yml` — inputs `artifact-name`, `heading`, `retention-days` (default `"7"`); writes a `.txt` to `$GITHUB_STEP_SUMMARY` then `actions/upload-artifact@043fb46d… # v7.0.1` with `if-no-files-found: ignore`. All upload-artifact uses in the repo pin that exact SHA. New lanes upload with the same pinned action + `retention-days: 7`.
- **Fuzz inventory**: exactly one cargo-fuzz target — `damage_grid_process` (`crates/jackin-term/fuzz/Cargo.toml:16-19`, body `src/damage_grid_process.rs:14-45`, differential oneshot-vs-split `DamageGrid::process`, "zero panics on any byte sequence"). PR CI runs it 60s with `--sanitizer none` (`ci.yml:850-854`).
- **Pure crates** (no tokio/PTY/OS-heavy deps; suitable for Miri + mutants — verified from each `Cargo.toml`): `jackin-term`, `jackin-core`, `jackin-config`, `jackin-manifest`. **NOT suitable**: `jackin-protocol` (tokio net/time), `jackin-env` (portable-pty). None of the four has a `build.rs`; `unsafe_code = "forbid"` workspace-wide (Miri here guards library UB + pointer provenance in deps, not first-party unsafe).
- **Toolchain/tools**: `rust-toolchain.toml` pins `1.96.1` (components clippy/rustfmt). `mise.toml [tools]` pins cargo tools (nextest 0.9.136, fuzz 0.13.1, hack 0.6.45, deny, shear, audit…). **Absent everywhere** (verified): `cargo-llvm-cov`, `grcov`, `tarpaulin`, `cargo-mutants`, `miri`, `cargo-hakari`, any coverage config. Miri requires a **nightly** toolchain — it cannot run on the pinned 1.96.1; the Miri job installs `nightly` + `miri` component ad hoc (acceptable for a scheduled advisory lane; do NOT touch `rust-toolchain.toml`).
- **nextest**: `.config/nextest.toml` has profiles `default` (filter `not binary(dind_e2e)`) and `docker-e2e`. cargo-llvm-cov integrates via `cargo llvm-cov nextest`.
- **cargo timings**: clippy/check jobs already upload `cargo-timings-*` HTML artifacts (`ci.yml:516-521`, `:687-692`) — the hakari experiment's baseline comes from these existing artifacts, not new instrumentation.
- Repo workflow conventions: every action pinned by full SHA + version comment; jobs set `timeout-minutes`; `CARGO_INCREMENTAL: "0"`; mise-action installs tools. Match all four.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Workflow lint | `mise run actionlint` — if that task doesn't exist, `actionlint` (pinned in mise.toml) | exit 0 |
| Local lane rehearsal (coverage) | `cargo llvm-cov nextest -p jackin-term -p jackin-core -p jackin-config -p jackin-manifest --lcov --output-path coverage.lcov` (after `cargo binstall cargo-llvm-cov`) | exit 0, lcov file |
| Local Miri rehearsal | `rustup toolchain install nightly --component miri && cargo +nightly miri nextest run -p jackin-core` (or `cargo +nightly miri test -p jackin-core` if miri+nextest integration misbehaves) | tests pass under Miri |
| Merge-readiness | `cargo xtask ci --fast` | exit 0 |

(You cannot run GitHub scheduled jobs locally; rehearse each lane's core command locally where feasible, then rely on `workflow_dispatch` after merge for the full proof. Say so in the PR body.)

## Scope

**In scope** (the only files you should modify):
- `.github/workflows/hygiene.yml` (new jobs; do not restructure existing ones)
- `mise.toml` (pin `cargo:cargo-llvm-cov`, `cargo:cargo-mutants`, `cargo:cargo-hakari`)
- `TESTING.md` (one short "scheduled advisory lanes" table row per lane: what it measures, where the artifact lands)
- `docs/content/docs/roadmap/(codebase-health)/codebase-health-enforcement.mdx` (status sentence, Step 6)

**Out of scope** (do NOT touch):
- `ci.yml` — nothing here is PR-blocking; the PR fuzz job keeps `--sanitizer none` (fast smoke is its job).
- `rust-toolchain.toml` — the workspace toolchain does not move for Miri.
- Any `Cargo.toml` — **the hakari lane is a timing investigation only**; do NOT run `cargo hakari init`, do NOT create a workspace-hack crate (roadmap line 61: value must be proven with timing artifacts before adding generated maintenance surface).
- The public-API report (roadmap advisory item 5) — operator-decided Skip on 2026-07-09.
- Coverage thresholds/gates of any kind — artifact only.

## Git workflow

- Branch: current active branch if the operator designates one; otherwise propose `ci/advisory-verification-lanes` and wait for confirmation.
- Conventional Commits, signed, push after every commit: `git commit -s -m "ci(hygiene): add coverage, miri, sanitizer, mutants, hakari advisory lanes"` → `git push`.

## Steps

### Step 1: Pin the tools

Add to `mise.toml [tools]` (match existing `cargo:` entry style and current versions from crates.io at execution time): `cargo:cargo-llvm-cov`, `cargo:cargo-mutants`, `cargo:cargo-hakari`. (Miri comes via rustup nightly component, not mise; fuzz/hack already pinned.)

**Verify**: `mise install` → exit 0; `cargo llvm-cov --version && cargo mutants --version && cargo hakari --version` → print versions.

### Step 2: Coverage lane (roadmap advisory item 1)

New job `coverage` in `hygiene.yml` (same checkout/rustup-cache/mise/registry/rust-cache step stack as `scheduled-hygiene`; `timeout-minutes: 45`; mise `install_args` including `cargo:cargo-llvm-cov cargo:cargo-nextest`). Steps: `rustup component add llvm-tools`, then

```
cargo llvm-cov nextest -p jackin-term -p jackin-core -p jackin-config -p jackin-manifest -p jackin-protocol -p jackin-env --lcov --output-path coverage.lcov
cargo llvm-cov report --summary-only >> "$GITHUB_STEP_SUMMARY"
```

(protocol/env are fine under coverage — the pure-crate restriction applies to Miri/mutants only; this is the roadmap's "parser/protocol/config/terminal crate-group trend".) Upload `coverage.lcov` with the pinned upload-artifact SHA, `retention-days: 7`, `if-no-files-found: error`.

**Verify**: `actionlint` → exit 0. Local rehearsal command from the table → lcov file exists, summary prints per-crate line %.

### Step 3: Miri lane (item 2)

New job `miri` (`timeout-minutes: 60`): install nightly + component (`rustup toolchain install nightly --profile minimal --component miri`), then per pure crate:

```
cargo +nightly miri nextest run -p jackin-core -p jackin-config -p jackin-manifest --no-default-features
cargo +nightly miri nextest run -p jackin-term --no-default-features
```

Set `MIRIFLAGS: -Zmiri-disable-isolation` only if the dry run shows fs/env access in tests (jackin-config reads tempdirs — likely needed; start without, add on failure). If `miri nextest` integration fails, fall back to `cargo +nightly miri test -p <crate>`. If a specific test is fundamentally Miri-incompatible (e.g. real time), filter it with nextest `-E` expressions and record the filter in a YAML comment — do not delete or modify tests.

**Verify**: `actionlint` → exit 0. Local rehearsal on the smallest crate: `cargo +nightly miri nextest run -p jackin-manifest` → passes (or produces the documented filter list; >5 filtered tests → STOP condition 3).

### Step 4: Sanitized fuzz lane (item 3)

In the existing `scheduled-hygiene` job, add one step after "Long jackin-term fuzz run" (keep that step untouched):

```yaml
- name: ASan jackin-term fuzz run
  run: |
    cd crates/jackin-term
    cargo +nightly fuzz run --sanitizer address --target x86_64-unknown-linux-gnu damage_grid_process -- -max_total_time=300
```

(cargo-fuzz sanitizer builds need nightly; add the nightly install step to this job if Step 3's job doesn't share it. `damage_grid_process` is the only target; when plan 009's protocol fuzz targets land, they join here — note as YAML comment.)

**Verify**: `actionlint` → exit 0. Local 30s rehearsal: same command with `-max_total_time=30` → runs, no findings (a finding is a real bug: STOP condition 4 — report it, do not fix here).

### Step 5: Mutants + hakari investigation lanes (items 4, 6)

New job `mutants` (`timeout-minutes: 90`, pure crates only, sharded to stay bounded):

```
cargo mutants -p jackin-manifest -p jackin-config --timeout 120 --in-place -- --locked
```

Start with the two smallest pure crates (manifest 5 tests.rs, config 10); term/core join later if runtime allows. Upload `mutants.out/` as artifact (`if-no-files-found: warn`); append `cargo mutants` summary (caught/missed/unviable counts) to the step summary. Do NOT fail the job on missed mutants (`|| true` on the run step, but preserve the summary) — trend instrument only.

New job `hakari-timing` (`timeout-minutes: 45`): a pure investigation that answers "would a workspace-hack help?" without creating one:

```
cargo hakari init --dry-run || true
cargo build --workspace --timings
```

Upload the `target/cargo-timings/*.html` artifact as `cargo-timings-hygiene-baseline`; append `cargo hakari` dry-run output to the step summary. A YAML comment states the decision rule from the roadmap: adopt hakari only if timing artifacts prove a win.

**Verify**: `actionlint` → exit 0. Local rehearsal: `cargo mutants -p jackin-manifest --timeout 120 --list` → lists mutants (list-only is enough locally).

### Step 6: TESTING.md + roadmap status

Add a "Scheduled advisory lanes (hygiene.yml)" table to `TESTING.md`: lane → what it measures → artifact name → how to trigger manually (`gh workflow run hygiene.yml`). In the roadmap Phase 1 advisory list (lines 54-61): mark items 1-4 and 6 as shipped-advisory with the date; item 5 already carries the Skip decision (plan 011 owns writing that note — if it's not there yet, add "Skip per operator decision 2026-07-09, see rust-ci-tooling dossier" to item 5 now).

**Verify**: `cargo xtask roadmap audit && cargo xtask docs repo-links` → exit 0. `cargo xtask ci --fast` → exit 0.

## Test plan

Workflow-only change — no Rust tests. Verification = actionlint + local rehearsals per step + first `workflow_dispatch` run after merge (state in the PR body that the operator should `gh workflow run hygiene.yml` once and eyeball the five new artifacts/summaries).

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `actionlint` exits 0
- [ ] `grep -c "uses: actions/upload-artifact@043fb46d" .github/workflows/hygiene.yml` ≥ 2 (coverage + mutants/timings uploads, all on the pinned SHA)
- [ ] `grep -n "cargo-llvm-cov\|cargo-mutants\|cargo-hakari" mise.toml` shows all three pinned
- [ ] `grep -n "sanitizer address" .github/workflows/hygiene.yml` shows the ASan step; `grep -n "sanitizer none" .github/workflows/ci.yml` still shows the PR smoke unchanged
- [ ] No `workspace-hack` crate exists (`test ! -d crates/workspace-hack`)
- [ ] `git diff --name-only` touches only the four in-scope files
- [ ] `cargo xtask ci --fast` exits 0
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

1. `hygiene.yml`'s job structure changed since the excerpt (plan 022's canary may have landed — append after its jobs; STOP only if the mise-action/tool-install pattern itself changed).
2. Any pinned-action SHA you need differs from the ones in the file — copy the repo's existing pin, never introduce a new version yourself.
3. Miri requires filtering more than 5 tests per crate to pass — the lane's value is then questionable; report the incompatibility list instead of shipping a hollow lane.
4. The ASan rehearsal finds a real sanitizer failure — that is a bug discovery, not a lane problem; report it (file:line + reproducer) for the defect→gate ledger and land the lane anyway if the finding is in a dependency, or hold the ASan step if first-party.
5. `cargo mutants` cannot complete `-p jackin-manifest` within the timeout locally — shrink to `--shard`ed or `--file`-scoped runs; if still infeasible, ship the other four lanes and report mutants as needs-rescope.

## Maintenance notes

- These lanes are **instruments for later ratchets**: coverage trends may earn a per-crate floor (roadmap says "only later ratchet specific critical surfaces"); mutants' missed-mutant list feeds test-writing priorities; the hakari timing artifact decides adoption via plan 017-style evidence. None of that promotion happens without a fresh plan.
- When plan 009 lands protocol fuzz targets, add them to both fuzz steps (a YAML comment marks the spot).
- Nightly drift: the Miri/ASan jobs float on `nightly` — a scheduled failure after a nightly bump is expected noise; fix-or-pin decisions belong to the operator (plan 022's beta canary is the early-warning instrument).
- Reviewer scrutiny: confirm every new job has `timeout-minutes`, pinned action SHAs, and cannot block PRs (hygiene.yml has no `ci-required` linkage — keep it that way).
