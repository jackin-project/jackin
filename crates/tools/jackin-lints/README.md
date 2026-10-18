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
cd crates/tools/jackin-lints
# Dylint's nested driver build must resolve Cargo through rustup's shims.
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
rustup run nightly-2026-10-18 cargo build --locked
rustup run nightly-2026-10-18 cargo nextest run --locked
```

Against the main workspace, from the repository root (requires cargo-dylint):

```sh
cargo dylint --all -- --workspace
```

The qualified compiler is `nightly-2026-10-18` (`rustc 1.101.0-nightly`,
commit `8006091eb`, 2026-10-17). The date names the published nightly manifest;
the compiler commit date is one day earlier. `rustup run` makes the lint's
nightly take precedence over a parent workspace's Mise-selected stable Cargo.
The shim-first PATH also lets Dylint's isolated driver build select its own
written toolchain file after Dylint removes inherited compiler environment
variables. Direct compiler-toolchain binaries or a Mise Cargo wrapper ahead of
the shims would bypass that selection.

The UI test runs Dylint 6.2.0's supported compiletest harness inside Nextest and
checks the positive, clean, and closure-boundary fixtures.

Scheduled enforced lane: Hygiene job `dylint-advisory`. Pinned 6.2.0 tools are
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
