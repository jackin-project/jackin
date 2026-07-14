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

Scheduled advisory lane: Hygiene job `dylint-advisory` (`continue-on-error: true`).
Real exit status is captured in the step summary (`CLEAN` vs `FINDINGS_OR_TOOL_FAILURE`); the job stays non-blocking.

## Pilot closure (plan 012, 2026-07-14)

| Item | Value |
|---|---|
| Lint | `render_thread_purity` (Warn) |
| UI corpus | `ui/render_blocks.rs` (positive), `ui/render_clean.rs` (negative), `ui/render_spawn_boundary.rs` (boundary) |
| UI false-positive rate | **0** against the committed corpus (UI tests document intended positives only) |
| Workspace FP rate | Not re-measured this session (requires full `cargo dylint --all -- --workspace` on a dylint-capable host); first scheduled hygiene artifact after this change is the living evidence |
| Decision | **Keep advisory (do not promote, do not retire)** until one scheduled run shows FP≈0 on the full workspace. Promotion to a pinned non-`\|\| true` PR lane requires that evidence; retirement requires sustained noise. |

## Architecture tier

**Build/CI tooling (isolated).** No jackin❯ runtime dependencies.
