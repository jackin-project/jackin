# jackin-tui Component Inventory

This inventory tracks repeatable terminal UI patterns, their current owner, call sites, and maturity. New TUI work should name the existing component it uses or add a row before introducing a new repeated pattern.

| Component / pattern | Owner | Current call sites | Maturity | Notes |
|---|---|---|---|---|
| Design tokens | `jackin_tui` root + `jackin_tui::theme` | Host console, launch progress, capsule ANSI renderers | 3 — shared Ratatui adapter | RGB tokens remain backend-neutral; `theme` adapts them to Ratatui `Color`. |
| `HintBar` | `jackin_tui::components::hint_bar` | Console footer facade, launch dialogs/build-log overlay | 3 — shared Ratatui widget | Capsule still renders `HintSpan` through raw ANSI until the capsule Ratatui frame lands. |
| `StatusFooter` | `jackin_tui::components::status_footer` | Launch progress footer | 3 — shared Ratatui widget | Replaces the former console-only `status_bar` helper; capsule bottom bar still has raw ANSI chrome. |
| `BrandHeader` | `jackin_tui::components::brand_header` | Console brand-header facade | 3 — shared Ratatui widget | Capsule status bar has a raw ANSI brand pill until its chrome moves to Ratatui. |
| `FilterInput` | `jackin_tui::components::filter_input` | Console select-list facade | 3 — shared Ratatui widget | Next picker extraction should consume this directly rather than drawing filter rows locally. |
| `TextField` / `TextInput` | `jackin_tui::TextField`, `jackin_tui::components::text_input` | Console text input, launch text prompt, capsule rename dialog model | 3 — shared Ratatui widget | Capsule still uses the shared model through raw ANSI until its Ratatui frame lands. |
| `TabCell` / tab layout | `jackin_tui` root | Console tab strips, capsule status bar | 2 — shared model | Ratatui `TabStrip` component still needs promotion. |
| Scroll metrics | `jackin_tui::scroll` | Console scrollable blocks, launch build-log overlay, capsule scroll math | 2 — shared model | Ratatui `ScrollablePanel` component still needs promotion. |
| `ModalOutcome` | `jackin_tui::ModalOutcome` | Console widgets, launch forced-choice prompts | 2 — shared update contract | Event vocabulary is shared; modal components are still per-surface. |
| Confirm dialog | `src/console/widgets/confirm.rs` and `crates/jackin-capsule/src/dialog.rs` | Console, launch, capsule | 1 — local helpers | Promote to one `ConfirmDialog` before further reuse. |
| Error dialog | `src/console/widgets/error_popup.rs` | Console, launch | 1 — local helper | Runtime still imports the console widget; move next. |
| Filter list picker | `src/console/widgets/select_list.rs` plus picker-specific modules | Console, launch | 1 — local helper | Promote as generic `FilterListPicker<T>` with typed row ids. |
