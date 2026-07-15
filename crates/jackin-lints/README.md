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
- **Pinned nightly** via `rust-toolchain` (dylint compiles against rustc-private).
- Main-workspace `cargo check --workspace` must never compile this crate.

## How to run

```sh
# Build the lint library (uses the crate's nightly pin):
cd crates/jackin-lints && cargo build

# UI tests:
cd crates/jackin-lints && cargo test

# Against the main workspace (from repo root; requires cargo-dylint):
cargo dylint --all -- --workspace
```

Scheduled enforced lane: Hygiene job `dylint-advisory`. Pinned 6.0.1 tools are
source-built (upstream prebuilt embeds its release-builder `dylint_driver`
path). Real exit status is in the step summary and fails the job on findings
or tool failure.

## Pilot closure (plan 012, 2026-07-14)

| Item | Value |
|---|---|
| Lint | `render_thread_purity` (Warn) |
| UI corpus | `ui/render_blocks.rs` (positive), `ui/render_clean.rs` (negative), `ui/render_spawn_boundary.rs` (boundary) |
| UI false-positive rate | **0** against the committed corpus (UI tests document intended positives only) |
| Workspace FP rate | **0% (0 false positives / 0 findings)** from `cargo dylint --all -- --workspace`, 2026-07-15; the committed positive UI fixture supplies the non-zero signal check |
| Decision | **Promote** to the pinned, exit-status-enforced Hygiene lane. Any workspace finding or Dylint tool failure now fails the scheduled job; the UI corpus remains mandatory. |

## Architecture tier

**Build/CI tooling (isolated).** No jackin❯ runtime dependencies.
