# Plan 001: Launch failure scroll-aware copy and hyperlink geometry

## Status

- **Execution status**: DONE
- **Priority**: P1
- **Effort**: S
- **Risk**: MEDIUM
- **Category**: bug

## Problem

The launch failure popup renders long failure bodies with `view.failure_scroll`, but mouse copy/hover hit-testing and OSC8 hyperlink overlay generation rebuild the failure popup state with no scroll.

Affected paths:

- `crates/jackin-launch-tui/src/tui/components/failure_dialog.rs`
- `crates/jackin-launch-tui/src/tui/subscriptions.rs`
- `crates/jackin-launch-tui/src/tui/view.rs`
- `crates/jackin-runtime/src/runtime/progress.rs`

Concrete drift:

- `render_failure_popup(...)` calls `failure_error_state(..., Some(view))`, so rows render at `view.failure_scroll.scroll_y`.
- `failure_copy_target_at(...)` calls `failure_error_state(..., None)`, so clicked rows are measured at scroll 0.
- `failure_popup_hyperlink_overlay(...)` calls `failure_error_state_with_feedback(..., None, ...)`, so OSC8 regions are emitted for scroll 0 even after the visible popup has scrolled.

Result: after the operator scrolls a long launch failure popup, hover/click/copy and terminal hyperlink overlays can target rows that are no longer visible, or miss rows that are visible.

## Fix Plan

1. Pass the current `DialogBodyScroll` into failure popup geometry helpers that depend on visible row positions.
2. Update mouse click and hover routing to use `view.failure_scroll`.
3. Update failure popup OSC8 overlay generation to use the same scroll state as rendering.
4. Keep scroll-independent helpers, such as popup block rect and body metrics, unchanged unless a test proves they need scroll.
5. Add regression tests covering:
   - a long failure body with `failure_scroll.scroll_y > 0`;
   - hover/copy hit-testing follows the visible scrolled rows;
   - OSC8 overlay row positions follow the same scrolled layout.

## Verification

```sh
rtk cargo test -p jackin-launch-tui failure_dialog
rtk cargo test -p jackin-launch-tui subscriptions
rtk cargo test -p jackin-runtime failure_copy_target
```

Before merge, also run the PR's relevant launch TUI suite:

```sh
rtk cargo test -p jackin-launch-tui -p jackin-runtime
```

