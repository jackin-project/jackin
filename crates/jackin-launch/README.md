# jackin-launch

Launch cockpit TUI — the presentation surface for `jackin load`. Renders build/launch progress, launch output, and standalone dialogs during the container bootstrap flow.

## What this crate owns

- Launch progress rendering (`progress`) and launch output streaming (`launch_output`, `build_log`).
- A standalone-dialog sink (`standalone_dialog_sink`) and the launch TUI shell (`tui`).

## Architecture tier and allowed dependencies

**Presentation crate.** Allowed workspace dependencies include `jackin-core`, `jackin-diagnostics`, TermRock, and `jackin-build-meta`. No runtime or infrastructure dependencies — it renders progress events emitted by `jackin-runtime`; it does not orchestrate.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`progress.rs`](src/progress.rs) · [`progress/`](src/progress) | `LaunchProgress` stage machine + test renderer | [`tests.rs`](src/progress/tests.rs) |
| [`launch_output.rs`](src/launch_output.rs) | launch output streaming | — |
| [`build_log.rs`](src/build_log.rs) | build-log streaming | — |
| [`standalone_dialog_sink.rs`](src/standalone_dialog_sink.rs) · [`standalone_dialog_sink/`](src/standalone_dialog_sink) | standalone dialog sink | [`tests.rs`](src/standalone_dialog_sink/tests.rs) |
| [`tui.rs`](src/tui.rs) · [`tui/`](src/tui) | launch cockpit view (TermRock + `jackin-tui` operator-info) | [`tests.rs`](src/tui/tests.rs) |

## Public API

The launch-cockpit entry point consumed by `jackin-runtime`'s launch flow. It composes TermRock primitives with launch-specific wording, animation, and output policy.

## How to verify

```sh
cargo nextest run -p jackin-launch
cargo clippy -p jackin-launch --all-targets -- -D warnings
```
