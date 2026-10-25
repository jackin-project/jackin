# jackin-lints

Workspace-owned [dylint](https://github.com/trailofbits/dylint) library for
jackin❯-specific invariants that Clippy cannot express structurally.

## What this crate owns

- **`render_thread_purity`** — flags blocking I/O / process / `std::sync` locks
  reachable from render-path functions (`render`, `compose_pending_frame`,
  `compose_ratatui_frame`) via a bounded local call-graph walk.

## Isolation rules

- **Not a workspace member.** Listed under the workspace root exclude list and has
  its own `[workspace]` table.
- **Pinned nightly:** `jackin-xtask custom-lints` builds Dylint against rustc-private.
  No nested Rust toolchain file or directory override changes the root compiler.
- Main-workspace `cargo check --workspace` must never compile this crate.

## How to run

Run `mise run check:custom-lints`. [Running and qualification](RUNNING.md)
documents the explicit nightly, isolated prebuilt-library discovery, and current
upstream workspace-check blocker.

## Pilot closure (plan 012, 2026-07-14)

| Item | Value |
|---|---|
| Lint | `render_thread_purity` (Warn) |
| UI corpus | `ui/render_blocks.rs` (positive), `ui/render_clean.rs` (negative), `ui/render_spawn_boundary.rs` (boundary) |
| UI false-positive rate | **0** against the committed corpus (UI tests document intended positives only) |
| Workspace FP rate | **0% (0 false positives / 0 findings)** from the former metadata-autobuild command, 2026-07-15; the committed positive UI fixture supplies the non-zero signal check |
| Decision | **Pilot only**. The earlier promotion proposal was not wired into the current generated workflow; no scheduled or required Dylint lane is active. |

## Architecture tier

**Build/CI tooling (isolated).** No jackin❯ runtime dependencies.
