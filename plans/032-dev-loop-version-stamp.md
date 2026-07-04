# Plan 032: Stop the git-SHA build stamp from forcing a wide rebuild after every commit

> **Executor instructions**: DX build-loop fix. Additive/local — must not change release-build behavior.
> Update `plans/README.md` when done.
>
> **Drift check**: `git diff --stat 46511939d..HEAD -- crates/jackin-build-meta/src/lib.rs crates/*/build.rs CONTRIBUTING.md`

## Status

- **Result**: DONE in PR #713 (`docs/advisor-improvement-plans`)
- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `46511939d`, 2026-07-03

## Why this matters

`jackin-build-meta` derives `<version>+<short-sha>` and emits `cargo:rerun-if-changed=.git/HEAD`,
`/refs`, `/packed-refs`; six crates consume it and re-export `JACKIN_VERSION`. The repo mandates "push
immediately after every commit", so **every commit changes HEAD → the six build scripts re-run →
`JACKIN_VERSION` changes → those crates recompile**, cascading to dependents (`jackin-runtime` is a dep of
jackin/image/isolation/instance/launch-tui). The downstream cost is documented in `.github/CLAUDE.md`: the
capsule binary cache key includes the SHA, so "any `cargo build` of jackin invalidates it" → a 2-3 min
capsule rebuild on the next `cargo nextest`/`cargo run` after each commit. An override hook already exists
(`JACKIN_VERSION_OVERRIDE`) — this plan just makes local dev use it by default.

## Current state

- `crates/jackin-build-meta/src/lib.rs:30-46` — `derive_version` emits the three `rerun-if-changed` git
  directives and honors `JACKIN_VERSION_OVERRIDE` if set:
  ```rust
  println!("cargo:rerun-if-env-changed=JACKIN_VERSION_OVERRIDE");
  println!("cargo:rerun-if-changed={git_dir_relative}/HEAD");
  // ... /refs, /packed-refs ...
  if let Ok(override_version) = std::env::var("JACKIN_VERSION_OVERRIDE") {
      return override_version;
  }
  ```
- Six consumers: `crates/jackin/build.rs:3`, `jackin-runtime/build.rs:3`, `jackin-docker/build.rs:3`,
  `jackin-image/build.rs:3`, `jackin-launch-tui/build.rs:3`, plus `jackin-capsule` (`JACKIN_CAPSULE_VERSION`).

## Scope

**In scope:** the documented local-dev setup (`CONTRIBUTING.md`/`TESTING.md`), a `.mise` env or a
`mise.toml`/`.cargo/config.toml`-level default for local dev, and — if plan 031 landed — the `xtask ci`
env. **Out of scope:** removing the SHA stamp from release/CI builds (it must stay accurate there); the
`build-meta` logic itself (the override hook already exists).

## Steps

### Step 1: Default `JACKIN_VERSION_OVERRIDE` for local dev

Provide a documented, low-friction way for a developer's inner loop to set `JACKIN_VERSION_OVERRIDE=dev`
(or `0.6.0-dev`), so build scripts stop re-stamping the SHA on every commit. Options (pick the one that fits
the repo's tooling):
- a `mise.toml` `[env]` entry (mise is the repo's tool manager) that sets `JACKIN_VERSION_OVERRIDE` for the
  dev environment, and/or
- a documented `export JACKIN_VERSION_OVERRIDE=dev` in the local-dev bootstrap section.

Do **not** set it in a way that leaks into release/CI builds (those must keep the real SHA — confirm the
release/preview workflows don't inherit the local mise env, or scope it to a local-only file).

**Verify**: with `JACKIN_VERSION_OVERRIDE=dev` set, `git commit` then `cargo build -p jackin` does **not**
recompile `jackin-runtime` solely due to a SHA change — confirm via `cargo build -p jackin --timings` before
and after a no-op commit (the build-meta-consuming crates should be cache hits).

### Step 2: Document it

Add a one-line note to the local-dev setup (`CONTRIBUTING.md` or `TESTING.md`) explaining: local builds use
`JACKIN_VERSION_OVERRIDE=dev` to avoid a per-commit rebuild cascade; release/CI keep the real SHA.

**Verify**: `grep -rn "JACKIN_VERSION_OVERRIDE" CONTRIBUTING.md TESTING.md mise.toml` → ≥1 match.

## Done criteria

- [x] A committed, documented default sets the local inner loop to a stable package-version stamp
- [x] A no-op `git commit` no longer forces recompilation of the six build-meta consumers locally (timings prove)
- [x] Release/CI builds still stamp the real `<version>+<sha>` (confirm the override does not leak into
      `release.yml`/`preview.yml`/`construct.yml`)
- [x] `plans/README.md` row updated

## Completion notes

- `jackin-build-meta` now returns the package version for local non-CI builds, unless
  `JACKIN_VERSION_OVERRIDE` is explicitly set.
- CI/release/preview/construct builds still run with `CI` set, emit the `.git` `rerun-if-changed` watches,
  and stamp `<version>+<sha>`.
- Local non-CI builds no longer emit `.git/HEAD`, `.git/refs`, or `.git/packed-refs` watchers, so commits do
  not dirty the build-meta consumers.
- Updated contributor and GitHub-agent docs to explain that local cache keys are stable while CI/release
  artifacts keep provenance.
- Verification:
  - `env -u CI mise exec -- cargo build -p jackin --locked --timings` before commit, then the same command
    after commit `418094139`, finished fresh in 0.85s and `./target/debug/jackin --version` printed
    `jackin 0.6.0-dev`.
  - `CI=true mise exec -- cargo build -p jackin --locked` rebuilt the consumers and
    `./target/debug/jackin --version` printed `jackin 0.6.0-dev+4180941`.
  - `mise exec -- cargo test -p jackin-build-meta --locked` passed.

## STOP conditions

- The only way to set the override also affects CI/release builds (env leaks) — report; the SHA must stay
  accurate in shipped artifacts. Scope it to a local-only mechanism instead.
- Setting the override breaks a test that asserts the version string contains a SHA — update that test to
  tolerate the dev override locally, or gate it; if it's load-bearing, STOP.

## Maintenance notes

- Reviewer: verify shipped binaries (release/preview/construct) still embed the real commit SHA — the whole
  value of the stamp is provenance in artifacts.
- This is a pure inner-loop speedup; it must be invisible in CI output.
