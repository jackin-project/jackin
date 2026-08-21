# jackin-console

Canonical host-console surface: state, input planning, views, components, and effects-as-data for `jackin console`.

## What this crate owns

- Console state + planning for workspaces, mounts, and services (`workspace`, `services`, `github_mounts`).
- Mount diff/info and the mount-info cache (`mount_diff`, `mount_info`, `mount_info_cache`).
- Console view composition + input (`tui`), including the Usage route. Usage
  adapts the Rust-owned projection; it does not discover accounts or refresh
  providers. See `docs/content/reference/tui/navigation.mdx` for the route
  contract.

## Architecture tier and allowed dependencies

**L3 presentation.** Dependencies include `jackin-config`, `jackin-oppicker`, `jackin-core`, `jackin-diagnostics`, `jackin-env`, `jackin-protocol`, `jackin-tui`, and TermRock. Must NOT depend on `jackin-runtime`, `jackin-launch`, or `jackin-capsule` — console reaches runtime through effects-as-data.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`workspace.rs`](src/workspace.rs) · [`workspace/`](src/workspace) | console workspace state | [`tests.rs`](src/workspace/tests.rs) |
| [`services.rs`](src/services.rs) · [`services/`](src/services) | services | — |
| [`github_mounts.rs`](src/github_mounts.rs) · [`github_mounts/`](src/github_mounts) | GitHub mounts | [`tests.rs`](src/github_mounts/tests.rs) |
| [`mount_info.rs`](src/mount_info.rs) · [`mount_info/`](src/mount_info) | mount info | [`tests.rs`](src/mount_info/tests.rs) |
| [`mount_info_cache.rs`](src/mount_info_cache.rs) | mount-info cache | — |
| [`mount_diff.rs`](src/mount_diff.rs) | mount diff | — |
| [`tui.rs`](src/tui.rs) · [`tui/`](src/tui) | chrome/input using TermRock and shared operator-info UI | [`input/`](src/tui/input) |
| [`tui/state.rs`](src/tui/state.rs) · [`tui/state/`](src/tui/state) | console manager state + bindings | [`tests.rs`](src/tui/state/tests.rs) |
| [`tui/state/manager.rs`](src/tui/state/manager.rs) · [`tui/state/manager/`](src/tui/state/manager) | concrete manager stage state | [`tests.rs`](src/tui/state/manager/tests.rs) |
| [`tui/screens/form_model.rs`](src/tui/screens/form_model.rs) | shared form `FieldRow` / `FormSection` view models | — |

## Public API

State machine and view models consumed by the `jackin` console entry point.

## How to verify

```sh
cargo nextest run -p jackin-console
cargo clippy -p jackin-console --all-targets -- -D warnings
```
