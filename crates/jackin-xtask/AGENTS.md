# AGENTS.md — jackin-xtask

The workspace's `cargo xtask` automation — CI lanes, lint/docs/schema gates, release + PR tooling.

## Hard rules (this crate)

- **Tier & dependencies:** build/CI tooling (xtask). No workspace dependencies — it inspects the workspace from the outside and must not link runtime crates. Runs on the host toolchain only.
- **Keep `README.md` current:** update it when lanes are added/renamed, when a gate's rule changes, or when the module layout changes (see `crates/AGENTS.md`).
- **One entry point.** Project checks are discoverable via `cargo xtask <lane>`; a new check is a new lane here (and, where relevant, wired into `ci`). Prefer the narrowest correct lane an agent can run.
- **Print is allowed here.** `jackin-xtask` is a CLI whose reports are its output, so `print_stdout` is scoped-allowed with a `reason`; keep that carve-out to reporting only.
- **Gates are ratchets.** File-size/test-layout/agent-file gates are shrink-only with explicit, reasoned grandfathering; do not silently widen a budget or allowlist.

## What lives here vs elsewhere

- This crate owns: `cargo xtask` lanes (ci, lint, docs, schema, test_layout, agent_files, arch, profile_matrix, pty_fixture, construct, release_verify, pr).
- The `jackin-dev` PR-checkout binary is separate (`jackin-dev`). DCO/merge policy + workflow config live in `.github/`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
