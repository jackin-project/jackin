# jackin-launch-tui

Launch cockpit TUI — the presentation surface for `jackin load`. Renders build/launch progress, launch output, and standalone dialogs during the container bootstrap flow.

## What this crate owns

- Launch progress rendering (`progress`) and launch output streaming (`launch_output`, `build_log`).
- A standalone-dialog sink (`standalone_dialog_sink`) and the launch TUI shell (`tui`).

## Architecture tier and allowed dependencies

**Presentation crate.** Allowed workspace dependencies: `jackin-core`, `jackin-diagnostics`, `jackin-tui`, `jackin-build-meta`. No runtime or infrastructure dependencies — it renders progress events emitted by `jackin-runtime`; it does not orchestrate.

## Structure

- `src/progress.rs` / `src/progress/` — build/launch progress rendering
- `src/launch_output.rs`, `src/build_log.rs` — output streaming
- `src/standalone_dialog_sink.rs` / `src/standalone_dialog_sink/` — standalone dialog sink
- `src/tui.rs` / `src/tui/` — launch TUI shell

## Public API

The launch-cockpit entry point consumed by `jackin-runtime`'s launch flow. Renders progress through `jackin-tui`'s design-system components, not bespoke widgets.

## How to verify

```sh
cargo nextest run -p jackin-launch-tui
cargo clippy -p jackin-launch-tui --all-targets -- -D warnings
```

