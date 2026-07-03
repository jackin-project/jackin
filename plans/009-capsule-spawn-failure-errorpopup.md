# Plan 009: Capsule spawn failures through the shared ErrorPopup, not an ephemeral banner

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b237..HEAD -- crates/jackin-capsule/src/tui/ crates/jackin-capsule/src/daemon.rs crates/jackin-capsule/src/daemon/compositor.rs`
> On mismatch with "Current state": STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/007-tui-error-dialog-canonical.md
- **Category**: bug (documented design-rule violation) / tech-debt
- **Planned at**: commit `a2ec1b237`, 2026-07-03
- **Execution status**: BLOCKED — drift check found existing capsule TUI changes in `components.rs`, `dialog_widgets.rs`, and `palette.rs` before plan work.

## Why this matters

The design canon bans ephemeral error surfaces: "All error conditions surfaced to the operator must use the red-border `ErrorPopup` … Ephemeral overlays (shimmer banners, auto-expiring toasts, single-line status strips) are banned for error display" (`dialogs.mdx` §Error Surface). The capsule violates this for spawn failures: a one-line red banner painted over the top row, cleared by the next keystroke — a real operator-facing error can vanish unread, cannot wrap long messages, and the capsule never uses the shared `error_dialog` at all (zero references). This plan routes spawn failures into the capsule's dialog stack backed by the shared `ErrorPopupState`.

## Current state

The banner (verified):

```rust
// crates/jackin-capsule/src/tui/components/chrome.rs:277-296
/// Spawn-failure banner: a red one-line notice painted over the top row.
/// Cleared by the next operator keystroke.
pub(crate) struct SpawnFailureBannerWidget<'a> { pub(crate) reason: &'a str }
impl Widget for SpawnFailureBannerWidget<'_> {
    fn render(self, area, buf) { ...
        buf.set_string(area.x, area.y, format!("jackin: {}", self.reason), style); } }
```

Data flow (all verified sites):
- `crates/jackin-capsule/src/daemon.rs:288` — `spawn_failure: Option<String>` field on the mux; set at `:1067-1072` via `spawn_request_failure_message(&label, &err)`, assigned at `:1126`.
- `crates/jackin-capsule/src/daemon/compositor.rs:392,457` — cloned into the frame as `spawn_failure: Option<&str>`.
- `crates/jackin-capsule/src/tui/view.rs:76` — frame field; `:218,:323` call `render_spawn_failure_banner`; `:326-330` renders the widget; `:388-392` message builders `spawn_failure_message`/`spawn_failure_agent_label`.

The capsule dialog stack: `crates/jackin-capsule/src/tui/components/dialog.rs:122` `pub enum Dialog` (~14 variants: CommandPalette, AgentPicker, RenameTab, …, ExitDirty, ExitInspect). Precedent for wrapping a shared jackin-tui state directly in a variant:

```rust
// dialog.rs:90
DebugInfo(jackin_tui::components::ContainerInfoState),
```

Footer hints: every dialog variant maps to hints through the exhaustive `footer_hint_spans` at `crates/jackin-capsule/src/tui/components/dialog/geometry.rs:142` — adding a variant without a hint arm is a compile error (this is the documented contract; your new variant gets an arm using `jackin_tui::components::error_popup_hint_spans()` — exists in `error_dialog.rs:47`).

Post-007 shared API: `ErrorPopupState::new(title, message)`, `render_error_dialog_in`, `handle_key(&mut self, …) -> ModalOutcome<()>`, `ERROR_POPUP_KEYMAP` (Enter/Esc/o dismiss).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| fmt / clippy | `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` | exit 0 |
| Tests | `cargo nextest run -p jackin-capsule` then `cargo nextest run` | pass |
| Capsule smoke mandate | see `.github/AGENTS.md` (jackin-capsule smoke-test requirement) before PR | per doc |

## Scope

**In scope**:
- `crates/jackin-capsule/src/tui/components/dialog.rs` (+ its submodules under `dialog/`: geometry/hint/key arms for the new variant)
- `crates/jackin-capsule/src/tui/components/dialog_widgets.rs` (render arm calling `render_error_dialog_in`)
- `crates/jackin-capsule/src/tui/view.rs` (delete banner render path), `chrome.rs` (delete `SpawnFailureBannerWidget`)
- `crates/jackin-capsule/src/daemon.rs`, `daemon/compositor.rs` (route `spawn_failure` into a dialog-open instead of a frame field — read how other dialogs open from daemon state first; follow that path)
- `crates/jackin-capsule/src/tui/{update,input,message,model}.rs` as needed for dialog open/dismiss wiring

**Out of scope**:
- PTY focus path and input routing while NO dialog is open — do not change how keystrokes reach the agent terminal.
- Other Dialog variants' behavior.
- The shared `error_dialog.rs` (extend only per plan 007's maintenance note if a gap appears — that is a STOP, not an improvisation).

## Git workflow

Branch (operator confirm): `fix/capsule-spawn-failure-errorpopup`. `git commit -s` + push. Update `dialogs.mdx` §Error Surface if it lists known violations, and the capsule docs page if one documents the banner (grep `docs/content` for `spawn` + `banner`).

## Steps

### Step 1: Add the Dialog variant

Add `Dialog::SpawnFailure(jackin_tui::components::ErrorPopupState)` following the `DebugInfo(ContainerInfoState)` precedent (`dialog.rs:90`). Wire the exhaustive arms the compiler will demand: `footer_hint_spans` (→ `error_popup_hint_spans()`), key handling (delegate to the state's `handle_key`; `Commit`/`Cancel` → pop dialog), `box_rect`/geometry (use the shared popup's sizing — read how `DebugInfo`'s arm sizes and mirror it), render arm in `dialog_widgets.rs` (→ `render_error_dialog_in`).

**Verify**: `cargo nextest run -p jackin-capsule` → compiles + passes (exhaustive matches force completeness).

### Step 2: Route spawn failures into the stack

Replace the `spawn_failure` string plumbing: where `daemon.rs:1067-1126` currently stashes the message onto the mux for the compositor, push `Dialog::SpawnFailure(ErrorPopupState::new("Spawn failed", message))` onto the dialog stack via the same mechanism other daemon-initiated dialogs use (find it: `rg 'Dialog::' crates/jackin-capsule/src/daemon.rs crates/jackin-capsule/src/tui/update.rs` — if no daemon-initiated dialog exists yet, route through a message/update event, matching the crate's TEA flow in `update.rs`; do NOT have the daemon mutate TUI state directly). Then delete: `spawn_failure` fields (`daemon.rs:288`, frame field `view.rs:76`, compositor pass-through), `render_spawn_failure_banner` (`view.rs:326`), both call sites (`view.rs:218,:323`), and `SpawnFailureBannerWidget` (`chrome.rs:277-296`). Keep `spawn_failure_message`/`spawn_failure_agent_label` (`view.rs:388-392`) — they build the message text.

**Verify**: `rg -n 'SpawnFailureBanner|spawn_failure:' crates/jackin-capsule/src` → only the message-builder fns remain; `cargo nextest run -p jackin-capsule` → pass.

### Step 3: Dismissal semantics

Popup dismisses on Enter/Esc/o via `ERROR_POPUP_KEYMAP` (NOT on any keystroke). While open, input goes to the dialog per the existing stack routing — confirm a keystroke aimed at the PTY no longer clears the error silently. Update any test asserting the old cleared-by-keystroke behavior to assert the new modal behavior.

**Verify**: `cargo nextest run -p jackin-capsule` → pass; capsule snapshot tests updated show the popup, not the banner.

## Test plan

- New test: spawn-failure event → dialog stack contains `SpawnFailure`, footer hints equal `error_popup_hint_spans()`, Esc pops it. Model on existing dialog-stack tests (`rg 'ExitDirty|CommandPalette' crates/jackin-capsule/src --glob '*tests*'` for the pattern).
- New test: while `SpawnFailure` is open, a printable keystroke does not reach the PTY path and does not dismiss (only Enter/Esc/o do).
- Update compositor/daemon tests that referenced the `spawn_failure` field.

## Done criteria

- [ ] fmt / clippy / `cargo nextest run` exit 0
- [ ] `rg 'SpawnFailureBannerWidget' crates/` → 0
- [ ] `rg 'error_dialog|ErrorPopupState' crates/jackin-capsule/src` → ≥1 real use (the capsule finally consumes the shared error surface)
- [ ] New variant has hint arm, key arm, render arm, geometry arm (compiler-enforced)
- [ ] `plans/README.md` updated

## STOP conditions

- No existing daemon→TUI dialog-open path exists and adding one requires touching the compositor protocol (`pane_snapshot`/socket messages) — report the required protocol change; that needs operator sign-off (protocol is versioned pre-release surface).
- The popup would obscure the PTY during agent output in a way tests show breaks scrollback/selection invariants.
- Plan 007 not landed.

## Maintenance notes

- Any future capsule error (not just spawn) must use this variant — reviewers reject new banners/toasts for errors (documented rule).
- Reviewer: scrutinize the daemon→dialog routing for ordering races (failure arriving while another dialog is open should stack, Esc walks back one — existing stack semantics).
- Deferred: richer failure content (diagnostics paths as structured rows) — the rows API from plan 007 supports it; wire when capsule failures carry paths.
