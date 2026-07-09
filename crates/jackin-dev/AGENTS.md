# AGENTS.md — jackin-dev

The `jackin-dev` PR-verification + contributor tooling binary.

## Hard rules (this crate)

- **Tier & dependencies:** standalone binary (tooling). No workspace dependencies — it bootstraps a PR checkout before the workspace is built, so it must not depend on jackin❯ runtime crates. Keep third-party deps minimal.
- **Keep `README.md` current:** update it when the `jackin-dev` CLI surface or workflows change (see `crates/AGENTS.md`).
- **Stay self-contained.** This tool runs against a fresh checkout; do not add assumptions about a built workspace. Coordinate CLI changes with `.github/AGENTS.md` and the PR-verification docs.
- **`--version` must work offline.** Keep version reporting cheap and dependency-light.

## What lives here vs elsewhere

- This crate owns: the `jackin-dev` binary (PR sync/path/env, contributor workflows).
- The CI lanes (lint, ci, docs gates) live in `jackin-xtask`. PR merge/DCO policy lives in `.github/AGENTS.md` + `CONTRIBUTING.md`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
