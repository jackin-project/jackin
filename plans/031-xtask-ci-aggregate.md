# Plan 031: Add `cargo xtask ci` that reproduces the full CI merge-readiness gate locally

> **Executor instructions**: DX tooling plan. Build the aggregate command, wire the docs. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-xtask/src CONTRIBUTING.md mise.toml`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW (additive tooling)
- **Depends on**: none
- **Category**: dx (DX-01 + DX-04)
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

No single command reproduces the CI gate. `CONTRIBUTING.md` documents five commands (fmt, clippy, check,
nextest, docker-e2e), but `ci.yml` gates ~15 jobs — additionally `audit`, `dependency-policy` (cargo-deny),
`schema-check`, `file-size-gate`, `workspace-depgraph`, `msrv`, `actionlint`. A contributor who runs the
five documented commands can still fail CI on any of those, each a push-wait-red-fix round trip. Also, the
documented `docker-e2e` command **omits its mandatory pre-steps** (export the capsule binary via
`build-jackin-capsule --export`; Docker must be running), so following `CONTRIBUTING.md` alone yields a
confusing failure. One `cargo xtask ci` fixes both.

## Current state

- `CONTRIBUTING.md:76-82` — five manual merge-readiness commands.
- `ci.yml:260-1065` — ~15 gated jobs (fmt/clippy/check/test + audit/deny/schema/file-size/depgraph/msrv/actionlint/…).
- `crates/jackin-xtask/src/main.rs:40-108` — subcommands `lint {files,tests,arch}`, `construct`, `docs`,
  `pr`, `schema`, … — **no aggregate**; `mise.toml` tasks are all `construct-*`; no `justfile`/`Makefile`.
- `TESTING.md:41-44` — docker-e2e needs `eval "$(cargo run --bin build-jackin-capsule -- --export)"` first,
  plus a running Docker daemon; `.config/nextest.toml` confines the `docker-e2e` profile to `binary(dind_e2e)`
  single-threaded.

## Scope

**In scope:** `crates/jackin-xtask/src/` (new `ci` subcommand), `mise.toml` (a `ci` task), `CONTRIBUTING.md`
(point at it + fix the e2e pre-steps). **Out of scope:** changing what CI actually runs; the docker-e2e
lane's existence.

## Steps

### Step 1: Enumerate the required checks from CI

Read `ci.yml` and list the checks that gate a merge (the non-optional ones). At minimum: `cargo fmt --check`,
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`, `cargo check --all-targets`,
`cargo nextest run --all-features`, `cargo audit`, `cargo deny check licenses bans sources`, the schema
check, the file-size gate, the workspace-depgraph/arch lint, MSRV check, `actionlint`. Record the exact
command each job runs.

### Step 2: Implement `cargo xtask ci`

Add a `ci` subcommand to `jackin-xtask` that runs the Step-1 commands in sequence (fail-fast or
collect-all-failures — prefer collect-all so a contributor sees every failure at once), shelling out to the
same tools CI uses (installed via mise). Include flags: `--fast` (skip the slow docker-e2e / feature-powerset)
and `--e2e` (include docker-e2e with its pre-steps). Reuse the existing xtask command-runner infrastructure
(the crate already shells to tools for `lint`/`construct`).

**Verify**: `cargo run --bin xtask -- ci --fast` (or `cargo xtask ci --fast`) → runs fmt/clippy/check/nextest/
audit/deny/schema/file-size/arch/msrv and exits 0 on a clean tree.

### Step 3: Make docker-e2e correct-by-construction

In the `ci --e2e` path (or a dedicated `xtask e2e`), run the capsule export pre-step
(`build-jackin-capsule --export`) and check Docker is running before invoking
`cargo nextest run -p jackin --features e2e --profile docker-e2e`. Fail early with a clear message if Docker
isn't up.

### Step 4: Wire the docs

Update `CONTRIBUTING.md`'s merge-readiness section to say "run `cargo xtask ci` (or `mise run ci`)" as the
single gate, and precede any manual docker-e2e mention with the capsule-export + Docker-running
prerequisites. Add a `mise.toml` `[tasks.ci]` that calls `cargo xtask ci`.

**Verify**: `grep -n "xtask ci\|mise run ci" CONTRIBUTING.md` → ≥1 match;
`actionlint` still passes (no workflow change needed, but confirm nothing broke).

## Done criteria

- [ ] `cargo xtask ci --fast` runs the full non-e2e gate and matches CI's pass/fail on a clean tree
- [ ] `cargo xtask ci --e2e` runs the capsule-export pre-step + docker-e2e (or fails early if Docker is down)
- [ ] `CONTRIBUTING.md` points at the single command; docker-e2e pre-steps documented
- [ ] `mise.toml` has a `ci` task
- [ ] `cargo clippy -p jackin-xtask -- -D warnings` exits 0
- [ ] `plans/README.md` row updated

## STOP conditions

- A CI job runs something not reproducible locally (e.g. needs the self-hosted `velnor` runner or org
  secrets) — exclude it from `xtask ci`, and `log`/document that it's CI-only so the command doesn't falsely
  promise full parity.
- The schema/file-size/arch checks are already xtask subcommands — reuse them (`cargo xtask lint …`,
  `cargo xtask schema`), don't reimplement.

## Maintenance notes

- Keep `xtask ci` in sync with `ci.yml`: when a new gating job is added to CI, add it here (a reviewer
  should check both change together). Consider a test that diffs the two lists.
- This is the single source both `CONTRIBUTING.md` and contributors reference — "green locally" should mean
  "green on CI".
