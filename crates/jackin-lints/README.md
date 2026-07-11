# jackin-lints

Workspace-owned [dylint](https://github.com/trailofbits/dylint) library for
jackin❯-specific invariants that Clippy cannot express structurally.

## What this crate owns

- **`render_thread_purity`** — flags blocking I/O / process / `std::sync` locks
  reachable from render-path functions (`render`, `compose_pending_frame`,
  `compose_ratatui_frame`) via a bounded local call-graph walk.

## Isolation rules

- **Not a workspace member.** Listed under root `Cargo.toml` `exclude` and has
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

## Architecture tier

**Build/CI tooling (isolated).** No jackin❯ runtime dependencies.
