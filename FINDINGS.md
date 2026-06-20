# FINDINGS.md

Open work only. Completed items removed. Each finding states the root cause and the structural fix required, not a patch at the symptom layer.

---

## Keymap / Hint-Bar Safety

**Context.** `Keymap<A>` makes hint/dispatch divergence architecturally impossible *within a single keymap table*: `dispatch()` and `hint_spans()` both read from `self.bindings`, so a `Visibility::Shown` binding is always both dispatched and advertised. That guarantee is real. Console and Launch surfaces already follow this pattern after the keymap unification in this branch.

**Root cause of all violations below.** `main_view_hint()` and the prefix cheat sheet in `crates/jackin-capsule/src/tui/components/dialog/hint.rs` were hand-authored rather than derived from the keymaps that own dispatch. This creates a class of bugs: any time dispatch logic changes, the hint must be manually kept in sync, and the compiler cannot catch the drift.

**The fix class is universal and must be applied to every surface, every dialog, every mode in the codebase — Capsule, Console, Launch — with no exceptions.** The rule:

> Every hint bar must be constructed by calling `.hint_spans()` or `.glyph_for()` on the `Keymap<A>` that owns dispatch for that surface. No `HintSpan::Key(…)` or `HintSpan::Text(…)` literal may appear in a hint builder unless it represents a genuinely unregisterable input (multi-byte CSI sequence or mouse — both must be marked with an explicit `// UNREGISTERABLE` comment naming the reason).

**Canonical pattern (already in use for Console and Launch — all Capsule surfaces must match):**

```rust
// 1. Define action enum
pub enum SurfaceAction { Quit, Navigate, /* … */ }

// 2. Register keys — single source of truth for both dispatch and hints
pub static SURFACE_KEYMAP: Keymap<SurfaceAction> = Keymap::new(&[
    KeyBinding {
        chords:     &[KeyChord::ctrl(LogicalKey::Char('q'))],
        action:     SurfaceAction::Quit,
        hint:       Some("quit"),
        visibility: Visibility::Shown,
        glyph:      None,  // auto-derives "Ctrl-Q"
    },
    // Alias chords: same action, not shown in hint bar
    KeyBinding {
        chords:     &[KeyChord::plain(LogicalKey::Char('k')),
                      KeyChord::plain(LogicalKey::Up)],
        action:     SurfaceAction::Navigate,
        hint:       Some("navigate"),
        visibility: Visibility::HiddenAlias,  // dispatches, grouped under primary
        glyph:      None,
    },
]);

// 3. Input handler — dispatch through keymap only
fn handle_key(chord: KeyChord) -> Option<Event> {
    SURFACE_KEYMAP.dispatch(chord).map(|action| match action {
        SurfaceAction::Quit     => Event::RequestExit,
        SurfaceAction::Navigate => Event::MoveUp,
    })
    // No raw byte checks, no parallel match arms outside the keymap.
}

// 4. Hint builder — derive from same keymap only
pub fn surface_hint_spans(/* context if needed */) -> Vec<HintSpan<'static>> {
    SURFACE_KEYMAP.hint_spans()
    // For filtered subsets: SURFACE_KEYMAP.hint_spans_filtered(|b| …)
}
```

**For global keys that apply across all Capsule surfaces (Ctrl+Q):** register once in `CAPSULE_GLOBAL_KEYMAP`, append to every surface's hint builder. No surface may hand-write `HintSpan::Key("Ctrl-Q")`.

**For palette key (runtime-configured `JACKIN_PALETTE_KEY`):** cannot be a static `Keymap` entry (chord determined at runtime). Resolved byte must flow into the hint builder at call time and be formatted with `format_key_glyph(byte)`. Never hardcode `"Ctrl+\\"`.

---

### 1 — Ctrl+Q: dispatched outside any keymap

**Dispatch:** `crates/jackin-capsule/src/tui/input.rs:351` — `else if b == CTRL_Q { events.push(InputEvent::RequestExit); }` where `CTRL_Q: u8 = 0x11` (line 533).

**Hints:** `crates/jackin-capsule/src/tui/components/dialog/hint.rs:30, 45, 62` — three separate hardcoded `HintSpan::Key("Ctrl-Q")` spans in prefix, scrollback, and normal modes.

**Root cause:** Ctrl+Q predates the keymap registry. It is a single-byte chord fully representable in `Keymap<u8>` but never added. Three manual hint spans must be maintained separately from dispatch.

**Fix:**

```rust
// crates/jackin-capsule/src/tui/keymap.rs  (ADD)
pub enum GlobalCapsuleAction { RequestExit }

pub static CAPSULE_GLOBAL_KEYMAP: Keymap<GlobalCapsuleAction> = Keymap::new(&[
    KeyBinding {
        chords:     &[KeyChord::ctrl(LogicalKey::Char('q'))],
        action:     GlobalCapsuleAction::RequestExit,
        hint:       Some("quit"),
        visibility: Visibility::Shown,
        glyph:      None,
    },
]);

// crates/jackin-capsule/src/tui/input.rs  (CHANGE)
// Remove: const CTRL_Q: u8 = 0x11;  and the raw else-if arm.
// Add before other dispatch:
if let Some(action) = CAPSULE_GLOBAL_KEYMAP.dispatch(KeyChord::from_byte(b)) {
    match action {
        GlobalCapsuleAction::RequestExit => events.push(InputEvent::RequestExit),
    }
}

// crates/jackin-capsule/src/tui/components/dialog/hint.rs  (CHANGE)
// Remove the three HintSpan::Key("Ctrl-Q") literals.
// In every hint builder fn, append: spans.extend(CAPSULE_GLOBAL_KEYMAP.hint_spans());
```

### 2 — Prefix cheat sheet: manual hints, semantic error

**File:** `crates/jackin-capsule/src/tui/components/dialog/hint.rs:19–32`

`main_view_hint()` returns a hardcoded `vec![…]` when `prefix_awaiting == true`. Errors:
- `"n/c"` / `"new/close"`: `'n'` is `NextTab` (not "new"); `'x'` is `KillPane` ("close") but hidden
- `"Ctrl-Q"` hardcoded (covered by item 1 above)

**Root cause:** Hints hand-written instead of derived from `PREFIX_COMMAND_KEYMAP`.

**Fix:**

```rust
// crates/jackin-capsule/src/tui/keymap.rs  (CHANGE existing PREFIX_COMMAND_KEYMAP)
// For grouped display (h/j/k/l), mark primary binding Shown with grouped glyph;
// alias bindings HiddenAlias so they dispatch but don't each emit a span:
KeyBinding {
    chords: &[KeyChord::plain(LogicalKey::Char('h'))],
    action: PrefixCommand::MoveFocus(Direction::Left),
    hint: Some("nav"),
    visibility: Visibility::Shown,
    glyph: Some("h/j/k/l"),  // grouped glyph
},
KeyBinding {
    chords: &[KeyChord::plain(LogicalKey::Char('j'))],
    action: PrefixCommand::MoveFocus(Direction::Down),
    hint: None,
    visibility: Visibility::HiddenAlias,  // dispatches, hidden from hint bar
    glyph: None,
},
// … same for k, l
//
// 'x' KillPane must be Shown (was missing from hand-written hints):
KeyBinding {
    chords: &[KeyChord::plain(LogicalKey::Char('x'))],
    action: PrefixCommand::KillPane,
    hint: Some("close"),
    visibility: Visibility::Shown,
    glyph: None,
},
// space/: palette: Visibility::Internal (dispatches, dynamic hint added separately)

// crates/jackin-capsule/src/tui/components/dialog/hint.rs  (CHANGE)
// Replace the entire hardcoded prefix branch:
if prefix_awaiting {
    let mut spans = PREFIX_COMMAND_KEYMAP.hint_spans();      // all prefix keys
    spans.extend(CAPSULE_GLOBAL_KEYMAP.hint_spans());        // Ctrl+Q
    spans.push(HintSpan::Dyn(format_key_glyph(palette_key))); // dynamic palette
    spans.push(HintSpan::Text("palette"));
    return spans;
}
```

### 3 — Palette key: dynamic dispatch, hardcoded hint

**Dispatch:** `crates/jackin-capsule/src/tui/input.rs:343` — `if Some(b) == self.palette_key`. Default `0x1C` (`Ctrl+\`), overridable via `JACKIN_PALETTE_KEY`.

**Hints:** `crates/jackin-capsule/src/tui/components/dialog/hint.rs:42, 49` — hardcode `HintSpan::Key("Ctrl+\\")`. Wrong when `JACKIN_PALETTE_KEY` overrides the default.

**Root cause:** Static `Keymap<u8>` cannot represent a runtime-configured key. Hint hardcoded to default, making it wrong on any override.

**Fix:**

```rust
// crates/jackin-capsule/src/tui/components/dialog/hint.rs  (ADD + CHANGE)

// Add this formatter (inverse of parse_key_binding):
fn format_key_glyph(byte: u8) -> String {
    match byte {
        0x01..=0x1A => format!("Ctrl-{}", (b'@' + byte) as char),
        0x1C         => "Ctrl-\\".to_owned(),
        _            => format!("0x{byte:02X}"),
    }
}

// Change every hint builder signature to accept palette_key: u8, e.g.:
pub fn main_view_hint(
    prefix_awaiting: bool,
    palette_key: u8,      // resolved byte from self.palette_key in input parser
    axes: ScrollAxes,
) -> Vec<HintSpan<'static>> { … }

// Replace HintSpan::Key("Ctrl+\\") at lines 42 and 49 with:
HintSpan::Dyn(format_key_glyph(palette_key))
```

The call sites in `input.rs` already hold `self.palette_key` (an `Option<u8>`); pass `self.palette_key.unwrap_or(0x1C)` into the hint builder.

### 4 — Alt+Shift+Arrow resize: CSI sequence outside keymap

**Dispatch:** `crates/jackin-capsule/src/tui/input.rs:741` — raw CSI sequence decode.

**Hint:** `crates/jackin-capsule/src/tui/components/dialog/hint.rs:56` — hardcoded.

**Root cause:** `Keymap<u8>` operates on single bytes; multi-byte CSI sequences are genuinely unrepresentable without architectural extension.

**Fix (two phases):**

Phase 1 — immediate: add explicit UNREGISTERABLE markers so the escape hatch is intentional and auditable, not silent:

```rust
// input.rs:741
// UNREGISTERABLE(CSI): multi-byte Alt+Shift+Arrow sequence; Keymap<u8>
// cannot represent multi-byte chords. Tracked in FINDINGS.md item 4.
if modifier == 4 { … }

// hint.rs:56
// UNREGISTERABLE(CSI): see input.rs:741 note.
HintSpan::Key("Alt+Shift+↑↓←→"),
```

Phase 2 — correct long-term fix: extend `KeyChord` to a sum type:

```rust
// crates/jackin-tui/src/keymap.rs  (EXTEND)
pub enum KeyChord {
    Byte(u8),         // existing: single-byte PTY chord (capsule)
    Csi(CsiKey),      // NEW: multi-byte CSI escape
    Event(KeyEvent),  // existing: crossterm KeyEvent (console/launch)
}

pub enum CsiKey {
    AltShiftUp, AltShiftDown, AltShiftLeft, AltShiftRight,
    // extend as needed
}

// Then resize is registerable:
KeyBinding {
    chords: &[KeyChord::Csi(CsiKey::AltShiftUp),
              KeyChord::Csi(CsiKey::AltShiftDown),
              KeyChord::Csi(CsiKey::AltShiftLeft),
              KeyChord::Csi(CsiKey::AltShiftRight)],
    action: NormalAction::ResizePane,
    hint: Some("resize pane"),
    visibility: Visibility::Shown,
    glyph: Some("Alt+Shift+↑↓←→"),
},
```

Phase 1 must ship in the same PR that addresses items 1–3. Phase 2 ships in a dedicated keymap-extension PR.

### 5 — Every dialog and mode: enforcement rule

After items 1–4 are fixed, the following rule applies to all future PRs:

> A PR that adds a new key binding must add a `KeyBinding` entry to the appropriate `Keymap<A>`. A PR that adds a new hint span must derive it from a `Keymap` via `.hint_spans()` or `.glyph_for()`. Any `HintSpan::Key(…)` literal without a corresponding keymap entry is a merge blocker unless accompanied by a `// UNREGISTERABLE` comment naming the specific reason (CSI multi-byte, mouse, or dynamic runtime value).

Affected surfaces to audit for remaining hand-written spans after items 1–3 are fixed:
- `crates/jackin-capsule/src/tui/components/dialog/hint.rs` — all hint builder functions
- `crates/jackin-launch/src/tui/` — verify all hint builders call keymap (currently correct; verify stays correct)
- `crates/jackin-console/src/tui/components/footer_hints.rs` — verify all hint builders call keymap (currently correct; verify stays correct)

---

## Test Module Layout Violations

Rule: `tests.rs` must never declare child modules. All tests live inline in a single `tests.rs` beside the implementation. Violations below must be fixed; no new submodule splits allowed.

For violations where `tests.rs` is a shell with no inline tests (marked **shell-only**), the fix has zero risk — merge children inline and delete the shell.

### 1 — `crates/jackin-capsule/src/daemon/tests/`

`daemon/tests.rs` (4896 lines, has inline tests) declares `mod render_conformance;`. Fix: inline `render_conformance.rs` (702 lines) into `daemon/tests.rs`; delete submodule file and `tests/` subdirectory entry.

### 2 — `crates/jackin-capsule/src/tui/layout/tests/` (**shell-only**)

`layout/tests.rs` (5 lines, no inline tests) declares `mod border_at; mod rect_shrink;`. Fix: merge both (102 + 71 = 173 lines) inline; delete subdirectory.

### 3 — `crates/jackin-config/src/app_config_roles/tests/`

`app_config_roles/tests.rs` (247 lines, has inline tests) declares `mod resolve_mode;`. Fix: inline `resolve_mode.rs` (384 lines); delete submodule file.

### 4 — `crates/jackin-console/src/tui/screens/editor/update/tests/`

`editor/update/tests.rs` (918 lines, has inline tests) declares `mod auth_flat_rows_integration;`. Fix: inline 359-line file; delete submodule file.

### 5 — `crates/jackin-console/src/tui/screens/editor/view/tests/`

`editor/view/tests.rs` (536 lines, has inline tests) declares 5 children: `agents_tab_render` (158), `contextual_row_items` (184), `general_tab_render` (38), `mounts_tab_render` (44), `secrets_tab_render` (900). Fix: merge all 1324 lines inline; delete subdirectory.

### 6 — `crates/jackin-console/src/tui/view/tests/` (insta snapshot complexity)

`tui/view/tests.rs` (464 lines, has inline tests) declares `mod consistency; mod snapshot;`. `snapshot.rs` itself declares `mod tests;` — three levels of nesting. Snapshot fixture names encode the module path; flattening changes names. Fix: (a) inline `consistency.rs` (296 lines); (b) inline `snapshot/tests.rs` into `view/tests.rs`; (c) move `snapshot/snapshots/` → `tests/snapshots/` and regenerate fixtures with `INSTA_UPDATE=new cargo test`; (d) delete `tests/snapshot.rs` and `tests/snapshot/`.

### 7 — `crates/jackin-core/src/agent/tests/`

`agent/tests.rs` (170 lines, has inline tests) declares `mod auth_table;`. Fix: inline 116-line file; delete submodule file.

### 8 — `crates/jackin-diagnostics/src/observability/tests/` (**shell-only**)

`observability/tests.rs` (3 lines, no inline tests) declares `mod endpoint_rewrite;`. Fix: inline 22-line file; delete subdirectory.

### 9 — `crates/jackin-protocol/src/tests/` (**shell-only**)

`src/tests.rs` (4 lines, no inline tests) declares `mod provider;`. Fix: inline 205-line file; delete subdirectory.

### 10 — `crates/jackin-runtime/src/instance/auth/tests/`

`auth/tests.rs` (1150 lines, has inline tests) declares 4 children: `amp_auth` (327), `codex_auth` (308), `github_auth` (438), `kimi_auth` (505). Fix: merge all 1578 lines inline; delete subdirectory.

### 11 — `crates/jackin-term/src/grid/tests/` (**shell-only**)

`grid/tests.rs` (8 lines, no inline tests) declares 5 children: `device_query` (54), `fuzz_regression` (62), `model_correctness` (278), `row_arena` (21), `scrollback_view` (114). Fix: merge all 529 lines inline; delete subdirectory.

### 12 — `crates/jackin/src/app/tests/` (**shell-only**)

`app/tests.rs` (5 lines, no inline tests) declares 2 children: `auth_set` (911), `resolve_role` (61). Fix: merge 972 lines inline; delete subdirectory.

### Summary

| # | Crate | Path | Shell-only? | Lines to inline |
|---|---|---|---|---|
| 1 | jackin-capsule | `daemon/tests/` | No | 702 |
| 2 | jackin-capsule | `tui/layout/tests/` | **Yes** | 173 |
| 3 | jackin-config | `app_config_roles/tests/` | No | 384 |
| 4 | jackin-console | `editor/update/tests/` | No | 359 |
| 5 | jackin-console | `editor/view/tests/` | No | 1324 |
| 6 | jackin-console | `tui/view/tests/` | No | complex (snapshots) |
| 7 | jackin-core | `agent/tests/` | No | 116 |
| 8 | jackin-diagnostics | `observability/tests/` | **Yes** | 22 |
| 9 | jackin-protocol | `src/tests/` | **Yes** | 205 |
| 10 | jackin-runtime | `instance/auth/tests/` | No | 1578 |
| 11 | jackin-term | `grid/tests/` | **Yes** | 529 |
| 12 | jackin | `app/tests/` | **Yes** | 972 |
