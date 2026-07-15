## Hard rule: Use only jackin-tui public API (no exceptions)

Every story and interactor **must** call the same public API the rest of the jackin❯ ecosystem uses. Non-negotiable.

**In practice:**
- Call the same `render_*()` helper the console or capsule calls — not the underlying widget constructor.
- Use the same `*State::handle_key()` callers use — not custom key dispatch reimplementing it.
- Use the same `*State::new()` constructor and public builder methods — not internal fields.
- If a story needs something `jackin-tui` doesn't expose publicly, **stop and fix the API first**. The story reveals an API gap, not a reason to add workarounds.

**Why:**
Lookbook is the reference implementation. A developer using `SelectList` or `TextInput` in a new surface reads the story and copies the API call. If the story uses `pub(crate)` internals or custom wrappers, the developer learns the wrong pattern and the component accumulates incompatible usage sites.

**Violation triggers:**
- Accessing `pub(crate)` or private fields of any `jackin-tui` type.
- Constructing `ratatui::widgets::*` types (Block, Paragraph, etc.) the component normally constructs internally.
- Duplicating logic from a component's `handle_key` instead of delegating.
- Adding `#[allow(...)]` to suppress lints from non-API usage.

**On finding a violation:**
1. Identify the missing public API (e.g. `ConfirmState::with_focus_no()`).
2. Add the method to `jackin-tui` in the same PR.
3. Update the story to call the new method.
4. Verify `cargo clippy -p jackin-tui-lookbook -- -D warnings` produces no errors.

## Story structure

Each story is a `fn story_*(frame, area)` that:
1. Constructs the component state via the public constructor.
2. Calls the public `render_*()` helper or `Widget::render()` — whichever matches real app usage.

Each interactor is a struct implementing `StoryInteraction` that:
1. Holds a real `*State` field (same type the real app holds).
2. Calls `state.handle_key(key)` directly — no custom key routing.
3. Calls the same render function as the static story.

## Adding a new component story

1. Add the component to `jackin-tui` following its existing component conventions.
2. Add a story function `fn story_<component>_<variant>` in `src/stories.rs`.
3. Register it in `stories()` with a `Story::new(...)` entry.
4. If interactive, add a `*Interactor` struct in `src/interactors.rs` and register in `make_interactor()`.
5. Run `cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook` to regenerate SVG previews.
6. Run `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` to verify no drift.
