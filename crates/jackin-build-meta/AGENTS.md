# AGENTS.md — jackin-build-meta

Build-script helpers shared by jackin❯ crates. Each workspace binary crate derives a runtime version string here.

## Hard rules (this crate)

- **Tier & dependencies:** build-time helper. No workspace dependencies; intended as a `build-dependency`. Keep it trivially small and compile-cheap — it runs in every binary's build script.
- **Keep `README.md` current:** update it when structure, public API, or responsibilities change (see `crates/AGENTS.md`).
- **No runtime logic.** This crate computes build-time metadata only. Local non-CI builds use `<cargo-version>` deliberately so incremental builds don't invalidate every build-meta consumer; do not change that without updating `CONTRIBUTING.md` and the `JACKIN_VERSION` stamping rules.
- **Stable interface for build scripts.** Consumers depend on the version-stamping API; break it only with a coordinated bump.

## What lives here vs elsewhere

- This crate owns: version-string derivation and build-time metadata helpers.
- Runtime use of the stamped version (CLI `--version`, telemetry) lives in the binary crates and `jackin-usage`. CI release stamping (`CI=true`) lives in the GitHub Actions workflows.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
