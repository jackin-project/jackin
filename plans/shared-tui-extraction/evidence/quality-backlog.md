# Bug-compatible extraction backlog

Source: donor audit and Decision 13, frozen at donor revision
`33896a504e19ef13adb8692550c1845cb86a9504`.

None of these defects may be fixed before jackin❯ render and behavior parity
passes. Each fix lands in TermRock after parity with regenerated fixtures, a
migration note, and a deliberate jackin❯ revision repin.

| ID | Frozen defect | Post-parity fix |
|---|---|---|
| WIDTH-HINT | `HintSpan::display_cols` uses character-count width math. | Measure terminal display columns and add combining-mark, ZWJ, CJK, regional-indicator, zero-width, and clipping cases. |
| WIDTH-LIST | Select-list label measurement uses character counts. | Use the canonical display-width/window implementation without changing selection semantics. |
| WIDTH-STATUS | Status-footer right-group layout uses character counts. | Measure slots in display columns and regenerate affected fixtures. |
| WIDTH-ERROR | Error-dialog wrapping uses character counts. | Wrap in display columns and document the visible diff. |
| FOCUS-PANEL | Panel focus is communicated only by border color. | Add a non-color glyph, border-character, or modifier cue. |
| COLOR-LAYERS | RGB constants, Ratatui colors, raw ANSI helpers, and the lookbook SVG hex table duplicate palette data. | Make semantic theme roles the canonical source and derive adapters. |
| RAW-OVERLAYS | Error and container-info dialogs return hyperlink overlays as raw post-frame byte vectors. | Return typed OSC hyperlink requests; keep emission policy consumer-owned. |
