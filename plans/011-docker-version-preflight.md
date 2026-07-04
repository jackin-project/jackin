# Plan 011: Decide whether the `docker_version` preflight needs a minimum-version floor (investigate)

> **Executor instructions**: Investigate-and-decide. Determine whether jackin❯ has a real minimum Docker
> engine version; then either add a floor or rename the check to reflect that it only reports. Update
> `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin/src/preflight.rs`

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`check_docker_version` runs `docker version --format {{.Server.Version}}` and returns `CheckResult::ok`
for **any** version string — it only `warn`s when the command itself fails, and its hint ("Upgrade Docker
to the latest stable release") implies a floor it never enforces. A too-old daemon that still answers
`docker version` passes cleanly. The check reads as a version *gate* but is purely a version *reporter*;
there is no `Fail` path, so it can never block `load`/`hardline` on an unsupported engine. This may be
intentional (informational-only), so the task is to decide, not to assume.

## Current state

`crates/jackin/src/preflight.rs:224-256` — `check_docker_version`:
```rust
// runs `docker version --format {{.Server.Version}}`
// on success: CheckResult::ok("docker_version", "Docker server {version}")
// on command failure: warn with hint "Upgrade Docker to the latest stable release"
// no Fail branch, no version parse/compare
```
The preflight module doc says preflight only "fails on any `Fail`", and this check emits only
`Ok`/`Warn`/`Skip`.

## Steps

### Step 1: Determine whether a real minimum exists

- Check docs / roadmap / CI for a stated minimum Docker/engine version
  (`grep -rn "Docker.*version\|minimum.*docker\|docker.*20\.\|docker.*24\." docs README.md`).
- Check whether any launch code path uses a Docker feature with a known engine-version requirement
  (e.g. specific `docker run` flags, BuildKit features) — `grep -rn "buildkit\|--mount=type\|compose" crates/jackin-docker/src crates/jackin-runtime/src`.
- Decide: (a) there **is** a real floor → Step 2a; (b) there is **not** → Step 2b.

### Step 2a: Add a floor (if a minimum exists)

Parse the reported `Server.Version` (use the `semver` crate — already in the workspace per
`ENGINEERING.md`; do not hand-roll version parsing) and return `Fail` (blocks launch) or `Warn` below
the documented minimum. Add a test with a below-floor version string → `Fail`, at/above → `Ok`.

### Step 2b: Rename to reflect reality (if no floor)

Rename/re-document the check so it reads as a version **reporter** (e.g. drop the "Upgrade Docker" hint
that implies enforcement, or rename to `report_docker_version`), so the next reader doesn't assume a gate
exists. Update any docs that describe preflight checks.

**Verify (either branch)**: `cargo check -p jackin --all-targets` → exit 0;
`cargo nextest run -p jackin -E 'test(/preflight|docker_version/)'` → pass.

## Done criteria

- [ ] A written decision (floor exists vs not) with evidence in this plan's row note
- [ ] Branch 2a: version parse + `Fail`/`Warn` floor + test; **or** Branch 2b: check renamed/re-documented
- [ ] `cargo clippy -p jackin -- -D warnings` exits 0
- [ ] `plans/README.md` row updated

## STOP conditions

- The minimum version is genuinely unknown and not documented anywhere — report and let the operator
  state the floor (or confirm informational-only); do not invent a version number.

## Maintenance notes

- If a floor is added, keep it in sync with the documented supported-Docker range; a reviewer should check
  the number against docs.
