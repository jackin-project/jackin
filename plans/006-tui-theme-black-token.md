# Plan 006: Name the last raw palette color — `theme::INK` for on-chip black

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-tui/src/theme.rs crates/jackin-tui/src/components/`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (coordinate with plans 004/007 which touch two of the same lines)
- **Category**: tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03

## Why this matters

The shared crate centralizes color as `theme.rs` constants, but the "black text on a bright chip" foreground is written as raw `Color::Black` at ~7 component sites. It is the only palette color without a name, so it cannot be reskinned or audited with the rest. One token, mechanical replacement. (An earlier audit claimed 32 hardcoded sites; verification shows most were `ansi_text.rs`'s ANSI SGR→ratatui decoding table, which is a by-design pass-through of terminal colors, NOT palette usage — leave it untouched.)

## Current state

Verified raw `Color::Black` component sites at `a2ec1b237`:

- `crates/jackin-tui/src/components/status_footer.rs:220, :235, :291`
- `crates/jackin-tui/src/components/button_strip.rs:106`
- `crates/jackin-tui/src/components/brand_header.rs:38` — `Span::styled(" jackin", block.fg(Color::Black))`
- `crates/jackin-tui/src/components/text_input.rs:465, :536` (cursor fg — plan 004 unifies these two into one; if 004 landed, there is one site)
- `crates/jackin-tui/src/components/error_dialog.rs:122` (hand-painted OK button — plan 007 replaces this line entirely; skip it if 007 landed)

Theme convention (`crates/jackin-tui/src/theme.rs`): named `_RGB` const + `pub const X: Color = color(X_RGB);` (e.g. `WHITE:51`, `DANGER_RED:62`). `Color::Black` is ANSI black, not an RGB triple — so this token is defined directly: `pub const INK: Color = Color::Black;` with a doc comment ("foreground for text on bright chips/buttons; ANSI black by design so terminals map it").

Out-of-pattern by design (do NOT touch): `crates/jackin-tui/src/ansi_text.rs:149-173` — SGR index→`Color` decoding table.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-tui` then `cargo nextest run` | pass |
| Lookbook | `cargo run -p jackin-tui-lookbook -- --check docs/public/tui-lookbook` | exit 0, no diffs |

## Scope

**In scope**: `crates/jackin-tui/src/theme.rs` (one const), the component sites listed above.
**Out of scope**: `ansi_text.rs`; `lib.rs` `ansi` module (moves in plan 018); any surface crate; any visual change (INK == Color::Black, pixel-identical).

## Git workflow

Branch (operator confirm): `refactor/tui-ink-token`. `git commit -s` + push. No docs page change needed (no behavior/visual change).

## Steps

### Step 1: Add the token

In `theme.rs`, next to `WHITE`, add `pub const INK: Color = Color::Black;` with the doc comment above.

### Step 2: Replace the sites

Replace `Color::Black` with `INK` (import from `crate::theme`) at every live site listed in Current state (skip lines already removed by plans 004/007). Then confirm exhaustively:

**Verify**: `rg -n 'Color::Black' crates/jackin-tui/src --type rust | grep -v 'ansi_text.rs\|theme.rs\|tests'` → 0 matches; `cargo nextest run` → pass; lookbook `--check` → exit 0 with zero diffs.

## Test plan

No new tests — pixel-identical alias. Lookbook `--check` with zero diffs is the proof.

## Done criteria

- [x] fmt / clippy / `cargo nextest run` exit 0
- [x] `rg -n 'Color::Black' crates/jackin-tui/src --type rust | grep -v 'ansi_text.rs|theme.rs|tests'` → 0
- [x] Lookbook `--check` exits 0, zero SVG diffs
- [x] `plans/README.md` updated

## STOP conditions

- Any lookbook SVG changes (should be impossible; if it happens, a site was not a pure alias).

## Maintenance notes

- Reviewer: confirm `ansi_text.rs` untouched.
- Future: a clippy lint or grep-based CI check forbidding `Color::` literals under `components/` would keep this closed; deferred (needs a lint-infrastructure decision).
