# jackin-tui Component Inventory

This inventory tracks repeatable terminal UI patterns, their current owner, call sites, and maturity. New TUI work should name the existing component it uses or add a row before introducing a new repeated pattern.

Renderable component stories live in `crates/jackin-tui-lookbook`. Run
`cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook` to render those
stories from the real `TestBackend` buffers into the docs SVG previews consumed
by `/reference/tui-lookbook/`. CI checks the committed previews with
`cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook`. Use
`cargo run -p jackin-tui-lookbook -- --terminal` for the interactive catalog.

| Component / pattern | Owner | Current call sites | Maturity | Notes |
|---|---|---|---|---|
| Design tokens | `jackin_tui` root + `jackin_tui::theme` | Host console, launch progress, capsule ANSI renderers | 3 — shared Ratatui adapter | RGB tokens remain backend-neutral; `theme` adapts them to Ratatui `Color`. |
| `HintBar` | `jackin_tui::components::hint_bar` | Console footer facade, launch dialogs/build-log overlay | 3 — shared Ratatui widget | Capsule still renders `HintSpan` through raw ANSI until the capsule Ratatui frame lands. |
| `StatusFooter` | `jackin_tui::components::status_footer` | Launch progress footer | 3 — shared Ratatui widget | Replaces the former console-only `status_bar` helper; capsule bottom bar still has raw ANSI chrome. |
| `BrandHeader` | `jackin_tui::components::brand_header` | Console brand-header facade | 3 — shared Ratatui widget | Capsule status bar has a raw ANSI brand pill until its chrome moves to Ratatui. |
| `FilterInput` | `jackin_tui::components::filter_input` | Console select-list facade | 3 — shared Ratatui widget | Next picker extraction should consume this directly rather than drawing filter rows locally. |
| `TextField` / `TextInput` | `jackin_tui::TextField`, `jackin_tui::components::text_input` | Console text input, launch text prompt, capsule rename dialog | 3 — shared Ratatui widget | Launch uses the shared one-label prompt rect + renderer; capsule rename uses the shared titled/labelled Ratatui dialog helper. |
| `Toast` | `jackin_tui::components::toast` | Capsule pane selection copied feedback | 2 — shared overlay primitive | Use for transient state feedback that must not replace footer/action hints; callers supply reserved bottom rows so status chrome stays visible. |
| `ButtonStrip` | `jackin_tui::components::button_strip` | Confirm dialog, confirm-save modal, scope picker, source picker, mount-destination choice, save/discard modal | 3 — shared Ratatui primitive | New button rows should consume this instead of declaring focused/unfocused button styles locally; capsule menus still need the Ratatui migration. |
| `Panel` | `jackin_tui::components::panel` | Shared scrollable panel | 2 — shared primitive | Dialogs still build blocks directly; migrate them onto `Panel` as their props are normalized. |
| `TabStrip` / `TabCell` | `jackin_tui::components::tab_strip`, `jackin_tui` root | Console editor/settings tab strips, capsule status bar geometry | 3 — shared Ratatui primitive + model | Host uses the Ratatui component; capsule still consumes the shared `TabCell` layout through raw ANSI until its Ratatui frame lands. |
| `ScrollablePanel` / scroll metrics | `jackin_tui::components::scrollable_panel`, `jackin_tui::scroll` | Console scrollable blocks, launch build-log overlay, capsule scroll math | 3 — shared Ratatui widget + model | Capsule consumes only the scroll math until its Ratatui frame lands. |
| `ModalOutcome` | `jackin_tui::ModalOutcome` | Console widgets, launch forced-choice prompts | 2 — shared update contract | Event vocabulary is shared; composed modal flows still need one runtime loop per surface. |
| `ConfirmDialog` | `jackin_tui::components::confirm_dialog` | Console, launch | 3 — shared Ratatui widget | Capsule still redraws confirm actions in raw ANSI until its Ratatui frame lands. |
| Error dialog | `jackin_tui::components::error_dialog` | Console, launch | 3 — shared Ratatui widget | Capsule still needs a matching error surface once it moves to Ratatui. |
| Filter list picker | `jackin_tui::components::select_list` plus picker-specific modules | Console GitHub / role / workdir / 1Password pickers, launch | 3 — shared picker renderer + per-surface state | Rich host pickers feed neutral row content into the shared renderer so the component owns the `▸` gutter, full-width highlight, selection-follow, and scrollbar-gutter stop. A future typed `FilterListPicker<T>` can still reduce state boilerplate, but callers must not pre-style selection. |
