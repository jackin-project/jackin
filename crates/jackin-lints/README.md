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
cd crates/jackin-lints && cargo nextest run

# Against the main workspace (from repo root; requires cargo-dylint):
cargo dylint --all -- --workspace
```

## Enforcement status

The current generated CI workflow has no Dylint lane. `.velnor/config.toml`
excludes this isolated crate from discovery. Installing the Dylint tools does
not run the lint. Restore an enforced generator-owned lane that runs the UI
corpus and checks the main workspace; findings and tool failures must fail it.
The lint crate must remain outside the main workspace.

## Regression corpus

The UI corpus covers free-function and method helper edges, loop `let`
initializers, match guards, and `let ... else` blocks. `render_clean.rs` and
`render_spawn_boundary.rs` retain the negative and closure-boundary cases.
The traversal uses rustc's HIR visitor for expression children and one resolved
call handler for both local functions and methods. Its local graph depth remains
bounded at five edges; closures remain explicit graph boundaries.

The new fixture expectations require execution on the pinned nightly before
claiming validation. Historical 2026-07-15 reports of zero workspace findings
predate these regressions and cannot establish current coverage or CI enforcement.

## Architecture tier

**Build/CI tooling (isolated).** No jackin❯ runtime dependencies.
