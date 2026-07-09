# jackin-launch-tui

Launch cockpit TUI — the presentation surface for `jackin load`. Renders build/launch progress, launch output, and standalone dialogs during the container bootstrap flow.

## What this crate owns

- Launch progress rendering (`progress`) and launch output streaming (`launch_output`, `build_log`).
- A standalone-dialog sink (`standalone_dialog_sink`) and the launch TUI shell (`tui`).

## Architecture tier and allowed dependencies

**Presentation crate.** Allowed workspace dependencies: `jackin-core`, `jackin-diagnostics`, `jackin-tui`, `jackin-build-meta`. No runtime or infrastructure dependencies — it renders progress events emitted by `jackin-runtime`; it does not orchestrate.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`progress.rs`](src/progress.rs) · [`progress/`](src/progress) | build/launch progress rendering | [`tests.rs`](src/progress/tests.rs) |
| [`launch_output.rs`](src/launch_output.rs) | launch output streaming | — |
| [`build_log.rs`](src/build_log.rs) | build-log streaming | — |
| [`standalone_dialog_sink.rs`](src/standalone_dialog_sink.rs) · [`standalone_dialog_sink/`](src/standalone_dialog_sink) | standalone dialog sink | [`tests.rs`](src/standalone_dialog_sink/tests.rs) |
| [`tui.rs`](src/tui.rs) · [`tui/`](src/tui) | launch TUI shell | — |

## Public API

The launch-cockpit entry point consumed by `jackin-runtime`'s launch flow. Renders progress through `jackin-tui`'s design-system components, not bespoke widgets.

## How to verify

```sh
cargo nextest run -p jackin-launch-tui
cargo clippy -p jackin-launch-tui --all-targets -- -D warnings
```

