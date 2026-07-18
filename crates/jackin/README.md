# jackin

The jackin❯ CLI — the operator-facing binary that loads roles into isolated containers, drives the console, and wires the workspace together. This crate is the L4 entry/glue layer: `lib.rs` re-exports the module tree consumed by `main.rs`, `src/bin/role.rs`, and integration tests; the crate is simultaneously a library and a binary.

## What this crate owns

- The CLI surface (`cli`) and top-level application wiring (`app`), error type (`error`), and preflight checks (`preflight`).
- Console entry (`console`), launch/prompt flow (`prompt`), and the Warp integration (`warp`).
- Role-authoring (`role_authoring`, `role_claude_plugins`) and workspace commands (`workspace`).

## Architecture tier and allowed dependencies

**L4 entry/glue crate.** It depends on the whole stack, including TermRock-backed presentation crates. Nothing depends on this crate; it is the top of the graph. Keep it thin — it assembles, it does not implement domain logic. Product-owned terminal formatting and ownership policy now live with their CLI callers instead of the retired shared donor modules.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | library re-exports | — |
| [`main.rs`](src/main.rs) | binary entry | — |
| [`cli.rs`](src/cli.rs) · [`cli/`](src/cli) | CLI parsing, including diagnostics validation (no local logs command) | [`tests.rs`](src/cli/tests.rs) |
| [`app.rs`](src/app.rs) · [`app/`](src/app) | app wiring | [`tests.rs`](src/app/tests.rs) |
| [`console.rs`](src/console.rs) · [`console/`](src/console) | console entry and dependency-crossing host adapters | — |
| [`prompt.rs`](src/prompt.rs) · [`prompt/`](src/prompt) | prompt flow | [`tests.rs`](src/prompt/tests.rs) |
| [`preflight.rs`](src/preflight.rs) · [`preflight/`](src/preflight) | preflight checks | [`tests.rs`](src/preflight/tests.rs) |
| [`workspace.rs`](src/workspace.rs) · [`workspace/`](src/workspace) | workspace commands | [`tests.rs`](src/workspace/tests.rs) |
| [`role_authoring.rs`](src/role_authoring.rs) · [`role_authoring/`](src/role_authoring) | role-authoring commands | [`tests.rs`](src/role_authoring/tests.rs) |
| [`role_claude_plugins.rs`](src/role_claude_plugins.rs) · [`role_claude_plugins/`](src/role_claude_plugins) | role Claude-plugins commands | [`tests.rs`](src/role_claude_plugins/tests.rs) |
| [`warp.rs`](src/warp.rs) | Warp integration | — |
| [`error.rs`](src/error.rs) | top-level error type | — |
| [`bin/`](src/bin) | extra binaries | — |

## Public API

The `jackin` binary (`jackin`, `jackin load`, `jackin console`, …) plus the library re-exports used by integration tests. New user-visible flags/commands land here (CLI-first), then optionally surface in the console.

## How to verify

```sh
cargo nextest run -p jackin
cargo clippy -p jackin --all-targets -- -D warnings
```
