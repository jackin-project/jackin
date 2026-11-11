> Research input dated 2026-11-02. It is evidence, not authority. If it disagrees with [DESIGN.md](../DESIGN.md), DESIGN.md is correct.

# terminal-components-baseline: reuse and visual-identity analysis

Goal: decide whether the new jackin console can depend on https://github.com/tailrocks/terminal-components-baseline as a crate, list every screen and component, map them to PLAN.md P9.1, and define how "visually identical" is proven.

Checked 2026-11-02. Clone: `<session scratch>` at `main` = `95bbcbbd83d5e97d7b296e4af9dd586aec321dab` (3 commits; last 2026-11-01). Captures: `.../scratchpad/tcb-captures/` (246 text files, plus 98 baseline conversions in `baselines/`). Capture tools: `.../scratchpad/tools/`. No change was made to jackin. The only edit in the clone was a temporary one-cell mutation of a baseline for a negative test, reverted with `git checkout` (`git status` clean).

## 1. What the repository is

- Purpose: "design reference implementation" and visual/interaction authority for Tailrocks terminal UIs (`docs/current-state.md`, `docs/product.md`). One Rust crate holding a component library plus four deterministic demo apps. All app backends are simulated in memory.
- Names disagree: GitHub repo `terminal-components-baseline`; Cargo package `junie-tui`; README title "Termrock"; AGENTS.md calls the jackin binary `jackin-preview` (clap name) while the Cargo bin is `jackin`. The repo is the import of the `design/interaction-polish` branch of `tailrocks/terminal-components-claude` (commit c0afa23).
- Layout: a single package, `[workspace] members = ["."]` (`Cargo.toml`). The multi-crate split in `docs/architecture.md` (core, layout, theme, text, runtime, ..., facade `termrock`) "does not exist in this tree yet" (`docs/current-state.md`).

| Path | Content | LOC |
| --- | --- | --- |
| `src/core/` | ids, events, intents, focus, hit regions, keymap, keyboard protocol, scroll, selection, text buffer | part of 62k lib total |
| `src/shell/` | App/Shell, Router, Page trait, Cx/Request, Chrome, Footer, StatusLine, KeyRouter, Clipboard | |
| `src/ui/` | RenderCtx, layers/modals/popups, layout, master-detail, pane tree, scroll view, leave/quit policy, reveal | |
| `src/widgets/` | 53 public component modules | |
| `src/theme.rs` 2588, `motion.rs`, `clock.rs`, `runtime.rs` 772, `cli.rs`, `catalog.rs` 3638, `docs.rs` | theme, motion modes, sim clock, terminal runtime, clap args, capability catalog | |
| `src/bin/showcase/` | 54-page component catalog | 41k |
| `src/bin/jackin/` | jackin demo app (manager, editor, accounts, usage, settings, cockpit, capsule, rain) | 30k |
| `src/bin/tablepro/`, `src/bin/holla/` | database workbench, launcher/finder/disk-cleanup demos | |
| `tests/functional/` | 93 PTY tests + 98 frozen `.tui.json` baselines (110 MB) | |

- Toolchain: edition 2024, `rust-version = "1.99"`, `rust-toolchain.toml` 1.99.1, mise + mr-boxington (`mbx`) cargo wrapper. jackin DESIGN.md D02 pins the same Rust 1.99.1 and mbx 1.29.1, edition 2024.
- Dependencies (`Cargo.toml`, `Cargo.lock`): ratatui `0.30` with feature `crossterm_0_29` (locked 0.30.2, ratatui-core 0.1.2, ratatui-widgets 0.3.2, ratatui-crossterm 0.1.2); crossterm 0.29.0 only through ratatui's re-export (`ratatui::crossterm`); clap 4.6 (derive); unicode-width 0.2; unicode-segmentation 1; libc 0.2 (unix). Dev-only: `tuiscotti-tui-test =0.2.1`, `tuiscotti` git rev d8d54155947f6303a41dfd6079b53c0abc16f29a, serde_json. No `termrock` dependency anywhere.
- License: `Cargo.toml` says `license = "MIT"`. There is no LICENSE file, and GitHub reports `licenseInfo: null`. The manifest is the only grant.
- Rendering model: not a pure Elm loop. `runtime::Application` has `handle(Input) -> Outcome`, `render(&mut Frame)`, `should_quit`, `tick_interval` (`src/runtime.rs:23`). A synchronous polling loop (`event_loop`, `runtime.rs:410`) drains input with coalescing and ticks. `shell::Page<A>` pages hold their own widget state, mutate a shared `State` in `on_key/on_msg/on_tick`, and queue effects as `Request` values (`Go`, `Open` layer, `Status`, `Progress`, `Copy`, `Back`; `src/shell/cx.rs`). `render(&mut self, area, buf, &mut RenderCtx, &State)` takes `&mut self` and registers hit regions in `RenderCtx`. Widgets emit `*Action`/`*Cmd` enums and return `Outcome::{Changed,Consumed,Ignored}`. `Input` is only Key, Mouse, Resize, Paste, Tick (`src/core/event.rs:31`): there is no external-message input.
- Theming tokens: `theme::Theme` (about 60 semantic color tokens: canvas, surface, surface_elevated, surface_overlay, field, popover, highlight, accent (+hover/pressed/bg), focus, text_primary..text_ghost, error/warning/success/info/link, selection_bg, border_subtle/strong, scroll_track, syntax_*), 10 `Plane`s, per-plane `Ladder{rest,hover,pressed}`, `StateModifiers` for non-color levels, `Glyphs`, `Spacing`, `HintBarStyle`, `InactiveSelection`. Four `ColorLevel`s (TrueColor, Ansi256, Ansi16, Mono/NO_COLOR) with separate palettes, two `Charset`s (Unicode, Ascii), `Tone` and `StatusKind` enums, three `Motion` modes (Full, Reduced, Paused). Palette (docs/product.md): canvas #000000, chrome #111111, card #18181b, field #1e1e22, highlight #2f5aa8, accent #48e054 (`src/theme.rs:402`, `GREEN = rgb(0x48e054)`).
- Demo binaries: yes, four. `showcase` is `default-run`. Common flags: `-c/--color truecolor|256|16|none`, `--glyphs unicode|ascii`, `-m/--motion full|reduced|paused`, `-f/--frame N`.

## 2. Can jackin depend on it as a crate?

Verdict: yes for the library half, as a git dependency. No for the jackin screens: they exist only inside the `jackin` binary and are coupled to the demo's `World`/`sim` types. Not published; there is nothing to pin by version.

Package and dependency line (the lib target is `junie_tui`):

```toml
junie-tui = { git = "https://github.com/tailrocks/terminal-components-baseline", rev = "95bbcbbd83d5e97d7b296e4af9dd586aec321dab" }
```

Proof: a scratch consumer (`.../scratchpad/consumer-test`, edition 2024, `ratatui = "0.30"`, `crossterm = "0.29"`, that line) passes `cargo check` and uses `junie_tui::theme::Theme` and `junie_tui::core::event::Key` with `crossterm::event::KeyCode`. `cargo tree -i crossterm` shows one crossterm (0.29.0) and one ratatui (0.30.2). Dev-dependencies (tuiscotti) are not pulled by consumers. Features: the crate declares none.

Public API (all `pub mod`, `src/lib.rs`): `core`, `shell`, `ui`, `widgets` (53 modules), `theme`, `motion`, `clock`, `runtime`, `cli`, `catalog`, `docs`.
- Components: `widgets::{button, choice (Checkbox, RadioGroup, Toggle), input::TextInput, textarea, select, picker, option_list, list::ListBox, tree::TreeView, table, grid, tabs, menu, dialog, form_dialog, info_dialog, choice_dialog, file_browser, panel, progress, steps::StepRail, meter_list, notice, empty, props, statusbar, hintbar, keyhint, help, toolbar, breadcrumb, rail, nav_list, scrollbar, splitter, code, diff, viewport, ...}`. Index in section 4.
- Layout: `ui::{layout::Stack/Split, master_detail, pane_tree, scroll_view, columns, measure, modal::Frame/ModalStack, popup::{Dismiss,Placement}, layer::Layer, tooltip, context}`.
- Theme: `theme::{Theme, Plane, Ladder(s), StateModifiers, ColorLevel, Charset, Tone, StatusKind, InactiveSelection}`; `Theme::for_level(level)`, `Charset::apply`.
- Keymap and routing: `core::keymap::{NavKeys, Nav, KeyBinding, Actions}`, `shell::{KeyRouter, ShellKeys, ShellAction}`, `ui::leave::{QuitKeys, Stakes, Leave}`, `core::keyboard::KeyboardProtocol` (kitty protocol negotiation).
- Shell: `shell::{App, Shell, Router, Page, PageTypes, Cx, Request, Chrome, Footer, StatusLine, Clipboard, run}`; `runtime::{Application, TerminalSession, Coalesce, run}`.

What blocks or complicates reuse:
1. The jackin screens are binary-only: `src/bin/jackin/{app.rs, screens/*, domain/*, sim/*, rain.rs}`, 30k LOC, built on `World`, `Msg`, `Go`, `Workspace`, `Account`, `Agent::ClaudeCode` demo types. They cannot be depended on. They are the only authority for jackin's screen layouts.
2. The showcase "Screens" pages (Workspaces, Accounts & usage, Launch cockpit, Settings) are separate bin code with different fixture data (`src/bin/showcase/pages/*.rs`), also binary-only.
3. No attach hand-off API. `TerminalSession::leave()` is public, but `reenter()` and `Restoration::reacquire()` are private, and `run()` owns the session (`src/runtime.rs:121-318`). The only re-entry path is SIGTSTP job control. P9.1 needs "leave alt screen, run `openshell connect`, restore". Upstream must add e.g. `Application::take_foreground() -> Option<ForegroundJob>` or a public `TerminalSession::run_foreground(FnOnce)`.
4. No external-event input. The loop polls and ticks (`tick_interval`). jackin must drain its tokio mpsc `Msg` receiver inside `Input::Tick` (latency up to one tick) or upstream adds `Input::Wake`/a waker. This works but is polling, not push.
5. Update model differs from PLAN P9.1 ("one update loop with model, messages, and effects. Keep the render path pure."): baseline `render` takes `&mut self` and `&mut RenderCtx` (hit registration, scroll state). Mapping: `World` = model, `Page::on_msg` = update, `Request`/`Go` = effects. Needs an architect decision, see section 6.
6. `unsafe` and libc in the lib (`src/runtime.rs:269-397`: `signal`, `raise`, `isatty`, `poll`). jackin's old workspace denies `unsafe_code` for its own crates; a dependency's unsafe does not trip that lint, but `cargo deny`/review policy may care.
7. The lib pulls clap derive unconditionally (`src/cli.rs`) and reads `JUNIE_TUI_*` env hooks (`JUNIE_TUI_TEST_CLOCK`, `JUNIE_TUI_CLIENT_PLATFORM`, `JUNIE_TUI_START_DELAY_MS`, `JUNIE_TUI_FIRST_POLL_DELAY_MS`). Harmless, but a `cli` feature would drop clap.
8. License: manifest-only MIT, no LICENSE file. Compatible with jackin's Apache-2.0 repo, but should be a real file upstream before jackin ships a dependency on it.
9. Stability: git-only, no tags (`git ls-remote --tags` empty; the `visual-baseline` tag the docs mention is not in this repo), three commits, AGENTS.md says "keep dependencies on latest, upgrade monthly", and `docs/decisions.md` says public APIs may break only by deliberate decision. Pin by `rev` and treat bumps as reviewed upgrades with golden re-approval.
10. Version conflicts: none. ratatui 0.30.2 and crossterm 0.29.0 match jackin's plan (research/jackin-crates.md:211: "ratatui 0.30, crossterm 0.29").

Upstream changes needed (all in tailrocks/terminal-components-baseline), in priority order:
- (a) Move the jackin app's reusable screens into a library crate or module that takes a typed model through a trait (not `sim::World`): at minimum `manager`, `editor`, `accounts`, `usage`, `settings`, `cockpit` (progress rail, build-log overlay, failure dialog), `modals`, `rowmenu`, `inspect`, `config`, plus `domain/{account,workspace,usage,instance}` as plain data types. Keep `rain.rs`, `capsule.rs`, `sim/*`, `domain/fixtures.rs` in the demo binary. Without this step jackin has to copy about 20k LOC and keep it identical by hand.
- (b) Public foreground hand-off API and an external-wake input (items 3 and 4).
- (c) Add LICENSE; optional `cli` feature.
- Alternative owned by jackin: port the screens into `crates/jackin-tui` against the library only; identity then rests on the golden files (section 5).

Related repository: `tailrocks/termrock` (workspace v0.11.0, Apache-2.0, ratatui 0.30.2, crossterm 0.29, features `crossterm` and `serde`, 153 widget files, last push 2026-10-17). The pre-rewrite jackin console used it at rev `1ac0d0793b8cfd9edc61ced241e2b2ecb89162b4` (`git show 470fc5d6:Cargo.toml:352`, 165 `use termrock` lines). It is a different codebase from `junie-tui`. The baseline's docs name `termrock` as the future facade but also warn that matching names or APIs prove nothing about equivalence (`docs/direction.md`). Visual identity with the baseline therefore cannot be assumed from a termrock dependency.

## 3. Conflicts with recorded jackin decisions (owner must resolve)

- D16 deletes Capsule, tabs, panes, 1Password, daemon. The baseline jackin app has all of them: `screens/capsule.rs` (3053 LOC), `sim/pty.rs`, `sim/onepassword.rs`, `OpFlow`, daemon health, `[op]` environment references. "Every screen has an identical counterpart" cannot hold for these. Affected captures: `jackin-scenario-capsule-multi`, `jackin-scenario-outro-last` (ends in the capsule), `jackin-flow-capsule-multi-help`, `jackin-flow-launch-complete-capsule`.
- D04 (Codex is the only agent): the baseline launch picker is "choose Agent" over Claude Code, Codex, OpenCode, Grok Build.
- D01/D24 (no Docker run by jackin): the baseline cockpit labels are Docker-centric ("Docker build", `jackin-net`, Sidecar, Capsule, Hardline stages).
- Scope: "every screen in that repository" includes TablePro (database workbench) and Holla (34 launcher scenarios). They are different products with no jackin counterpart. Recommend scoping "identical" to: the showcase component pages (54 x 3 views) as component spec, and the baseline jackin app screens that survive D16.
- PLAN P9.1 proof asks for ratatui `TestBackend` tests; the baseline's AGENTS.md forbids TestBackend and in-process tests for its own estate (PTY only). The two can coexist (section 5).

## 4. Screens and components

### 4.1 How to run (from the clone; `cargo` here means `~/.cargo/bin/cargo`, because the mise shim refuses the untrusted `mise.toml` and I did not run `mise trust`)

```sh
cargo build --locked --bins
cargo run --bin showcase -- --page <name> --motion paused --frame 40       # 54 pages
cargo run --bin jackin   -- --scenario <name> --motion paused --frame 40   # 8 scenarios
cargo run --bin tablepro -- [--connect Development] --motion paused --frame 40
cargo run --bin holla    -- --scenario <name> --motion paused --frame 40   # 34 scenarios
```

Page names for `--page` are the lower-cased alphanumeric nav labels (`Disk cleanup` -> `diskcleanup`) plus aliases (`src/bin/showcase/app.rs:371`). Each showcase page has three views reachable with Tab then Right: Playground (default), Options (`Tab Right`), States (`Tab Right Right`). Quit with `q`; Holla uses Ctrl+Q.

### 4.2 Captures (text, 120x40, truecolor, via PTY plus pyte; tool `tools/capture.py`)

`<session scratch>`:
- `showcase-<page>.txt`: 54 Playground views. `showcase-<page>--options.txt` and `--states.txt`: 108 more, each checked to show the selected tab marker.
- `jackin-scenario-<name>.txt`: 8. `jackin-flow-*.txt`: 34 key-driven states (launch picker, workspace editor and its 5 tabs, new-workspace chain, accounts, add account, account detail, usage, settings and its tabs, help, instance expanded and inspect, delete and stop dialogs, prewarm, cockpit at frames 200/300/450, build log, cancel dialog, container info, failure dialog, first-use intro and manager, outro, capsule).
- `tablepro-*.txt`: 7. `holla-scenario-*.txt`: 34, `holla-flow-help-f1.txt`: 1.
- `baselines/<test_id>.txt`: all 98 approved `.tui.json` frozen baselines converted to text (`tools/baseline_to_text.py`). These are the repo's own approved frames, including the interaction states (dialogs, hover, resize 60x20, NO_COLOR) that no CLI flag reaches.
- Not captured: the 12-tick cockpit-to-capsule fade (`rain::HANDOFF_LEN = 12`, time-driven; only its endpoints are captured), intro/outro animation frames other than frame 0/1. Text captures drop color and style; use the `.tui.json` baselines or the tests for style.
- Determinism check: my PTY capture `jackin-scenario-launch-failure.txt` is byte-identical (text) to the approved baseline `baselines/functional__jackin_launch_failure.txt` (`diff` exit 0).

### 4.3 Showcase pages (all in `src/bin/showcase/pages/<file>.rs`; group per `NAV_ENTRIES`)

| Group | Pages (file) |
| --- | --- |
| root | Overview (`overview.rs`) |
| App shell | Chrome, Status & hints, Breadcrumbs & rails, Help & keys, Quit & leave, Focus & navigation |
| Inputs & forms | Buttons, Inputs, Text areas, Chips & selects, Forms, Checks & radios, Toolbars |
| Tables | Basic table, Editable, Data grid, Selection, Cell editors, Columns |
| Navigation | Lists, Trees, Sidebars, Tabs, Menus |
| Overlays | Dialogs, Pickers, Dialog kinds, Popups & tooltips, File browser |
| Feedback | Progress, Empty & error, Meters, Notices |
| Layout | Panels, Scrolling, Splits & stacks, Master-detail, Pane tree |
| Text & code | Code editor, Diff, Terminal, Text utilities |
| Screens | Settings, Task runner, Query workbench, Connections, Finder, Disk cleanup, Activity, Workspaces, Accounts & usage, Launch cockpit, Read-only report |

### 4.4 Jackin app (binary `jackin`, `src/bin/jackin/`)

Routes (`app.rs:54`): Intro, Manager, Prelude, Editor, Settings, Accounts, Usage, Cockpit, Handoff, Capsule, Outro. Modals: launch/session pickers, stop/delete/cancel/save dialogs, container-info and inspect, keyboard help, file browser, git-URL form, 1Password flow (`OpFlow`).
Scenarios: first-use, returning, accounts-mixed, launch-running, launch-failure, capsule-multi, outro-last, hard-cases. Keys: Enter launch, `n` new, `e` edit, `c` accounts, `u` usage, `s` settings, `?` help, `i` inspect, `t` stop, `d` delete, `w` prewarm, `b` build log (cockpit).

### 4.5 TablePro (`src/bin/tablepro/`) and Holla (`src/bin/holla/`)

- TablePro routes: Connections, Workbench (explorer, query tabs, results, history, filter editor, write gate).
- Holla pages (`screens/mod.rs:121`): Finder, Disk, PlanReview, Review (second gate), Trust, Activity, Cleanup, Files/browser, Snapshot; 11 concept scenarios and 23 `parity-*` scenarios.

### 4.6 Reusable components (library, by file)

- `src/widgets/`: action_row, brand (Lockup), breadcrumb, button (Button, ButtonRow), chips (Chip, ChipBar), choice (Checkbox, RadioGroup, Toggle), choice_dialog, code (CodeEditor), completion, detail_card, dialog (Dialog), diff, edit (validation), empty (EmptyState), field_common, file_browser, form_dialog, grid (DataGrid), help (HelpOverlay), hintbar, icon_button, info_dialog, input (TextInput), keyhint, keyref, list (ListBox), menu (ContextMenu, MenuBar), meter_list, nav_list, notice, option_editor, option_list, page_header, panel (Panel, ScrollPanel), picker, progress (bars, spinners, meters), props (PropsList), rail (SidebarRail), scrollbar, segments, select, splitter, statusbar, steps (StepRail), table, tabs, textarea, too_small, toolbar, tree (TreeView), viewport (TextViewport).
- `src/ui/`: columns, content, context, ctx (RenderCtx, Backdrop), edit_menu, fade, layer, layers, layout, leave, list_core, master_detail, measure, modal (Frame, ModalStack), pane_tree, popup, reveal, row, scroll_view, specimen, text, tooltip.
- `src/shell/`: app, chrome, clipboard, cx, footer, nav (Router), page, reach, router (KeyRouter), status.
- `src/core/`: edit, event, focus, hints, hit, id, intent, keyboard, keymap, list, nav, pointer, response, scroll, selection, shape, text, typeahead.

## 5. Mapping to jackin P9.1

| P9.1 jackin screen | Baseline counterpart | Fit |
| --- | --- | --- |
| Sandbox list | jackin Manager: workspace tree with an expandable instance roster (`jackin-scenario-returning`, `jackin-flow-instance-expanded`), actions Reconnect/Shell/Inspect/Stop/Purge (`screens/manager.rs`); showcase `Workspaces` | Layout reusable. Content differs: instances become sandboxes from `jackin ls` labels, no daemon/pane topology. The workspace-less sandbox list has no exact frame. |
| Launch flow (workspace, role, account) | Manager launch Picker `Launch · choose Agent` (`jackin-flow-launch-picker`), `Prelude` 5-step new-workspace chain (`jackin-flow-new-workspace`), account choice in the picker rows | Partial. Role and account steps are not separate screens in the baseline; agent step must go (D04). Compose from the same `Picker`/`Dialog` widgets. |
| Launch progress with build output | Cockpit: 11-stage rail, Docker build-log overlay with follow/scroll, cancel dialog, failure dialog, container info (`jackin-flow-cockpit-*`, `showcase-launchcockpit`) | Strong. Stage names and the failure text change (OpenShell stages, `buildx` output); widgets and frame are the same. |
| Attach hand-off | Cockpit "Hardline open · handing off" state (`jackin-flow-cockpit-complete-f300`) and the `Handoff` fade into Capsule | Weak. Real hand-off leaves the alt screen for `openshell connect`; no baseline frame for that and no library API (section 2, item 3). The Capsule itself is out under D16. |
| Workspace editor | Editor with tabs General, Mounts, Roles, Environments, Accounts (`jackin-flow-workspace-editor`, `-editor-*`, `-editor-save-dialog`), `showcase-forms/settings` | Strong. |
| Role editor | None as a screen. Closest: editor `Roles` tab (allowed roles), Settings `Trust` tab (role sources), `showcase-pickers` | No counterpart. New screen built from `ListBox`/`Picker`/`Dialog`. |
| Account editor (any provider) | Accounts: provider tree, detail, `New account` form with provider select (`jackin-flow-accounts*`, `-accounts-add`) | Strong. Credential-source options (1Password) must be replaced. |
| Default accounts and bindings | Star (default) markers in Accounts tree, editor `Accounts` tab, "choose at start" label | Partial. Provider default and workspace binding exist visually; role binding screen does not. |
| Docker setting (D24) | None. Closest: editor General checkboxes (`Keep awake`, `Git pull before launch`) | No counterpart. Add one `Checkbox` row to the editor General tab, and a warning `Notice` for the host-engine source. |
| Usage panel (5-hour, weekly) | Usage screen (read-only) and Accounts overview: `Session · 5-hour`, `Weekly · all models`, reset times, "per-workspace bound accounts" absent (`jackin-flow-usage`, `jackin-scenario-accounts-mixed`, `showcase-accountsusage`, `showcase-meters`) | Strong. Per-workspace bound-accounts view is new. |
| Doctor panel | None. Closest: `showcase-readonlyreport` (host snapshot with MeterList, facts, notices, Next actions) | No counterpart. Use the Read-only report pattern. |

Baseline screens with no jackin P9.1 counterpart:
- Jackin app: Intro and Outro rain (brand; keep or drop by owner decision), Capsule (tabs/panes), Settings screen (global commits/mounts/environments/agents/trust: only partly needed), 1Password flow, keyboard help (generic, keep), file browser (used by workspace create; keep).
- Showcase: all component pages (they are the spec, not screens): Task runner, Query workbench, Connections, Finder, Disk cleanup, Activity have no jackin use.
- TablePro: all screens. Holla: all 34 scenarios.

## 6. How to prove "visually identical"

Mechanisms the repository already has (all verified on this machine):
1. Frozen baselines: `tests/functional/baselines/**/*.tui.json`, 98 files, schema `tuiscotti.sample.v1`. Each stores every cell of a 120x40 screen: grapheme, width, fg, bg, bold/dim/italic/inverse/strike/blink, underline and color, hyperlink, plus cursor and the 256-color palette.
2. Gate: `App::visual("<id>")` in `tests/functional/support.rs:122` launches the real binary on a PTY (cleared env, `TERM=xterm-256color`, `COLORTERM=truecolor`, `TZ=UTC`, `LANG=C.UTF-8`, `JUNIE_TUI_TEST_CLOCK=1`), waits for 700 ms of stable cells, builds a tuiscotti state sample and calls `FrozenStore::check`; `assert!(outcome.is_pass())`.
3. Approval: only `tuiscotti accept --candidate <sample> --candidate-sha256 <sha> --expect-baseline missing --baselines tests/functional/baselines`.
4. Run: `cargo nextest run --locked --test functional --no-tests fail` (never `cargo test`).

Evidence I produced:
- Positive: `cargo nextest run --locked --test functional --no-tests fail -E 'test(jackin_cockpit_launch_failure_acknowledges) | test(jackin_first_use_empty_manager_and_intro)'` -> 2 passed, 91 skipped.
- Negative control: I changed the fg of one cell (col 60, row 5) in `jackin_launch_failure.tui.json` and reran. The test failed with `visual mismatch ... changed_cells: [ReportedCell { col: 60, row: 5, fields: ["fg"] }]`. The gate detects a one-cell colour change. File restored.

Proposed jackin gate, two tiers:

- Tier A, identity for screens whose content survives. Vendor the needed baseline `.tui.json` files (pinned to the baseline rev; they are 1.1 MB each) into jackin's test fixtures with the same `test_id`. Add a jackin PTY test (same harness: `tuiscotti-tui-test =0.2.1`, `tuiscotti` rev d8d5415) that runs `jackin` in a fixture mode fed with the baseline's own data (via `jackin-testkit` fixtures) and calls `FrozenStore::check`. Candidates, in order of survival under the decisions: accounts (`jackin_account_remove`), workspace editor (`jackin_workspace_save`, `jackin_editor_discard`), new-workspace chain, usage (`showcase_page_accountsusage`), cockpit failure and cancel dialogs (`jackin_launch_failure`), help (`jackin_help_ownership`), empty manager (`jackin_first_use_empty`). Pass means every cell, color and attribute equal.
- Tier B, structural identity for screens whose text must change (sandbox list, OpenShell stages, role editor, Docker setting, doctor). Add a masked comparison: render the jackin screen, then compare its cell grid to the nearest baseline frame with data-bearing cells masked, requiring equal box-drawing/gutter glyphs, fg/bg per plane, modifiers, and layout of the chrome (header, menu, footer, hints, panel borders, scrollbars). Tier B needs a small tool (xtask) that loads two `tuiscotti.sample.v1` files and diffs the non-masked cells. Masks are declared per screen next to the golden. After review, jackin's own goldens are approved with `tuiscotti accept`.
- In-process complement for PLAN's `TestBackend` requirement: render the screen model into `TestBackend(120,40)`, convert `Buffer` to the sample cell schema (resolve `Color::Reset` to the sample's default fg `#c0c0c0` and bg `#000000`, indexed colors through the sample palette) and diff against the same goldens. This is fast and unit-testable, but it is not the repository's authority; PTY remains the gate. Not verified here: that the Buffer-to-sample conversion is cell-exact with the alacritty-based snapshot (needs a spike).
- By-construction identity for components: pin the `junie-tui` rev. Theme tokens, ladders and widget drawing then come from the same code. Add a test that renders each used widget family from the showcase `States` views and compares to the baseline's `showcase_*` frames where they exist (38 `showcase_page_*` baselines).

## 7. Risks and open questions

| # | Item | Resolver |
| --- | --- | --- |
| 1 | Owner decision: "every screen in the repo" includes TablePro, Holla and the D16-deleted Capsule/1Password/daemon screens. Narrow to showcase components plus surviving jackin screens, or reopen D16. | owner |
| 2 | Choose: depend on `junie-tui` by git rev and port the jackin screens into `jackin-tui` (about 20k LOC), or have upstream extract a screens library first (section 2, item a). | owner, then `rust-cli-tui-engineer` |
| 3 | Update model conflict with P9.1 (`&mut self` render, hit regions in `RenderCtx`, mutate-in-place pages). Decide whether `World` plus `Page::on_msg` plus `Request` satisfies "model, messages, effects, pure render", or wrap it. | owner / architect |
| 4 | Foreground hand-off for `openshell connect` and an external wake input need upstream API (section 2, items 3 and 4). Spike before P9.1. | `rust-cli-tui-engineer`, upstream change in tailrocks/terminal-components-baseline |
| 5 | Baseline has no tags and 3 commits; the `visual-baseline` tag lives elsewhere. A `rev` pin is the only stable handle; baseline goldens may change on any bump. | `rust-cli-tui-engineer` |
| 6 | 110 MB of baselines: vendor only the needed `.tui.json` files, or fetch at the pinned rev in `xtask`. Size and LFS policy for jackin. | `rust-cli-tui-engineer` |
| 7 | Tier B masking rules and the Buffer-to-sample conversion are unproven. The test plan needs review before any test is written. | `test-reviewer` |
| 8 | License file missing upstream; manifest MIT only. | owner (upstream) |
| 9 | `unsafe`/libc in the lib runtime vs jackin lint policy; clap pulled in by the lib. | `security-reviewer` |
| 10 | The cockpit fade frames are not captured. Low value. | none |

Not verified: whether the baseline binaries run on Linux (only macOS arm64 tried here); a full `cargo nextest run` of all 93 tests (only 2 ran); that `Theme::for_level` token values equal the `docs/product.md` hex list beyond `GREEN`; any Tier B tooling (it does not exist).
