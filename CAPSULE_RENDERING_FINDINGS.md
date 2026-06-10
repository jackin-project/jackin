# Capsule rendering — target architecture and implementation plan

Status: approved plan, 2026-06-10. Decision rule for everything in this document: judge by correctness, never by cost, effort, or ROI. The project is explicitly long-term; every item below is in scope until proven infeasible.

Scope: `crates/jackin-term/` (terminal model) and `crates/jackin-capsule/` (multiplexer daemon: compositor, socket backend, sessions, input dispatch), plus the shared `crates/jackin-tui/` components they consume.

---

## 0. Execution protocol and status (for autonomous runs)

This section makes the plan executable end-to-end by an autonomous session (e.g. Claude Code `/goal`). The executing agent updates this section as work proceeds: check items off, fill in evidence, and commit the file change together with the work it describes.

### 0.1 Protocol

- **Order:** strictly §5 order — Stage 0 alongside PR 1, then PR 2 → PR 3 → PR 4. Within a PR, steps in their numbered order.
- **Evidence rule:** an item is checked only after its verification command ran and the output (command + result) was shown in the session transcript, with a one-line evidence note (commit hash, test name, or PR URL) recorded next to the item below. Assertions without surfaced output do not count.
- **Operator gates** (mark the item `BLOCKED(operator): <what is needed>` and continue with the next unblocked work; never stall and never bypass):
  1. **Merges.** The operator authorizes every merge. Mark the PR ready, request merge, move on. A "complete" program may end with PRs ready-but-unmerged.
  2. **Stage-0 run id.** If no `--debug` run id has been provided, proceed with synthetic fixtures (PR 2 step 4 lists them) and leave the real-transcript capture blocked.
  3. **Scrollback-retention decision** (§5 PR 4 step 9). Present both candidates with fixture results and wait for the operator's choice; implement the rest of PR 4 around it.
- **Branch/PR discipline:** PR 1 lives on `fix/capsule-scrollback-redraw` (PR #555). Each later PR branches from the previous PR's branch when that PR is not yet merged (stacked; name the base in the PR body) or from `main` after it merges. Push every commit in the turn it is created. Conventional Commits, DCO `-s`, `Co-authored-by: Claude <noreply@anthropic.com>`.
- **Per-PR closeout:** apply the §7 docs row, run the §6.1 block showing output, update this checklist, push, mark the PR ready for review.

### 0.2 Status checklist

Stage 0 — evidence capture
- [ ] S0.1 Operator `--debug` run id obtained — `BLOCKED(operator)`: needs an operator repro session (`cargo run --bin jackin -- console --debug`, Codex + Claude Code panes, heavy streaming, scrollback in/out, focus swaps, dialog open/close) and the printed run id. `~/.jackin/data/diagnostics/runs/` is empty on this machine and an in-session attempt found the nested DinD image cache cold (no construct image), so a faithful real-agent capture needs the operator's host (checked 2026-06-10). Proceeding with synthetic fixtures per §0.1 gate 2; `cargo xtask pty-fixture` turns the eventual run id into fixtures in one command.
- [ ] S0.2 CSI inventory extracted and appended to this file — `BLOCKED(operator)`: derives from the S0.1 run JSONL (`forwarding unhandled CSI to client` lines). PR 4's allowlist ships from spec-known sequences (kitty keyboard push/pop, modifyOtherKeys) until the inventory arrives.
- [ ] S0.3 PTY byte streams extracted for fixtures — `BLOCKED(operator)`: derives from the S0.1 run JSONL (`session feed_pty bytes` hex lines). PR 2 ships the synthetic fixture set; `jackin-xtask pty-fixture` makes the recording one command once a run id exists.

PR 1 — scroller + scrollbar + stopgap (`fix/capsule-scrollback-redraw`, PR #555)
- [x] P1.0 Plan document committed and draft PR opened — Evidence: commit a6613726, https://github.com/jackin-project/jackin/pull/555
- [x] P1.1 `Terminal::clear()` ambiguity resolved, stale comment fixed — Evidence: pinned ratatui-core 0.1.0 `terminal.rs:540–559` read in-session — Fullscreen `Terminal::clear()` calls `clear_region(ClearType::All)` (emits `\x1b[2J`), never `Backend::clear`. Stale comment in `socket_backend.rs` fixed; one-shot `suppress_next_clear_escape` added to `SocketBackend::clear_region`; latent stray-`2J` call site in `pane_layout.rs::resize` fixed with it. Test: `suppressed_clear_resets_style_without_screen_erase`.
- [x] P1.2 Repaint-pending on offset change — Evidence: `session.rs::scroll_by` / `set_scrollback_offset` / `reset_scrollback_view` set `pane_body_repaint_pending`; covered by `wheel_back_to_live_repaints_body_and_footer`.
- [x] P1.3 Wheel frames via `ScrollbackMovement` — Evidence: `input_dispatch.rs` wheel arm now composes `compose_diff_frame(wheel_scrollback_redraw_reason())`; test `wheel_back_to_live_repaints_body_and_footer` asserts body + footer repaint on the offset→0 step.
- [x] P1.4 Scrollback anchoring in `feed_pty` — Evidence: `session.rs::feed_pty` grows the offset by the rows evicted during the feed before clamping; test `feed_while_scrolled_keeps_view_anchored`.
- [x] P1.5 Convergence stopgap (no screen-erase byte) — Evidence: `compose_ratatui_frame` resets the Ratatui baseline through the suppress flag, so every Ratatui frame re-emits cells with no `2J`; test `diff_frames_repaint_in_place_without_screen_erase`.
- [x] P1.6 Cursor hidden while scrolled in `current_mode_state` — Evidence: `session.rs::current_mode_state` emits `?25l` whenever `scrollback_offset != 0`; test `current_mode_state_hides_cursor_while_scrolled`.
- [x] P1.7 Scrollbar via shared `scrollable_panel` component — Evidence: `view.rs::apply_pane_scrollbar` renders through `render_vertical_scrollbar_in_area` with `TailScroll::to_top_offset`; click-to-jump added (`mouse_input.rs::scrollbar_jump_at` via `scrollbar_offset_for_track_position`). Tests: `pane_scrollbar_renders_shared_component_glyphs_only`, `scrollbar_click_jumps_scrollback`.
- [x] P1.8 Step-8 tests added and passing — Evidence: `cargo test -p jackin-capsule` 459 passed / 0 failed (transcript 2026-06-10); new tests named in P1.2–P1.7 evidence.
- [ ] P1.9 Manual smoke matrix executed (`--debug`) — `BLOCKED(operator)`: needs an interactive Docker-capable host terminal. Recipe: `cargo run --bin jackin -- console --debug`; stream Codex; wheel up 3 pages → view holds still; wheel down to bottom (wheel only) → input box + live footer return; type while scrolled → snap; focus swap while scrolled → no cursor in history.
- [x] P1.10 §7 PR 1 docs row applied — Evidence: `reference/tui/components.mdx` "One scrollbar renderer" rule; `reference/capsule/multiplexer-design-rules.mdx` Scrollback rules (repaint-on-offset-change, cursor hidden while scrolled, anchored view). (`tui-design-decisions.mdx` named by AGENTS.md does not exist; `reference/tui/` pages are its current home.)
- [x] P1.11 §6.1 block run with passing output; PR ready; CI green (`gh pr checks`) — Evidence: fmt/clippy/`cargo test --workspace` (exit 0) + capsule eval build shown in-session 2026-06-10; PR #555 marked ready; `gh pr checks 555` all pass (ci-required, cargo nextest, clippy, fmt, msrv, fuzz, docs-required, construct-required) on head 95b39a5e after merging origin/main (#552 provider change).
- [ ] P1.12 Merged — `BLOCKED(operator)`: per-PR merge authorization required; PR will be marked ready and merge requested. — Evidence:

PR 2 — echo-back harness + fixtures (`chore/capsule-render-conformance`)
- [x] P2.1 VirtualClient + I1 assertion helper — Evidence: `render_conformance_tests.rs` `VirtualClient` (second `DamageGrid`) + `assert_screen_matches_model` (grapheme, full `Attrs`, wide flags over each pane rect) + `assert_cursor_contract`.
- [x] P2.2 Deterministic harness driving `compose_pending_frame` — Evidence: `feed_and_compose` marks the pane dirty and calls `compose_pending_frame` directly; no ticker, no sleeps; scenarios cover streaming, full scroll cycle (incl. wheel-to-zero), focus swap, resize, dialog open/close, alt-screen enter/exit, selection.
- [x] P2.3 `jackin-xtask pty-fixture` subcommand — Evidence: `crates/jackin-xtask/src/pty_fixture.rs` (`cargo xtask pty-fixture <run.jsonl> <label> <out.bin>`); unit tests `extracts_matching_label_from_raw_log_line` et al. green (`cargo test -p jackin-xtask`: 8 passed).
- [x] P2.4 Fixtures: synthetic-only (S0 blocked) + Unicode/CSI synthetic set — Evidence: synthetic streams in the harness (SGR streaming, alt-screen, combining/VS16/ZWJ, wide-lead overwrite, DECSTR, DSR); `tests/fixtures/pty/` created with recording instructions; recorded fixtures remain blocked on S0.1.
- [x] P2.5 Scenario suite green; remaining failures `#[ignore = "fixed by PR 3/4"]` — Evidence: `cargo test -p jackin-capsule`: 467 passed / 6 ignored; the 6 ignored (grapheme/VS16/ZWJ, wide-lead, DECSTR, DSR clamp) all FAIL when forced with `--ignored` (transcript 2026-06-10) — the executable spec for PR 4. The harness also exposed a PR 1 stopgap hole (default-blank residue), fixed on the PR 1 branch by the sentinel-baseline commit.
- [x] P2.6 §7 PR 2 docs row; §6.1 output; PR ready; CI green — Evidence: jackin-term README correctness-ledger entry + TESTING.md recording flow; fmt/clippy/`cargo test --workspace` exit 0 + capsule eval build in-session; PR #557 open + ready (stacked on #555); `gh pr checks 557` all pass (docs-required, repo-link-check, spell checks, DCO; the path-aware Rust suites run against this code on #555's full pipeline and again when the stack retargets `main`), surfaced 2026-06-10.
- [ ] P2.7 Merged — `BLOCKED(operator)`: per-PR merge authorization required. — Evidence:

PR 3 — single writer + derived rendering (`refactor/capsule-single-render-path`, PR #559)
- [x] P3.1 `ClientWriter` sole socket owner; `?2026` frame brackets — Evidence: `crates/jackin-capsule/src/client_writer.rs`; `write_frame` wraps `?2026h…l`, `enqueue_out_of_band` flushes at frame boundaries; `attached_out` + `send_output` deleted (commit "route every client byte through one ClientWriter").
- [x] P3.2 Patch tier deleted — Evidence: `compose_direct_dirty_pane_frame`, `SocketBackend::draw_grid_patch`, the wire-efficiency example, and the dirty-patch allocation tests are gone; `GridPatch` remains in jackin-term for the terminal-observation roadmap consumer.
- [x] P3.3 Derived rendering; request flags + per-action compose returns deleted — Evidence: `Multiplexer::invalidate` + `frame_generation`; `pending_full_redraw`/`pending_diff_redraw`/`dirty_panes`/`pane_body_repaint_pending`/`pane_chrome_dirty` deleted; `handle_input`/`apply_action`/`apply_dialog_action` return `()`; wipe-policy test `wipe_policy_erases_only_on_first_attach_and_resize` (I4).
- [x] P3.4 Mode + cursor reconciliation; three mode lists deleted — Evidence: `AssertedClientState` + `append_client_state_reconciliation` in `compositor.rs`; `current_mode_state`/`drain_mode_transitions`/`focus_swap_reset` deleted; tests `mode_reconciliation_resets_agent_modes_on_focus_swap`, `cursor_reconciliation_hides_cursor_while_scrolled`; harness cursor contract green.
- [x] P3.5 Hyperlink frame layer; raw overlays deleted — Evidence: `SocketBackend::set_hyperlink_regions` + OSC 8 brackets during cell emission; compositor overlay append removed; `container_info_hyperlink_regions` added to jackin-tui.
- [x] P3.6 Banner + chrome as widgets; `last_bottom_chrome` deleted — Evidence: `BottomChromeWidget`/`DialogBottomChromeWidget`/`SpawnFailureBannerWidget` in `tui/components/chrome.rs`; `Multiplexer::spawn_failure` cleared on keystroke; raw chrome renderers + byte cache deleted; tests `bottom_chrome_rides_the_cell_buffer_on_every_frame`, `spawn_failure_banner_rides_the_frame_until_a_keystroke_clears_it`.
- [x] P3.7 Encoder CUP-skip restricted to ASCII runs — Evidence: `SocketBackend::draw` advances the tracked column only for single ASCII printables (0x20–0x7E); any other glyph forces an explicit CUP (D8).
- [x] P3.8 Event-driven pacing — Evidence: render deadline (immediate after idle, cadence cap during bursts) replaces the 33 ms ticker in `run_daemon` (commit "event-driven frame pacing with a cadence cap").
- [x] P3.9 Perf numbers recorded in PR body — Evidence: `render_perf_probe` (release, 80×24 stream): before p50/p95 16/20 µs + 2316 B/frame (patch tier, PR 2 worktree) → after 104/132 µs + 5777 B/frame; transcript 2026-06-10; escape hatch documented in ADR-005.
- [x] P3.10 PR-3-tagged `#[ignore]` cases green; no harness regression — Evidence: the only PR-3-tagged case (`clear_screen_during_selection_overlay_converges_after_clear`) flipped green on the PR 1 branch when the sentinel baseline landed; full harness green through the structural swap (`cargo test -p jackin-capsule`: 465 passed / 6 ignored, all PR-4-tagged).
- [x] P3.11 §7 PR 3 docs row (incl. new ADR); §6.1 output; PR ready; CI green — Evidence: ADR-005 + multiplexer-design-rules + terminal-model + roadmap render-model updated; docs build/check:repo-links/tsc/bun test green in-session; fmt/clippy/`cargo test --workspace` exit 0; capsule eval build OK; PR #559 open + ready (stacked on #557); `gh pr checks 559` all pass, surfaced 2026-06-10.
- [ ] P3.12 Merged — `BLOCKED(operator)`: per-PR merge authorization required. — Evidence:

PR 4 — model correctness + CSI gating (`fix/capsule-csi-gating`, PR #560; no split needed)
- [x] P4.1 Default-deny unhandled CSI + allowlist — Evidence: `perform.rs` catch-all emits `PassthroughEvent::DroppedCsi` (session `cdebug!`-logs, never forwards); allowlist = kitty push/pop + `CSI > 4;n m` with reasons in `multiplexer-design-rules.mdx`; tests `unknown_csi_is_default_denied_and_carried_as_dropped`, `kitty_and_modify_other_keys_stay_on_the_forward_allowlist`.
- [x] P4.2 DECSCUSR per-pane via reconciliation — Evidence: grid `cursor_style` + `AssertedClientState.cursor_style`; tests `decscusr_is_tracked_per_grid_and_not_forwarded`, harness `decscusr_reconciles_per_pane_and_never_forwards_raw`.
- [x] P4.3 DECSTR in-grid — Evidence: `'p'` with `!` resets attrs/margins/wrap/cursor-visible/app-cursor/bracketed-paste/saved-cursor, never forwarded; tests `decstr_resets_modes_attrs_and_margins_in_grid`, harness `decstr_soft_reset_is_handled_in_grid` (former `#[ignore]`, now green).
- [x] P4.4 Agent `?2026` absorbed — Evidence: `set_dec_mode` 2026 arm absorbs; `PassthroughEvent::SynchronizedOutput` deleted; tests `synchronized_output_toggles_are_absorbed`, `agent_synchronized_output_toggles_are_absorbed`.
- [x] P4.5 Grapheme-cluster cells + Unicode fixtures — Evidence: zero-width/ZWJ join in `write_char_at_cursor` (`append_to_previous_cluster`); tests `combining_mark_joins_base_cell`, `vs16_and_zwj_sequences_stay_one_cluster`; harness `combining_mark_joins_base_character`, `vs16_emoji_stays_one_cluster`, `zwj_family_emoji_stays_one_cluster` un-ignored and green.
- [x] P4.6 Wide-lead overwrite fix — Evidence: lead→continuation blanking + dirty extension; tests `overwriting_a_wide_lead_blanks_the_continuation`, harness `wide_lead_overwrite_blanks_continuation`.
- [x] P4.7 DSR clamp — Evidence: CPR column clamped to `min(cursor_col, cols-1)+1`; tests `dsr_clamps_the_deferred_wrap_phantom_column`, harness `dsr_cursor_report_clamps_phantom_column`.
- [x] P4.8 Scrollback-offset single owner — Evidence: `Session.scrollback_offset` field deleted; grid owns via `set_scrollback` (clamping) + `scrollback()`; session delegates (`Session::scrollback_offset()`).
- [x] P4.9 Retention decision implemented — Evidence: candidate (b) preserve-on-clear with exact dedupe (operator delegated the in-flight decisions to the agent in-session 2026-06-10; (a) rejected because it drops the cleared-screen recoverability the wheel fixtures assert). `mutated_since_preserve` flag + byte-equality vs `last_preserved_block`; test `repeated_clear_without_mutation_preserves_exactly_once`; recorded in `terminal-model.mdx`.
- [x] P4.10 Spurious LF mark removed — Evidence: LF arm no longer marks damage; scrolls mark their own rows; test `plain_line_feed_marks_no_damage`.
- [x] P4.11 Zero non-blocked `#[ignore]` in harness; CSI inventory annotated — Evidence: harness runs with zero `#[ignore]` (`cargo test -p jackin-capsule`: 473 passed / 0 ignored incl. the perf probe, transcript 2026-06-10); the real-agent CSI inventory annotation remains `BLOCKED(operator)` with S0.2 — the allowlist table in `multiplexer-design-rules.mdx` documents the spec-known set and every drop is `--debug`-visible for future annotation.
- [x] P4.12 §7 PR 4 docs row; §6.1 output; PR ready; CI green — Evidence: docs rows applied (allowlist table; retention/ownership section); fmt/clippy/`cargo test --workspace` exit 0 + `cargo nextest run` 606/606 (0 skipped) + docs gates + capsule eval build shown in-session; PR #560 open + ready (stacked on #559); `gh pr checks 560` all pass, surfaced 2026-06-10.
- [ ] P4.13 Merged — `BLOCKED(operator)`: per-PR merge authorization required. — Evidence:

### 0.3 Program completion definition

The program is complete when: every §0.2 item is either checked with evidence or marked `BLOCKED(operator)` with what is needed; PRs 1–4 exist, are pushed, show green CI (`gh pr checks` output surfaced), and are marked ready; the echo-back harness runs in `cargo test --workspace` with zero `#[ignore]` cases other than operator-blocked ones; each invariant I1–I7 names its enforcing test or mechanism with a file path; and §0.2 itself reflects all of this in the committed file.

---

## 1. Goal

The capsule presents agent/shell terminal output inside jackin' chrome (tabs, borders, scrollbars, dialogs) to one attached client. The required end state:

- **No stale cells, ever.** What the operator sees is always exactly what the pane's terminal model contains.
- **No flicker, no tearing.** Frames apply atomically; repaints overwrite in place; the screen never flashes blank.
- **Low latency.** Output and input echo appear as fast as the terminal allows, not gated by a fixed tick.
- **jackin' look and feel everywhere.** All UI renders through Ratatui with the shared `jackin-tui` components; the capsule and the host console stay visually and behaviorally identical.
- **Mechanically verified.** The core invariant (screen == model) is enforced by CI, not by review vigilance.

## 2. Defects this plan removes

Each defect is listed with the structure that permits it; the architecture in §3 removes the structure, not just the instance.

| # | Defect | Permitting structure | Where |
|---|---|---|---|
| D1 | Stale/interleaved cells: old glyphs persist inside new text (`body─from─the─template`), duplicated transcript blocks, fragments at wrong columns | Three independent writers to the client (Ratatui diff frames, direct grid patches, raw passthrough) share one diff baseline that only the first updates; any change-then-revert cell between Ratatui frames is skipped forever | `compositor.rs:531–638`, `socket_backend.rs:81–120`, `socket_backend.rs:10` (stale claim) |
| D2 | Wheel-scroll back to bottom leaves the old scrollback view on screen: input box missing, footer stuck on "Esc exit scrollback", cursor painted over history rows | Wheel frames reuse the PTY-output partial path; at offset 0 it emits a near-empty grid patch instead of repainting; render decisions live in scattered request flags ("state changed but nobody requested the right repaint" class) | `input_dispatch.rs:420–431`, `compositor.rs:26–51` |
| D3 | Scrolled-back view slides under the reader while the agent streams | Offset is clamped but not anchored when rows are evicted into scrollback | `session.rs:730–737`, `session.rs:846–851` |
| D4 | Cursor visible over history (focus swap / attach while scrolled) | Mode re-assertion spread across three hand-maintained lists; each is a place to forget a rule | `session.rs:1033–1101` |
| D5 | Outer-terminal state corruption: cursor shape leaks across panes, soft reset hits the host terminal, style cache silently invalidated | Unknown CSI is forwarded raw by default (DECSCUSR `CSI n SP q`, DECSTR `CSI ! p`, ANSI `h`/`l` without `?`) | `perform.rs:378–385`, `session.rs:987–1006` |
| D6 | Frames freeze until "something pokes the screen" | Agent `?2026` BSU/ESU forwarded verbatim on the passthrough schedule, decoupled from frame timing; a dropped ESU leaves the outer terminal holding updates; capsule's own frames have no atomicity brackets | `grid.rs:1259–1262`, `session.rs:962–968` |
| D7 | Black flash on tab switch / zoom / dialog close | Full tier wipes with `\x1b[2J` before repainting | `compositor.rs:60–88` |
| D8 | Column drift after ambiguous-width glyphs (`…`, `─`, `•`, VS16 emoji) | Encoder skips cursor positioning based on `unicode-width` assumptions the outer terminal may not share | `socket_backend.rs:157–184, 290–335` |
| D9 | Grid text model is wrong for Unicode: combining marks overwrite their base character (data loss); VS16/ZWJ sequences split across cells | Cells are written per `char`, not per grapheme cluster | `grid.rs:754–814` |
| D10 | Orphaned wide-char half: overwriting a wide lead leaves the continuation cell stale and unmarked | Asymmetric wide-char cleanup (continuation→lead handled, lead→continuation not) | `grid.rs:769–777` |
| D11 | Scrollback fills with duplicated screen copies | Scrollback derived from clear-event snapshots (ED2 / ED0-at-home preserve) — inference duplicates | `grid.rs:879–904` |
| D12 | Scrollback offset can diverge from the view (live instance: RIS resets only the grid's copy) | The offset is stored twice (`Session` and `DamageGrid`) | `session.rs:214`, `grid.rs:93–94, 727` |
| D13 | DSR cursor reply reports impossible column `cols+1` in the deferred-wrap state | Phantom column exposed unclamped | `perform.rs:337–354` |
| D14 | Capsule pane scrollbar diverges from the canonical jackin' scrollbar (hand-painted `█`, no track, no click-to-jump) | Shared geometry reused but rendering hand-rolled instead of the shared component | `view.rs:168–205` vs `jackin-tui/src/components/scrollable_panel.rs` |
| D15 | Input/output latency floored at 33 ms | Fixed render tick | `daemon.rs` render ticker |
| D16 | Spurious damage on every LF (wasted emission) | LF marks the new cursor row dirty though no cells changed | `perform.rs:16–21` |

## 3. Target architecture

```
PTY bytes ──→ DamageGrid (vte parse; damage recorded at mutation; grapheme-cluster cells)
                  │
                  │  damage = "did anything change?" signal + observation API (never an emit path)
                  ▼
  state mutation (any source: PTY, input, focus, scroll, dialog, context)
                  │  bumps frame_generation
                  ▼
  render loop:  generation moved? → ratatui::Terminal::draw(full widget tree)
                  │  Ratatui diffs current vs previous buffer (the one true client model)
                  ▼
  SocketBackend encoder (cells + cursor + modes + hyperlinks, reconciled)
                  ▼
  ClientWriter — the only socket writer: \x1b[?2026h … frame … \x1b[?2026l
                  (out-of-band bytes flush only at frame boundaries)
```

### 3.1 Single render path

Every frame is `Terminal::draw` of the full widget tree: status bar, pane bodies (`PaneBodyWidget` reading each pane's `DamageGrid` via borrowed `GridView`), borders, scrollbars, bottom chrome, dialogs, selection, spawn-failure banner. There is no second emit path: no direct grid-patch tier, no raw cell-region appends. With exactly one writer, Ratatui's previous buffer is the true model of the client screen *by construction* — the stale-cell class (D1) is structurally impossible. Diff cost is ~15k packed-cell compares per 250×60 frame (sub-millisecond); `DamageGrid`'s dirty tracking decides *whether* to compose, never *what* to emit.

### 3.2 Derived rendering

Event and input handlers only mutate state; none of them compose or request frames. Every mutation bumps `Multiplexer.frame_generation`; the render loop composes when the generation moved since the last frame. There are no repaint tiers, no `pending_full_redraw` / `pending_diff_redraw` / `dirty_panes` / `pane_body_repaint_pending` / `pane_chrome_dirty` flags, and no byte-cache for chrome — the buffer diff is the only "what changed" computation. `FullRedrawReason` survives only as (a) wipe policy — `\x1b[2J` precedes the frame for `FirstAttach` and `Resize` only — and (b) telemetry labels for `--debug` traces. This removes the D2 class ("state changed, nobody requested the right repaint") and matches the Elm rule the project's TUI architecture docs already mandate.

### 3.3 One ClientWriter

A single type owns the attach socket. `write_frame(bytes)` wraps every non-empty frame in `?2026` begin/end so the outer terminal applies it atomically (D6, D7's tearing half). `enqueue_out_of_band(bytes)` — clipboard OSC 52, window title, kitty-keyboard bytes — buffers and flushes only at frame boundaries, never mid-frame. Nothing else can reach the socket; the interleaving class is gone.

### 3.4 The frame model is more than cells

The composed frame carries, and the encoder reconciles desired-vs-last-asserted on every frame:

- **Cells** — Ratatui buffer diff.
- **Cursor** — position, visibility, style (DECSCUSR), derived per frame from the focused pane's grid and view state (hidden while a dialog is open, while browsing scrollback, before first output). No ad-hoc cursor appends; focus swap is not a special case because every frame reconciles.
- **Modes** — one `TerminalModeState` derived from the focused pane's grid: bracketed paste, application cursor keys, kitty-keyboard stack top. Replaces the three hand-maintained assertion lists (D4, and the class that produced the DECSCUSR leak in D5).
- **Hyperlinks** — a per-rect URI layer; the encoder emits `OSC 8` open/close around those cells during normal emission. No raw overlay writes (closes the last D1 loophole).

### 3.5 Encoder rules

`SocketBackend` is the cell→ANSI encoder under Ratatui: SGR state cache, cursor tracking, mode/cursor reconciliation, hyperlink brackets. The skip-the-CUP optimization applies only across runs of single ASCII printables (0x20–0x7E); after any other glyph the next cell gets an explicit `\x1b[row;colH` (D8 — correct regardless of the outer terminal's ambiguous-width configuration).

### 3.6 Passthrough policy

Sequences that are not cell content: query replies (DA, DSR, DECRQM, kitty query) answer **to the agent** from the grid's own state, never the host. Unknown CSI is **default-denied** with a documented allowlist (kitty keyboard push/pop, modifyOtherKeys), every drop `cdebug!`-logged. DECSTR is handled inside the grid (attrs, margins, wrap, cursor visibility reset) and never forwarded. The agent's `?2026` toggles are absorbed — the capsule's own frame brackets supersede them (D5, D6). OSC policy (title/clipboard/notification/hyperlink gating, OSC 7 retention) is unchanged.

### 3.7 jackin-term model correctness

- Cells hold **grapheme clusters** (`unicode-segmentation`): combining marks join the preceding cell; VS16/ZWJ sequences stay whole; cluster width drives wide/continuation flags (D9).
- Overwriting a wide **lead** blanks its continuation cell and marks both dirty (D10).
- DSR/CPR replies clamp the phantom column to `cols` (D13).
- The scrollback view offset has one owner — the grid; the session delegates (D12).
- Scrollback retention semantics are explicit, not heuristic (D11). Two correct candidates, decided with fixtures during implementation: (a) scrollback = scroll-evicted rows only (classical mux semantics; duplication impossible; cleared-but-never-scrolled screens are not retained — a real capability tradeoff to surface); (b) preserve-on-clear with exact dedupe (content-mutated flag plus byte-equality against the last preserved block). The decision and rationale land in `terminal-model.mdx`.
- LF no longer marks undamaged rows (D16).
- Existing correct behaviors stay: deferred wrap (DECAWM), BCE blanks, scroll-region damage, replies-to-agent, DECRQM declining mode 2027.

### 3.8 Scroller semantics

- Any offset change bumps the frame generation; the next frame repaints body and footer together (D2).
- While scrolled, the offset grows by the rows newly evicted into scrollback, then clamps — the view is anchored to content (D3).
- The cursor is hidden whenever the view is not live, via the frame model (D4).
- Typing snaps to live; wheel-down to offset 0 returns to the live view with no special case — it is just another state change.

### 3.9 Scrollbar = the shared component

The pane scrollbar renders through `jackin-tui`'s `scrollable_panel` family — `ScrollbarStyle::Line` (`┃`) thumb, `·` track, shared theme colors, `TailScroll::to_top_offset` bridging tail-scroll offsets to the panel renderer, click-to-jump via `scrollbar_offset_for_track_position` (D14). Rule codified in the TUI docs: every scrollbar in jackin' renders through these functions; hand-painted thumbs are a review-blocking violation (mirrors the existing `select_list` rule).

### 3.10 Pacing

Event-driven composition with a cadence cap: compose immediately when the last frame is older than the cap, otherwise schedule at the cap (D15). Atomicity comes from `?2026`, not from pacing.

### 3.11 Component roles after the change

| Component | Role |
|---|---|
| `jackin-term::DamageGrid` | Terminal emulation model per pane: vte parsing, grapheme cells, scrollback, mode tracking, damage (frame-skip + observation), passthrough events |
| `PaneBodyWidget` + the capsule widget tree | All visible content, including pane bodies, chrome, dialogs, banner — pure Ratatui |
| `ratatui::Terminal` | The double-buffer and diff — the single client model |
| `SocketBackend` | Cell→ANSI encoder + cursor/mode/hyperlink reconciliation |
| `ClientWriter` | The only socket writer; `?2026` frame brackets; frame-boundary flushing of out-of-band bytes |
| `jackin-tui` shared components | Scrollbars, pickers, dialogs — identical across host console and capsule |

## 4. Invariants (mechanically enforced)

- **I1 — screen == model.** After every frame, a virtual terminal fed the emitted bytes equals the pane grid (cells, attrs, cursor) within the pane rect. Enforced by `assert_screen_matches_model` across every scenario in `crates/jackin-capsule/src/daemon/render_conformance_tests.rs` (e.g. `stream_keeps_screen_equal_to_model`, `full_scroll_cycle_keeps_screen_equal_to_model`) in CI.
- **I2 — one writer.** No code path outside `ClientWriter` writes to the attach socket. Enforced by ownership: the sender lives only in `crates/jackin-capsule/src/client_writer.rs` (`tx` is private; `attach`/`take` are the only handles) — plus the review hard rule in `multiplexer-design-rules.mdx`.
- **I3 — atomic frames.** Every non-empty frame is `?2026`-bracketed and out-of-band bytes never appear inside a frame — by construction in `ClientWriter::write_frame` (`crates/jackin-capsule/src/client_writer.rs`), which drains the out-of-band queue ahead of the bracket pair in one socket write.
- **I4 — no screen erase outside FirstAttach/Resize.** Enforced by `wipe_policy_erases_only_on_first_attach_and_resize` in `crates/jackin-capsule/src/daemon/tests.rs`.
- **I5 — modes/cursor reconciled every frame** from the focused pane's grid; no assertion site outside the encoder. Enforced by `mode_reconciliation_resets_agent_modes_on_focus_swap` and `cursor_reconciliation_hides_cursor_while_scrolled` in `crates/jackin-capsule/src/daemon/tests.rs`, plus `assert_cursor_contract` in the echo-back harness.
- **I6 — unknown CSI never reaches the client.** Enforced by `unknown_csi_is_default_denied_and_carried_as_dropped` and `kitty_and_modify_other_keys_stay_on_the_forward_allowlist` in `crates/jackin-term/src/grid/model_correctness_tests.rs`; allowlist additions require a documented sequence + reason in `multiplexer-design-rules.mdx`.
- **I7 — scrollbars render through the shared component.** Enforced by `pane_scrollbar_renders_shared_component_glyphs_only` and `scrollbar_click_jumps_scrollback` in `crates/jackin-capsule/src/daemon/tests.rs`, plus the review-blocking rule in `reference/tui/components.mdx`.

## 5. Implementation order

Four PRs plus a no-code evidence stage. Order is load-bearing: relief → safety net → structure → model correctness. Each PR is independently green and shippable; one concern per PR; every commit pushed in the turn it is created.

### Stage 0 — evidence capture (no code; runs alongside PR 1)

1. Operator repro session: `cargo run --bin jackin -- console --debug`, Codex pane + Claude Code pane, heavy streaming, scrollback in/out, focus swaps, dialog open/close. Share the run id.
2. From `~/.jackin/data/diagnostics/runs/<run-id>.jsonl` extract: every `forwarding unhandled CSI to client` line (the real CSI inventory feeding the §3.6 allowlist); `render:` lines around a visible corruption (frame-tier traces); `session feed_pty bytes` hex lines (raw PTY streams → PR 2 fixtures).
3. Append the CSI inventory as a table to this file.

### PR 1 — scroller correctness + scrollbar reuse + convergence stopgap

Branch: `fix/capsule-scrollback-redraw`. Immediate relief for D2–D4, D14, and most of D1's daily pain. Steps 2–3 are deliberate symptom-layer scaffolding — the root cause (request-flag scheduling) is removed by PR 3; they ship first because the relief is correct on its own and the structural change belongs in its own PR.

1. **Resolve the `Terminal::clear()` ambiguity (blocks step 5).** `compositor.rs:62–71` and `socket_backend.rs:364–373` disagree about which backend method `Terminal::clear()` calls (one claims `clear_region(All)` → `2J`, the other the no-escape `Backend::clear`). Read the pinned ratatui source; if it emits `2J`, add a one-shot suppress flag to `SocketBackend::clear_region`; fix the stale comment in the same commit.
2. **Repaint-pending on offset change.** `session.rs::scroll_by` (676) and `scroll_to_live` (693) set `pane_body_repaint_pending = true` when the offset changed → the direct-patch path's precondition (compositor.rs:558–564) auto-rejects; the empty-frame transition (D2) becomes impossible.
3. **Wheel frames use the documented reason.** `input_dispatch.rs:420–431`: on `moved`, compose via `FullRedrawReason::ScrollbackMovement` (`tui/update.rs:24`) so body and footer repaint together on every scroll step including offset→0.
4. **Anchoring.** `session.rs::feed_pty` (846–851): capture `scrollback_len()` before/after `process()`; in the was-scrolled branch, `scrollback_offset += delta` before clamping (D3). Alt screen yields delta 0; ED3/`ScrollbackClear` still resets.
5. **Convergence stopgap.** Reset the Ratatui baseline (no screen-erase byte) at the top of `compose_ratatui_frame` so every Ratatui frame re-emits all cells and converges the physical screen. This becomes the permanent no-`2J` repaint in PR 3 — not throwaway.
6. **Cursor hidden while scrolled, everywhere.** `session.rs::current_mode_state` (1060–1083): emit `\x1b[?25l` when `scrollback_offset != 0` (D4 until PR 3's reconciliation subsumes it).
7. **Scrollbar unification.** Replace `view.rs::apply_pane_scrollbar`'s hand-painted loop with the shared `scrollable_panel` render functions: `TailScroll::new(offset).to_top_offset(filled + interior_rows, interior_rows)`, `ScrollbarStyle::Line`, `SCROLLBAR_TRACK`, shared theme colors (D14). Acceptance: glyph-identical to the workspaces screen's Global-mounts scrollbar.
8. **Tests** (`daemon/tests.rs`, `tui/view/tests.rs`): wheel-to-zero produces a frame repainting body + footer; feed-while-scrolled keeps the top row stable; `current_mode_state` contains `?25l` when scrolled; scrollbar glyphs come from the shared constants.
9. **Manual smoke** (`--debug`): stream Codex; wheel up 3 pages → view holds still; wheel down to bottom **wheel only** → input box + live footer return; type while scrolled → snap; focus swap while scrolled → no cursor in history.

### PR 2 — echo-back conformance harness + fixtures (test-only)

Branch: `chore/capsule-render-conformance`. The safety net PR 3 is judged against; zero behavior change.

1. **Harness:** `crates/jackin-capsule/src/daemon/render_conformance_tests.rs` (`#[cfg(test)]`; `Multiplexer` is crate-private), reusing `daemon/tests.rs` constructors.
2. **VirtualClient:** a second `DamageGrid` sized to the terminal; `apply(&frame_bytes)` = `process()`. jackin-term emulating the outer terminal closes the loop; `?2026` parses harmlessly.
3. **Invariant I1 assertion** after every composed frame: cell-exact equality (grapheme, fg, bg, modifiers, wide flags) over the pane rect; cursor position/visibility per the frame-model contract. Drive composition deterministically — direct `compose_pending_frame()` calls, no ticker, no sleeps.
4. **Fixtures:** `crates/jackin-capsule/tests/fixtures/pty/<agent>-<scenario>.bin` from Stage-0 JSONL; new `jackin-xtask pty-fixture <run.jsonl> <session-label> <out.bin>` subcommand makes re-recording one command. Synthetic fixtures: ambiguous-width glyphs, VS16/ZWJ emoji, combining marks, DECSTR, wide-lead overwrite.
5. **Scenarios:** Codex stream + full scroll cycle (incl. wheel-to-zero), Claude alt-screen session, focus swap mid-stream, resize mid-stream, dialog open/close over streaming, selection. Cases that still fail after PR 1 get `#[ignore = "fixed by PR 3"]` / `"fixed by PR 4"` — the executable spec.

### PR 3 — single writer + derived rendering (the core)

Branch: `refactor/capsule-single-render-path`. Merge criterion: **PR 2's PR-3-tagged `#[ignore]` cases flip green; nothing regresses.**

1. **`ClientWriter` (§3.3).** Move the socket sender behind the type; `write_frame` wraps `?2026`; `enqueue_out_of_band` flushes at frame boundaries only. Delete every other send site.
2. **Delete the patch tier.** Remove `compose_direct_dirty_pane_frame` and `SocketBackend::draw_grid_patch`; keep `GridPatch` only if the terminal-observation roadmap item still consumes it.
3. **Derived rendering (§3.2).** Add `frame_generation`; bump on every mutation site; render loop composes on movement. Delete `pending_full_redraw`, `pending_diff_redraw`, `dirty_panes`, `pane_body_repaint_pending` (PR 1 scaffolding), `pane_chrome_dirty`, `last_bottom_chrome`, and every composed-frame return from `input_dispatch` arms — handlers mutate state only. `FullRedrawReason` → wipe policy (`2J` on `FirstAttach`/`Resize` only) + telemetry. The PR 1 baseline-reset becomes the permanent repaint mechanism.
4. **Frame-model cursor + modes (§3.4).** One `TerminalModeState` derived per frame; encoder reconciles desired-vs-last-asserted. Delete `append_cursor_state`-as-append, `drain_mode_transitions`, `current_mode_state`, `focus_swap_reset`.
5. **Hyperlink layer (§3.4).** Frame carries per-rect URIs; encoder emits `OSC 8` brackets during cell emission; delete the raw overlay appends.
6. **Banner + chrome become widgets.** `spawn_failure_banner` → `Multiplexer.spawn_failure: Option<String>` rendered as a top-row Paragraph; bottom chrome renders inside the Ratatui frame (the buffer already spans the full terminal).
7. **Encoder hardening (§3.5).** CUP-skip only across ASCII printables.
8. **Event-driven pacing (§3.10).**
9. **Perf measurement.** Record p95 frame duration + bytes/frame under the Codex fixture before/after in the PR description. Documented escape hatch if a pathological size measures hot: per-pane damage skips re-rendering clean pane widgets into the buffer — never a second emit path or writer.

### PR 4 — jackin-term model correctness + passthrough gating (splittable 4a/4b)

Branch: `fix/capsule-csi-gating` (+ `fix/jackin-term-fidelity` if split). Driven by the Stage-0 CSI inventory.

1. **Default-deny unhandled CSI (§3.6).** Allowlist: kitty keyboard push/pop, modifyOtherKeys. Every drop `cdebug!`-logged; allowlist additions documented in `multiplexer-design-rules.mdx`.
2. **DECSCUSR per-pane.** Grid tracks `cursor_style` from `CSI {n} SP q`; flows through the PR 3 reconciliation; no separate assertion site.
3. **DECSTR in-grid** (`'p'` with `!` intermediate): reset attrs, margins, `pending_wrap`, cursor visible, application-cursor off, bracketed-paste off, saved cursor := cursor; never forwarded.
4. **Absorb agent `?2026`**: drop the passthrough event; capsule frame brackets supersede it.
5. **Grapheme-cluster cells (§3.7).** `unicode-segmentation` in the write path; conformance fixtures: combining accents, VS16 emoji, ZWJ family emoji, flag pairs.
6. **Wide-lead overwrite fix**: blank the continuation cell, extend the dirty range.
7. **DSR clamp**: reported column = `min(cursor_col, cols-1) + 1`.
8. **Scrollback-offset single owner**: delete `Session.scrollback_offset`; grid owns, session delegates.
9. **Scrollback retention decision (§3.7)**: choose (a) evicted-rows-only or (b) exact-dedupe preserve, with fixtures; record in `terminal-model.mdx`.
10. **Remove the spurious LF mark.**
11. Tests: grid unit tests per item; un-ignore the PR 2 DECSTR/width/grapheme fixtures; conformance for DECSCUSR reconciliation.

## 6. Verification

### 6.1 Definition of done (every PR)

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace
eval "$(cargo run --bin build-jackin-capsule -- --export)"   # any crates/jackin-capsule/ change
cargo run --bin jackin -- console --debug                     # smoke per the PR's verify list
```

Plus: PR body from `.github/PULL_REQUEST_TEMPLATE.md` with the Verify-locally block and the `### jackin-capsule smoke` section (the eval line stays in Checkout before any `jackin` invocation); docs updates ride the same PR (§7); roadmap freshness check before marking ready; `Co-authored-by: Claude <noreply@anthropic.com>` + DCO sign-off on every commit; operator authorizes merge.

### 6.2 Echo-back harness (the I1 enforcer)

Replay recorded PTY transcripts through the multiplexer; feed the emitted client bytes into a virtual terminal (a second `DamageGrid`); assert cell-exact equality with the pane grid after every frame, plus cursor position/visibility per the frame-model contract. Fixtures recorded from real Codex/Claude Code sessions via `--debug` JSONL plus synthetic Unicode/CSI cases. Lands in PR 2, before the structural change, so PR 3 and PR 4 are red-then-green.

### 6.3 Manual smoke matrix (per PR, `--debug`)

Streaming Codex + Claude panes: scroll cycle (hold-still, wheel-only return, snap-on-type), focus swaps while scrolled, tab switch / zoom / dialog close under streaming (no flash, no residue), resize mid-stream, selection + copy, cursor shape and visibility across pane swaps.

## 7. Documentation obligations (same-PR, per repo rules)

| PR | Docs |
|---|---|
| PR 1 | `tui/components.mdx` — scrollbar rule (every scrollbar renders through `scrollable_panel`; hand-painted thumbs are review-blocking); `multiplexer-design-rules.mdx` — scrollback cursor + repaint-on-offset-change; roadmap freshness check |
| PR 2 | `jackin-term/README.md` — emit-side conformance harness; `TESTING.md` if fixture recording needs operator docs |
| PR 3 | `terminal-model.mdx` — single-writer invariant + frame model; `multiplexer-design-rules.mdx` — `?2026` contract, ClientWriter rule; `roadmap/jackin-capsule.mdx` — render-model status; new ADR "capsule single render path" beside ADR-003/004 |
| PR 4 | `multiplexer-design-rules.mdx` — CSI default-deny + allowlist with reasons; `terminal-model.mdx` — DECSTR/DECSCUSR ownership + scrollback retention decision |

## 8. Risk register

| Risk | PR | Mitigation |
|---|---|---|
| `Terminal::clear()` backend-method ambiguity (two comments disagree) | 1 | Resolved as step 1 before anything depends on it; suppress-flag fallback designed |
| Anchored offset growth while parked in history | 1 | Clamp at `filled` unchanged; wheel-down path unchanged |
| Harness flakiness | 2 | Deterministic composition: direct `compose_pending_frame` calls, no ticker, no sleeps |
| Request-tier deletion touches many call sites | 3 | Compiler-driven (deleting the flags surfaces every site); harness covers every scenario the tiers served; `FullRedrawReason` kept as telemetry so `--debug` traces stay comparable |
| Chrome/hyperlink regression when chrome becomes widgets | 3 | Hyperlinks are frame data emitted by the encoder; harness asserts chrome cells + link presence |
| `?2026` on a non-supporting outer terminal | 3 | Unknown private modes are ignored by spec; no fallback needed |
| Single-path compose cost on very large terminals | 3 | Measured in-PR; documented escape hatch (per-pane widget skip) stays inside the single model |
| Over-aggressive CSI deny breaks an agent feature | 4 | Stage-0 inventory first; allowlist additions documented; every drop `cdebug!`-logged for `--debug` triage |
| Grapheme segmentation changes width behavior for existing content | 4 | Conformance fixtures cover the Unicode matrix; DECRQM mode-2027 decline unchanged |

## 9. Future direction (recorded, not scheduled)

Structured frame protocol to a host-side renderer (the wezterm-mux/mosh shape: grid deltas as typed messages over the attach socket, rendered by the host process). Strongest long-term evolution — it would serve terminal observation, session resume, and multi-client attach from one mechanism — and it layers cleanly on top of this plan's frame model. Revisit after PR 4 ships.

## 10. References

- Zellij differential-rendering drift → duplicate panes; fix = synchronized output: <https://github.com/zellij-org/zellij/issues/4693>
- Ratatui rendering internals (draw → diff → swap; resize resets baseline): <https://ratatui.rs/concepts/rendering/under-the-hood/>; out-of-band write caveats: <https://ratatui.rs/faq/>, <https://github.com/ratatui/ratatui/issues/1116>
- Synchronized output (`?2026`) spec: <https://gist.github.com/christianparpart/d8a62cc1ab659194337d73e399004036>
- Clear-and-redraw as the canonical flicker trigger: <https://github.com/QwenLM/qwen-code/issues/1778>
- tmux client-terminal state ownership: <https://github.com/tmux/tmux/blob/master/tty.c>, <https://github.com/tmux/tmux/blob/master/screen-redraw.c>
- mosh screen-state diffing: <https://mosh.org/mosh-paper-draft.pdf>
- wezterm mux structured-delta protocol (future direction): <https://deepwiki.com/wezterm/wezterm/2.2-multiplexer-architecture>
- Prior art for everything-through-`Terminal::draw` muxes: psmux (via <https://github.com/rothgar/awesome-tuis>); pane-widget pattern: <https://github.com/a-kenji/tui-term>
- Internal: `crates/jackin-term/README.md`, `reference/capsule/terminal-model.mdx`, `reference/capsule/multiplexer-design-rules.mdx`, `reference/roadmap/jackin-capsule.mdx`, ADR-003, ADR-004, `reference/tui/components.mdx`
