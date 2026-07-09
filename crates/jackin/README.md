# jackin

The jackin❯ CLI — the operator-facing binary that loads roles into isolated containers, drives the console, and wires the workspace together. This crate is the L4 entry/glue layer: `lib.rs` re-exports the module tree consumed by `main.rs`, `src/bin/role.rs`, and integration tests; the crate is simultaneously a library and a binary.

## What this crate owns

- The CLI surface (`cli`) and top-level application wiring (`app`), error type (`error`), and preflight checks (`preflight`).
- Console entry (`console`), launch/prompt flow (`prompt`), and the Warp integration (`warp`).
- Role-authoring (`role_authoring`, `role_claude_plugins`) and workspace commands (`workspace`).

## Architecture tier and allowed dependencies

**L4 entry/glue crate.** It depends on the whole stack — `jackin-config`, `jackin-core`, `jackin-docker`, `jackin-manifest`, `jackin-diagnostics`, `jackin-env`, `jackin-image`, `jackin-runtime`, `jackin-tui`, `jackin-launch-tui`, `jackin-console`, `jackin-protocol`, `jackin-build-meta`. Nothing depends on this crate; it is the top of the graph. Keep it thin — it assembles, it does not implement domain logic.

## Structure

- `src/cli.rs` / `src/cli/`, `src/app.rs` / `src/app/` — CLI + app wiring
- `src/console.rs` / `src/console/` — console entry
- `src/prompt.rs` / `src/prompt/`, `src/preflight.rs` / `src/preflight/` — prompt flow + preflight
- `src/workspace.rs` / `src/workspace/`, `src/role_authoring.rs` / `src/role_authoring/`, `src/role_claude_plugins.rs` / `src/role_claude_plugins/` — workspace + role-authoring commands
- `src/warp.rs` / `src/warp/`, `src/error.rs`, `src/lib.rs`, `src/main.rs`, `src/bin/` — Warp integration, errors, crate roots, extra binaries

## Public API

The `jackin` binary (`jackin`, `jackin load`, `jackin console`, …) plus the library re-exports used by integration tests. New user-visible flags/commands land here (CLI-first), then optionally surface in the console.

## How to verify

```sh
cargo nextest run -p jackin
cargo clippy -p jackin --all-targets -- -D warnings
```

