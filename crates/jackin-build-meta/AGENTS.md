# AGENTS.md — jackin-build-meta

Build-script helpers shared by jackin❯ crates (runtime version strings).

## Rules (this crate)

- Build-time metadata only; no runtime logic.
- Local non-CI builds deliberately use `<cargo-version>` so incremental builds don't invalidate every consumer — do not change that without updating `CONTRIBUTING.md` and the `JACKIN_VERSION` stamping rules.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
