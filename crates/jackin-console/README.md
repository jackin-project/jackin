# jackin-console

Canonical host-console product surface. Owns reusable console state, update/input planning, view composition, components, pure product decisions, and effects-as-data for the operator console — the TUI an operator drives via `jackin console`.

## What this crate owns

- Console state + planning for workspaces, mounts, and services (`workspace`, `services`, `github_mounts`).
- Mount diff/info and the mount-info cache (`mount_diff`, `mount_info`, `mount_info_cache`).
- Console view composition + input (`tui`).

## Architecture tier and allowed dependencies

**L3 presentation.** Allowed workspace dependencies: `jackin-config`, `jackin-console-oppicker`, `jackin-core`, `jackin-diagnostics`, `jackin-env`, `jackin-protocol`, `jackin-tui`. Must NOT depend on `jackin-runtime`, `jackin-launch-tui`, or `jackin-capsule` directly — console reaches runtime through effects-as-data, not direct calls.

## Structure

- `src/workspace.rs` / `src/workspace/` — console workspace state
- `src/services.rs` / `src/services/`, `src/github_mounts.rs` / `src/github_mounts/` — services + GitHub mounts
- `src/mount_info.rs` / `src/mount_info/`, `src/mount_info_cache.rs`, `src/mount_diff.rs` — mount info/diff/cache
- `src/tui.rs` / `src/tui/` — view composition + input

## Public API

Console state machine + view models consumed by the `jackin` binary's console entry point. Picker model/planning is split into `jackin-console-oppicker`; this crate owns only the side-effect adapters.

## How to verify

```sh
cargo nextest run -p jackin-console
cargo clippy -p jackin-console --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.
