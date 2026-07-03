# Plan 005: One spelling per key — shared glyph constants for every hint surface

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-tui/src/keymap.rs crates/jackin-console/src/tui/components/footer_hints/ crates/jackin-launch-tui/src/tui/ crates/jackin-capsule/src/tui/`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

The same logical key renders with different spellings depending on surface: `Tab` spelled out in launch and capsule dialogs while the keymap elsewhere derives `⇥`; console footer hints write slashed `↑/↓`/`←/→` while shared keymaps and capsule write unslashed `↑↓`/`←→`; `PgUp/PgDn` (launch) vs `PgUp PgDn` (shared diff_view); `Ctrl-` hyphen join (launch, capsule palette glyph) vs `Alt+Shift+` plus join (capsule resize hint). Glyphs are free strings — nothing prevents divergence. Operators learn key hints by shape; two spellings for one key erodes that. This plan introduces shared glyph constants + a normalization test, then converges every hint literal.

## Current state

Glyph infrastructure lives in `crates/jackin-tui/src/keymap.rs`: `KeyBinding.glyph: Option<&'static str>` (`:239`) overrides the auto-derived `chord_glyph(...)` (`:303-305`); doc comment (`:224`, `:237-238`) shows grouped examples `"↑↓"`, `"N/Esc"`. So the *derivation* is centralized but override strings are free literals.

Verified divergence sites (each excerpt read at `a2ec1b237`):

```rust
// crates/jackin-launch-tui/src/tui/run.rs:813
HintSpan::Key("Tab"),
// crates/jackin-capsule/src/tui/components/dialog/hint.rs:171
HintSpan::Key("Tab"),
// crates/jackin-capsule/src/tui/components/dialog/hint.rs:16-21 (Ctrl- hyphen join)
fn format_key_glyph(byte: u8) -> String {
    match byte { 0x01..=0x1A => format!("Ctrl-{}", (b'@' + byte) as char),
                 0x1C => "Ctrl-\\".to_owned(), ... } }
// crates/jackin-capsule/src/tui/keymap.rs:421 (plus join)
glyph: Some("Alt+Shift+↑↓←→"),
// crates/jackin-launch-tui/src/tui/keymap.rs:109
glyph: Some("PgUp/PgDn"),
// console slashed arrows (8+ sites):
// footer_hints/common.rs:20, settings.rs:123,140,196,382, editor.rs:226,
// modals.rs:276 ("↑/↓"), modals.rs:298 — all HintSpan::Key("←/→") or ("↑/↓")
// capsule unslashed: crates/jackin-capsule/src/tui/components/dialog/hint.rs:168
HintSpan::Key("←→"),
```

Also `crates/jackin-launch-tui/src/tui/file_browser/state.rs:60` (`PgUp/PgDn`) and `crates/jackin-tui/src/components/diff_view.rs:348` (`PgUp PgDn` — being migrated to the registry by plan 004; coordinate).

Already normalized (do not touch): `Esc` is uniformly `"Esc"`, Enter is uniformly `↵`.

Canonical rules doc: `docs/content/docs/reference/tui/navigation.mdx` (glyphs derive from the keymap table).

## Decisions (do not re-litigate)

- Arrows: **unslashed** `↑↓`, `←→`, `↑↓←→` (matches keymap auto-derivation and capsule).
- Tab: **`⇥`** (matches keymap-derived glyph elsewhere).
- Page keys: **`PgUp/PgDn`** as the grouped pair glyph (slash marks alternative keys, consistent with the documented `"N/Esc"` idiom).
- Modifier join: **hyphen** — `Ctrl-Q`, `Alt-Shift-↑↓←→` (matches the majority: launch keymap `:31,38`, capsule `format_key_glyph`).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run` | pass |
| Lookbook | `cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook` + `--check` | exit 0 |
| Survey | `rg -n '"(Tab|↑/↓|←/→|PgUp[ /]PgDn|Alt\+|Ctrl\+)"' crates/ --type rust` | enumerates targets |

## Scope

**In scope**:
- `crates/jackin-tui/src/keymap.rs` — new `pub mod glyph` with the constants + the normalization test
- Every `HintSpan::Key`/`glyph:` literal in: `crates/jackin-console/src/tui/components/footer_hints/*.rs`, `crates/jackin-launch-tui/src/tui/**`, `crates/jackin-capsule/src/tui/**`
- Snapshot/test expectation updates those changes trigger
- `docs/content/docs/reference/tui/navigation.mdx` — one paragraph: glyph constants are mandatory for shared keys
- Regenerated lookbook SVGs if any story shows an affected glyph

**Out of scope**:
- Key *dispatch* (chords) — display strings only.
- `diff_view.rs` hints if plan 004 already routed them through `SCROLL_HINT_KEYMAP` (then only ensure the registry itself uses the constants).
- Capsule `format_key_glyph` runtime formatting (already hyphen-join; just leave it, or have it reuse a shared `glyph::ctrl(char)` helper if trivial).

## Git workflow

Branch (operator confirm): `refactor/tui-glyph-constants`. `git commit -s`, push per commit. navigation.mdx updated same PR.

## Steps

### Step 1: Add `glyph` module in `jackin-tui`

In `crates/jackin-tui/src/keymap.rs` (bottom, near `chord_glyph`), add:

```rust
/// Canonical display spellings for keys that appear in hints.
/// Every `KeyBinding.glyph` override and `HintSpan::Key` literal for these
/// keys MUST use these constants — one spelling per key, everywhere.
pub mod glyph {
    pub const TAB: &str = "⇥";
    pub const UP_DOWN: &str = "↑↓";
    pub const LEFT_RIGHT: &str = "←→";
    pub const ALL_ARROWS: &str = "↑↓←→";
    pub const PGUP_PGDN: &str = "PgUp/PgDn";
    pub const ESC: &str = "Esc";
    pub const ENTER: &str = "↵";
    // modifier join is "-": e.g. "Ctrl-Q", "Alt-Shift-↑↓←→"
}
```

Confirm `chord_glyph`'s auto-derived spellings agree with these constants (read its match arms); if `chord_glyph` derives a different spelling for any of them, align `chord_glyph` to the constant (that IS the normalization).

**Verify**: `cargo nextest run -p jackin-tui` → pass.

### Step 2: Converge all literals

Using the survey command, replace every divergent literal with the constant (import `jackin_tui::keymap::glyph` or the re-export you add to `components.rs`):
- `"Tab"` → `glyph::TAB` at `launch-tui/tui/run.rs:813`, `capsule/dialog/hint.rs:171`, and any other hits.
- `"↑/↓"` → `glyph::UP_DOWN`, `"←/→"` → `glyph::LEFT_RIGHT` across `footer_hints/{common,settings,editor,modals}.rs` (8+ sites listed above).
- `"PgUp/PgDn"` literals → `glyph::PGUP_PGDN` (launch `keymap.rs:109`, `file_browser/state.rs:60`).
- `"Alt+Shift+↑↓←→"` (capsule `keymap.rs:421`) → `"Alt-Shift-"` join composed with `glyph::ALL_ARROWS` (a `concat!`-style const or a single literal `"Alt-Shift-↑↓←→"` — but spelled with hyphens; prefer building from the constant if `const` concatenation is awkward, a plain literal with a `// glyph::ALL_ARROWS` comment is acceptable ONLY if a test guards it — see Test plan).

Update every failing test/snapshot expectation to the new spellings — each diff must be exactly a glyph-spelling change.

**Verify**: `rg -n 'HintSpan::Key\("(Tab|↑/↓|←/→)"\)' crates/` → 0 matches; `rg -n '"PgUp PgDn"|Alt\+Shift' crates/ --type rust` → 0 matches; `cargo nextest run` → pass.

### Step 3: Guard against regrowth

Add a test in `crates/jackin-tui/src/keymap.rs`'s test module: for each constant, assert the forbidden variants differ (trivial) AND add a workspace-level guard the repo can run — a unit test that walks a hardcoded list of known-forbidden spellings is not possible across crates from jackin-tui, so instead add the guard as a CI-friendly grep in the Done criteria and a sentence in `navigation.mdx`: "Shared keys use `jackin_tui::keymap::glyph` constants; a hint PR introducing `Tab`, `↑/↓`, `←/→`, `PgUp PgDn`, or `+`-joined modifiers is a design violation."

**Verify**: `cd docs && bun run build` → exit 0.

## Test plan

- Existing hint/snapshot tests updated in Step 2 are the regression net.
- New: in each surface crate with a keymap table (`launch-tui`, `capsule`), add/extend a test asserting its `Visibility::Shown` bindings' glyphs contain none of the forbidden spellings (iterate the binding table; assert `!glyph.contains("↑/")`, `glyph != "Tab"`, `!glyph.contains('+') || glyph.starts_with("0x")` — adapt to real content). Model on existing keymap tests in those crates (`rg 'mod tests' crates/jackin-launch-tui/src/tui/keymap.rs crates/jackin-capsule/src/tui/keymap.rs`).

## Done criteria

- [x] fmt / clippy / `cargo nextest run` exit 0
- [x] `rg -n 'HintSpan::Key\("Tab"\)|HintSpan::Key\("↑/↓"\)|HintSpan::Key\("←/→"\)' crates/` → 0
- [x] `rg -n '"PgUp PgDn"' crates/ --type rust` → 0
- [x] `rg -n 'Alt\+|Shift\+' crates/ --type rust` → 0 (excluding genuine string content unrelated to hints — verify each residual hit and list it in the PR body if intentionally kept)
- [x] Lookbook `--check` exits 0 (regen first if a story glyph changed)
- [x] `plans/README.md` updated

## STOP conditions

- `chord_glyph` derives `Tab` as something other than `⇥` AND changing it breaks >5 snapshot tests in one surface — report scope before mass-updating.
- A surface's hint width math depends on glyph length such that the new spelling truncates hints (capsule single-row truncation, pre plan-010) — report which hint overflows.

## Maintenance notes

- Plan 010 (capsule hint renderer) and plan 004 (diff_view) touch neighboring lines — land order flexible, but re-run the survey grep after each.
- Reviewer: every diff hunk should be a spelling change or a test expectation; anything else is scope creep.
