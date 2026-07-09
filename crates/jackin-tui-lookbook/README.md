# jackin-tui-lookbook

Interactive component lookbook for `jackin-tui` — the reference rendering of every shared TUI component in its real state. This is where a developer (or agent) copies the canonical API call for a component, and where SVG previews for the docs are generated.

## What this crate owns

- A `story_*()` per component variant, each calling the **public** `render_*()` helper or `Widget::render` exactly as the real surfaces do.
- Interactive `*Interactor` structs that drive a real component `*State` through `handle_key`, matching real-app usage.
- The SVG preview generator that feeds `docs/public/tui-lookbook/`.

## Architecture tier and allowed dependencies

**Presentation / dev-tool crate.** Allowed workspace deps: `jackin-tui`. It depends on nothing else because it must call only `jackin-tui`'s public API — that is its whole purpose.

## Structure

- `src/stories.rs` / `src/stories/` — one story per component variant
- `src/interactors.rs` — interactive state drivers
- `src/svg.rs` — SVG preview rendering for the docs
- `src/main.rs` — lookbook runner + `--check` drift gate

## Public API

None consumed; this crate is a consumer of `jackin-tui`. Its own surface is the lookbook binary (`cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook` to regenerate, `-- --check …` to verify no drift).

## How to verify

```sh
cargo nextest run -p jackin-tui-lookbook
cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook
```

The hard rule — *use only `jackin-tui` public API* — applies to every story and interactor.
