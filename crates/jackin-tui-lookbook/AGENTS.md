# AGENTS.md — jackin-tui-lookbook

This crate is the interactive component lookbook for `jackin-tui`. It contains stories that preview every shared TUI component in its real rendered state.

## Hard rule: Use only jackin-tui public API (no exceptions)

Every story and interactor in this crate **must** call the same public API that the rest of the jackin ecosystem uses. This is non-negotiable.

**What this means in practice:**
- Call the same `render_*()` helper function that the console or capsule calls — not the underlying widget constructor.
- Use the same `*State::handle_key()` method that callers use — not custom key dispatch that reimplements it.
- Use the same `*State::new()` constructor and public builder methods — not internal fields.
- If a story needs something and `jackin-tui` does not expose it publicly, **stop and fix the API first** before writing the story. The story reveals a gap in the library's design, not a reason to add workarounds.

**Why this matters:**
The lookbook is the reference implementation. When a developer needs to use `SelectList` or `TextInput` in a new surface, they read the lookbook story and copy the API call. If the story uses `pub(crate)` internals or custom wrappers, the developer learns the wrong pattern and the component accumulates incompatible usage sites.

**What triggers a violation:**
- Accessing `pub(crate)` or private fields of any `jackin-tui` type.
- Constructing `ratatui::widgets::*` types (Block, Paragraph, etc.) that the component normally constructs internally.
- Duplicating logic from a component's `handle_key` instead of delegating to it.
- Adding `#[allow(...)]` to suppress lint warnings caused by non-API usage.

**When you find a violation:**
1. Identify the missing public API (e.g. `ConfirmState::with_focus_no()`).
2. Add the method to `jackin-tui` in the same PR.
3. Update the story to call the new method.
4. Verify `cargo clippy -p jackin-tui-lookbook -- -D warnings` produces no errors.

## Story structure

Each story is a `fn story_*(frame, area)` function that:
1. Constructs the component state using the public constructor.
2. Calls the public `render_*()` helper or `Widget::render()` — whichever matches real app usage.

Each interactor is a struct implementing `StoryInteraction` that:
1. Holds a real `*State` field (the same type the real app holds).
2. Calls `state.handle_key(key)` directly — no custom key routing.
3. Calls the same render function as the static story.

## Adding a new component story

1. Add the component to `jackin-tui` following its existing component conventions.
2. Add a story function `fn story_<component>_<variant>` in `src/stories.rs`.
3. Register it in `stories()` with a `Story::new(...)` entry.
4. If interactive, add a `*Interactor` struct in `src/interactors.rs` and register in `make_interactor()`.
5. Run `cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook` to regenerate SVG previews.
6. Run `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` to verify no drift.
