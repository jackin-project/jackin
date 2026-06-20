# FINDINGS.md

Open work only. Completed items removed. Each finding states the root cause and the structural fix required, not a patch at the symptom layer.

---

## Keymap / Hint-Bar Safety

**Context.** `Keymap<A>` makes hint/dispatch divergence architecturally impossible *within a single keymap table*: `dispatch()` and `hint_spans()` both read from `self.bindings`, so a `Visibility::Shown` binding is always both dispatched and advertised. That guarantee is real.

**Root cause of all violations below.** `main_view_hint()` and the prefix cheat sheet in `crates/jackin-capsule/src/tui/components/dialog/hint.rs` were hand-authored rather than derived from the keymaps that own dispatch. This creates a class of bugs: any time dispatch logic changes, the hint must be manually kept in sync, and the compiler cannot catch the drift. The fix class is: every hint bar must be constructed from the same `Keymap` that owns its dispatch, with no parallel hand-authored representation.

### 1 — Prefix cheat sheet: manual hints, semantic error

**File:** `crates/jackin-capsule/src/tui/components/dialog/hint.rs:19–32`

`main_view_hint()` returns a hardcoded `vec![HintSpan::Key("n/c"), HintSpan::Text("new/close"), ...]` when `prefix_awaiting == true`. The label "new/close" is wrong: in `PREFIX_COMMAND_KEYMAP`:
- `'n'` → `NextTab` (navigate, not "new")
- `'c'` → `NewTab` (new, correct)
- `'x'` → `KillPane` (close — the actual close key is not shown)

**Root cause:** Hint written by hand instead of derived from `PREFIX_COMMAND_KEYMAP`. Label lives in the hint file, not in the binding entry where the compiler can see both together.

**Structural fix required:** Add a context-filter path on `Keymap::hint_spans_filtered()` (or a new `hint_spans_for_context()` method) and replace the `prefix_awaiting` branch's manual `vec![...]` with `PREFIX_COMMAND_KEYMAP.hint_spans_filtered(...)`. The binding entry in `PREFIX_COMMAND_KEYMAP` already carries the label; the hint is then always identical to the dispatch table. The semantic error cannot recur because label and dispatch live in one place.

### 2 — Ctrl+Q: dispatched outside any keymap

**Dispatch:** `crates/jackin-capsule/src/tui/input.rs:351` — `else if b == CTRL_Q { events.push(InputEvent::RequestExit); }` where `CTRL_Q: u8 = 0x11` (line 533).

**Hints:** `crates/jackin-capsule/src/tui/components/dialog/hint.rs:30, 45, 62` — three separate hardcoded `HintSpan::Key("Ctrl-Q")` spans in prefix, scrollback, and normal modes.

**Root cause:** Ctrl+Q predates the keymap registry. It is a single-byte chord (`0x11`) that is fully representable in `Keymap<u8>` but was never added. Every surface where Ctrl+Q fires has a manual hint that must be maintained separately.

**Structural fix required:** Add `Ctrl+Q` (`0x11`) as a `Visibility::Shown` binding in each surface's keymap (capsule normal-mode keymap, scrollback keymap, prefix keymap). Route dispatch through `keymap.dispatch(b)` instead of the raw `else if b == CTRL_Q` arm. Remove the three manual `HintSpan::Key("Ctrl-Q")` entries — they become redundant once the keymap drives the hint bar. Any future change to the Ctrl+Q binding (key chord or action label) is then a single-source edit.

### 3 — Palette key: dynamic dispatch, hardcoded hint

**Dispatch:** `crates/jackin-capsule/src/tui/input.rs:343` — `if Some(b) == self.palette_key`. Default `0x1C` (`Ctrl+\`), overridable via `JACKIN_PALETTE_KEY` env var (parsed at `input.rs:541–577`).

**Hints:** `crates/jackin-capsule/src/tui/components/dialog/hint.rs:42, 49` — both hardcode `HintSpan::Key("Ctrl+\\")`. If the operator sets `JACKIN_PALETTE_KEY=C-j`, dispatch fires on `C-j` but the hint still shows `Ctrl+\`.

**Root cause:** Static `Keymap<u8>` cannot represent a key whose chord is determined at runtime. The hint was hardcoded to the default value, making it wrong whenever the default changes.

**Structural fix required:** This key genuinely cannot live in a static keymap. The fix is not to force it into `Keymap` but to eliminate the staleness: pass the resolved palette key byte into `main_view_hint()` and format it as a glyph at hint-build time using the same byte that drives dispatch. The resolved byte is already available on the input parser struct (`self.palette_key`); it must flow through to the hint builder. The hint will then always reflect the actual configured key, not a baked-in default.

### 4 — Alt+Shift+Arrow resize: CSI sequence outside keymap

**Dispatch:** `crates/jackin-capsule/src/tui/input.rs:741` — decodes multi-byte CSI escape sequence `\x1b[1;6A/B/C/D` (modifier=4 = Alt+Shift) into `InputEvent::ResizePane(dir)`.

**Hint:** `crates/jackin-capsule/src/tui/components/dialog/hint.rs:56` — `HintSpan::Key("Alt+Shift+↑↓←→")`, hardcoded.

**Root cause:** `Keymap<u8>` operates on single bytes. Multi-byte CSI sequences are not representable as a `u8` chord without architectural extension. This is a genuine capability gap, not an oversight: the keymap architecture today cannot express `Alt+Shift+Arrow`.

**Structural fix required (two options, choose one):**

Option A — Extend `Keymap` to support multi-byte chords (e.g., `Keymap<Chord>` where `Chord` is an enum covering `Byte(u8)` and `Csi(CsiKey)`). This closes the gap for all CSI-sequence keys and is the correct long-term fix. It requires changing the dispatch site in the input parser to attempt keymap lookup before falling through to raw CSI decoding.

Option B — Until Option A is implemented: add an explicit `// UNREGISTERABLE(CSI): multi-byte sequence, Keymap<u8> cannot represent` comment at both `input.rs:741` and `hint.rs:56`, and record the gap in this file. This makes the escape hatch intentional and auditable rather than silent. Do not leave it undocumented.

Option B is acceptable only if Option A is deferred to a dedicated PR. It is not a permanent state.

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
