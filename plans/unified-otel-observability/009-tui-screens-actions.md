# Plan 009: TUI observability — screen lifecycle events, action roots, widget focus, render health

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/unified-otel-observability/README.md`.
>
> **Drift check (run first)**: `git diff --stat fa8194882..HEAD -- crates/jackin-diagnostics/src/screen.rs crates/jackin/src/console/tui/run.rs crates/jackin-console/src/tui crates/jackin-tui/src/runtime.rs crates/jackin-launch-tui/src/tui`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/unified-otel-observability/004-telemetry-facade-api.md, 007-identity-lifecycle-roots.md
- **Roadmap item**: [Unified OpenTelemetry observability](../../docs/content/docs/roadmap/unified-otel-observability.mdx) (`docs/content/docs/roadmap/unified-otel-observability.mdx`) — this plan implements the "TUI screens, widgets, and actions" case table, the screens-are-state paragraphs of "Bounded traces, logs, and events", and the UI rows of "Metrics"; the roadmap item is the binding contract and overrides this plan on any conflict. Also read `docs/content/docs/reference/tui/index.mdx` before changing any TUI code (repo rule).
- **Planned at**: commit `fa8194882`, 2026-07-15

## Why this matters

Screens are state, not operations. Today every screen visit is a detached ROOT SPAN held for the whole visit (`ScreenGuard`), which is exactly the lifetime-span anti-pattern the roadmap eliminates: it delays export, defeats sampling, and describes history instead of causality. The contract replaces it with: `ui.screen.entered`/`ui.screen.exited` events carrying `ui.screen.visit.id` + `ui.navigation.sequence`, `ui.widget.focused`/`ui.widget.unfocused` events, dwell/focus duration histograms, root `ui.action` traces for semantic actions (keyboard and mouse converging on the same action), child `ui.screen.transition` spans, `ui.render` spans only for bounded action-triggered renders, and standard `app.jank`/`app.crash`. Raw input never becomes telemetry.

## Current state

(verified at planning commit)

- **The mechanism to remove**: `crates/jackin-diagnostics/src/screen.rs` — `Screen` enum (`:38`, values `list/settings/editor/create/launch/capsule` via `as_str` `:55`); `ScreenGuard` (`:92`); `enter_screen` (`:123`) starts a detached root (`span.set_parent(Context::new())` `:132`), stamps `jackin.screen.name`/`jackin.component`/`parallax.run.id`, links the previous screen and sets `jackin.screen.from` (`:143-150`); thread-local `CURRENT` (`:76`); `record_action` (`:234`, `user.action` span event + `jackin.action.target` `:241`); `launch_trace` (`:255`); `record_capsule_activity` (`:286`, `capsule.tab` span); `carry_link_forward` (`:327`); `current_traceparent` (`:341`).
- **Host loop call sites** (all in `crates/jackin/src/console/tui/run.rs`): `sync_active_screen` swaps guards per tick (`:203-210`, declaration `:847`, called `:853`); `screen_of` maps stage→Screen (`:65`); launch actions `:471-473,487-488`; `carry_link_forward` `:632`; the launch wrapper `launch_trace` at `crates/jackin/src/app/load_cmd.rs:127,406,456` (its telemetry role was re-anchored by plan 008 step 1); capsule activity at `crates/jackin-capsule/src/session.rs:421`.
- **Screen registry mapping** (contract `app.screen.id` values): `workspace.list`, `workspace.editor`, `settings`, `workspace.create`, `launch.progress`, `capsule` — map from `ConsoleManagerStage` (`crates/jackin-console/src/tui/model/stage.rs:12`: `List, Editor, Settings, CreatePrelude, ConfirmDelete, ConfirmInstancePurge` — confirm dialogs remain the underlying screen, as `screen_of` already treats them).
- **Widgets** (contract: tab focus is widget focus, never a new screen): `EditorTab` (`crates/jackin-console/src/tui/screens/editor/model.rs:22`: `General, Mounts, Roles, Secrets, Auth` — `Secrets` displays as "Environments", `label()` at `:40-45`; its `app.widget.id` per contract is `secrets_environments`); `SettingsTab` (`settings/model.rs:48`: `General, Mounts, Environments, Auth, Trust`). Tab switches: keyboard `MoveEditorTab`/`MoveSettingsTab`, mouse `SelectEditorTab`/`SelectSettingsTab` (`input/mouse/selection.rs:26,65`) — all converge on the reducer `update_manager` (`crates/jackin-console/src/tui/state/update.rs:93`).
- **Action vocabulary source**: `ConsoleManagerMessage` (`crates/jackin-console/src/tui/message.rs:16`, ~80 variants) and `ConsoleInputOutcome` (`:220`) / `ConsoleInstanceAction` (`:245`); launch-tui `CockpitAction` (`crates/jackin-launch-tui/src/tui/keymap.rs:18`), `BuildLogAction` (`:58`). Keyboard and mouse already converge on the same reducer messages — the semantic-equivalence requirement is structurally satisfied; the plan maps reducer messages → registry actions.
- **Render**: single shared chokepoint `drive_frame` (`crates/jackin-tui/src/runtime.rs:284`; `View` trait `:268`, `Dirty` `:9`). Host console renders via `draw_console_frame` (`run.rs:230/:272`), launch-tui via `RichRenderer::render` (`launch-tui/src/tui/run.rs:370/:414`), capsule via compositor (plan 010). Today only the capsule records render metrics; host/launch-tui record none. `jackin-tui` has NO `jackin-diagnostics` dependency (telemetry-free) — and must stay lean; instrumentation hooks go in as a small callback/port, not a hard dependency on the facade (jackin-tui is T1, jackin-telemetry is T0, so a direct dep is tier-legal — prefer the direct dep on `jackin-telemetry` only, never on `jackin-diagnostics`).
- **Panic hook**: `install_host_panic_hook` (`crates/jackin-diagnostics/src/run.rs:955`), installed at `crates/jackin/src/app.rs:97`.
- **Contract essentials**: `ui.action` is a ROOT trace with `ui.action.name` + current `app.screen.*`/`app.widget.*`; screen transition caused by an action = child `ui.screen.transition` (old/new `app.screen.id`, `ui.transition.reason`) accompanied by exited/entered events; dwell histogram on completed visits; focus duration on unfocus; continuous rendering metric-only; threshold crossings = standard `app.jank` (+`app.jank.frame_count`, `app.jank.period`, `app.jank.threshold`); panics = standard `app.crash` + `app.crash.id` + `session.id` + redacted `exception.*`; equivalent keyboard/mouse input ⇒ same `ui.action.name`; raw keys/coords/text/labels never exported. `ui.screen.visit.id` and visit ids are never metric dimensions.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Console/capsule snapshots | `cargo nextest run -p jackin-capsule -p jackin-console --locked` | all pass |
| Workspace | `cargo nextest run --workspace --all-features --locked` | all pass |
| Conformance | `cargo nextest run -p jackin-diagnostics -p jackin-capsule --all-features --locked -E 'test(/conformance/)'` | all pass |
| Console bench | `cargo bench -p jackin --bench console_frame -- --quick` | completes |
| Lint | `cargo xtask ci --only lint` | exit 0 |

## Scope

**In scope:**
- `crates/jackin-telemetry` — `ui.rs`: visit tracker (visit id + per-session monotonic `ui.navigation.sequence` counter + dwell timing), widget-focus tracker, action registry glue; schema registry additions: full `ui.action.name` enum (below), UI metric instruments, `app.jank`/`app.crash` defs
- `crates/jackin/src/console/tui/run.rs` — replace `sync_active_screen` guard-swapping with lifecycle events; wire action roots at the input-dispatch boundary (`handle_key_event` `:616` region and mouse dispatch `:770` region); `screen_of` maps to `app.screen.id` registry values
- `crates/jackin-console/src/tui/` — reducer-level mapping `ConsoleManagerMessage` → `ui.action.name` (a pure `fn action_of(&ConsoleManagerMessage) -> Option<UiActionName>`; most low-level cursor moves map to `None` — not every message is a semantic action)
- `crates/jackin-launch-tui/src/tui/` — same for cockpit actions; launch-progress screen enters/exits (`launch.progress` screen id)
- `crates/jackin-tui/src/runtime.rs` — render-duration measurement in `drive_frame` (feed a metric hook; `jackin-tui` gains a dep on `jackin-telemetry` only)
- `crates/jackin/src/app.rs` + `crates/jackin-diagnostics/src/run.rs` — panic hook upgraded to emit `app.crash` (+ redacted exception) through the facade
- `crates/jackin-diagnostics/src/screen.rs` — gutted: `Screen` enum and `current_traceparent` survive temporarily if still referenced (launch env injection uses it — plan 008 re-anchored the launch span; `current_traceparent` should now read the active operation span, move it to `jackin-telemetry::propagation`); `ScreenGuard`/`enter_screen`/`record_action`/`launch_trace`/`record_capsule_activity`/`carry_link_forward` DELETED with their call sites
- Capsule tab/pane focus events are plan 010 (same event defs, capsule surfaces)

**Out of scope:**
- Capsule TUI internals (plan 010). Legacy `jackin.screen.name` metric dims and old hot-path metrics (plan 013 removes with the old instruments; new UI metrics land in parallel).
- Any docs page (plan 015). The repo rule "TUI behavior changes update the matching `docs/content/docs/reference/tui/` page same-PR" is satisfied automatically: plan 015's docs sweep lands on the same branch and ships in the same single PR as this plan.

## Git workflow

- Branch: `feature/unified-otel-observability` — single branch, single PR for the whole roadmap item (plans 001–015 together); no per-plan branch or separate PR. Conventional Commits, e.g. `feat(console): screen lifecycle events and ui.action roots replace screen spans`. Sign `-s`, push after every commit.

## The `ui.action.name` registry (seed)

From the contract's action list, mapped to reducer realities: `workspace.open`, `workspace.save`, `workspace.launch`, `settings.open`, `settings.save`, `dialog.confirm`, `dialog.cancel`, `agent.select`, `agent.spawn`, `tab.switch`, `tab.rename`, `tab.close`, `pane.split`, `pane.focus`, `pane.resize`, `pane.zoom`, `pane.clear`, `pane.close`, `usage.refresh`, `session.detach`, `file.export`, `image.stage`, `link.open`, `app.exit_request`, `screen.back`, `workspace.create`, `workspace.delete`, `instance.purge`. Extend ONLY via the schema registry (closed-set discipline); if a reducer message has no matching name and is genuinely semantic, add it to the registry YAML first.

## Steps

### Step 1: Visit/focus trackers + schema

Implement `jackin-telemetry/src/ui.rs`: `ScreenVisitTracker` (current screen id, visit uuid, sequence counter, entered-at instant) producing `ui.screen.entered` / `ui.screen.exited` events (exit carries bounded reason = `ui.transition.reason` value + dwell duration recorded to the dwell histogram) and `WidgetFocusTracker` (same for `app.widget.id`). Add the metric instruments (dwell histogram, focus-duration histogram, transition/action counters, render-duration histogram, jank counter — dimensions per the Metrics table: `app.screen.id`, bounded widget/action/reason, `outcome`, `error.type`; NEVER visit ids).

**Verify**: `cargo nextest run -p jackin-telemetry --locked -E 'test(ui)'` → pass (sequence monotonicity, dwell measurement, id absence from metric dims).

### Step 2: Host console lifecycle swap

In `run.rs`, replace `sync_active_screen`'s guard logic (`:203-210`) with tracker calls: stage-route change ⇒ `exited(reason)` + `entered(new)` events; drop the `Option<(Screen, ScreenGuard)>` state (`:847,:853`). Session binding: trackers read the ambient `session.id` (plan 007). Delete `carry_link_forward` usage (`:632`) — the launch action root (step 3) carries causality now.

**Verify**: `cargo nextest run -p jackin --locked` → pass; in-memory export test: navigating List→Editor→List yields 3 entered + 2 exited events with strictly increasing `ui.navigation.sequence`, correct visit ids, and ZERO screen-named spans.

### Step 3: Action roots

At the dispatch boundary in `run.rs` (`handle_key_event`/mouse handler), after the reducer returns: if `action_of(message)` yields a name, open a `ui.action` ROOT (facade guard; attrs `ui.action.name`, current `app.screen.id`/`app.screen.name`, applicable `app.widget.*`) around the action's synchronous effect; transitions triggered by it become a child `ui.screen.transition` span (old/new screen id + reason) emitted where the stage route changes (thread a lightweight "current action guard" through the effect path — the console loop is single-threaded, a scoped thread-local in the host loop is acceptable and mirrors the old `CURRENT` pattern). Launch action: `workspace.launch` root replaces the `record_action("launch", …)` sites (`:471-473,487-488`); the launch OPERATION (plan 008) becomes a child/continuation of this root via normal parenting (launch begins inside the action's scope) — a long-running launch continues under `.instrument` (the async attach pattern from plan 005). Keyboard and mouse paths hit the same reducer message so equivalence is inherited — write the test proving it.

**Verify**: test: key-driven tab switch and mouse-click tab switch export two `ui.action` roots with identical `ui.action.name=tab.switch`; transition child present with correct old/new + reason=action.

### Step 4: Widget focus (editor/settings tabs)

Reducer arms for `MoveEditorTab`/`SelectEditorTab`/`MoveSettingsTab`/`SelectSettingsTab` (via the host boundary — keep `jackin-console` free of direct emission if its tier makes that awkward; the host loop observes the before/after active tab and drives `WidgetFocusTracker`): `ui.widget.unfocused(old)` + `ui.widget.focused(new)` with `app.widget.id` from the bounded sets {`general`, `mounts`, `roles`, `secrets_environments`, `auth`} / {`general`, `mounts`, `environments`, `auth`, `trust`}; focus-duration histogram on unfocus. The screen stays `workspace.editor`/`settings` — assert no screen events fire on tab switch.

**Verify**: test: tab switch emits focus pair + duration, zero screen entered/exited events.

### Step 5: Render health + crash

- `drive_frame` (`jackin-tui/src/runtime.rs:284`): measure draw duration, feed the render-duration histogram (+ painted-cells if cheaply available from the ratatui buffer — optional) tagged with a caller-supplied surface/screen dim; continuous rendering stays metric-only. Bounded `ui.render` child span ONLY when a `ui.action` guard is active (action-triggered render) — implement as: the host loop requests a render pass while the action guard is open ⇒ wrap that single `terminal.draw` in a `ui.render` child.
- Jank: sliding-window frame-time monitor in the host loop; on threshold crossing emit standard `app.jank` event with `app.jank.frame_count`/`app.jank.period`/`app.jank.threshold` (pick 100 ms/frame threshold over a 1 s window; constants in one place).
- Crash: upgrade the panic hook (`run.rs:955` + `app.rs:97`) to emit `app.crash` + `app.crash.id` (uuid) + `session.id` + `exception.type`/`exception.message` (message through the redactor, 4 KiB cap) then force-flush — reuse the hook's existing flush path.

**Verify**: `cargo bench -p jackin --bench console_frame -- --quick` completes (no obvious regression); panic-hook unit test (spawn thread, panic, assert `app.crash` in in-memory export — model on the existing hook tests in `run.rs` tests if present, else a new one).

### Step 6: Delete the old mechanism

Remove `ScreenGuard`/`enter_screen`/`record_action`/`launch_trace`/`record_capsule_activity`/`carry_link_forward`/`set_workspace*`/`set_agent_selected`/`set_agents_active`/`set_provider` and their call sites (the set_* feature-decision emitters become registry events only if a contract row covers them — `feature.decision` has NO contract row, so they are deleted; the information they carried (workspace name, agent selected) is user data / prohibited anyway). Update `crates/jackin-capsule/src/session.rs:421` (plan 010 replaces it — if executing out of order, leave a compiling stub that no-ops and mark `TODO(plan-010)`). Keep `Screen` enum only if still referenced by `screen_of`; prefer mapping `ConsoleManagerStageRoute` → schema screen ids directly and deleting `Screen` too.

**Verify**: `grep -rn "ScreenGuard\|enter_screen\|launch_trace\|record_capsule_activity\|carry_link_forward" crates/ --include='*.rs' | grep -v tests` → no production matches; `cargo nextest run --workspace --all-features --locked` → pass.

## Reopened audit additions (2026-07-16)

- Move semantic action ownership to the actual keyboard/mouse dispatch boundary so the root remains active through reducer effects, screen transition, and one action-triggered render. Continuous frames remain metric-only.
- Exhaustively map the bounded host and launch-TUI action vocabulary, including launch, save/open, confirm/cancel, exit request, and instance/cockpit/build-log actions, with current bounded screen/widget context.
- Add `launch.progress` lifecycle/dwell and produce `ui.screen.transition` children with old/new screen plus the true bounded reason; initial entry is not a transition.
- Replace legacy panic telemetry with complete standard `app.crash` shape and final flush. Implement the specified one-second sliding-window jank threshold/crossing plus counter through a shared render hook.
- Exporter-backed scenarios prove keyboard/mouse equivalence, lifecycle/focus/dwell/action causality, render/jank/crash shapes, and absence of visit IDs, raw input/coordinates, and dynamic labels in metric dimensions or payloads.

## Test plan

- Tracker unit tests (step 1); lifecycle export test (step 2); keyboard/mouse equivalence test (step 3); widget-focus test (step 4); jank/crash tests (step 5).
- Conformance additions: `conformance_no_screen_spans` (no exported span named `screen`/`capsule.tab`/screen-visit-shaped); privacy negative: no `jackin.workspace`, no tab label, no key/mouse coordinate attr in any exported UI signal.
- Export-volume ratchet regen (counts change substantially): `cargo nextest run -p jackin-diagnostics --all-features -E 'test(conformance_export_volume)'` + `cargo xtask lint ratchet --print export-volume` → update `ratchet.toml`.
- Existing console snapshot tests (`cargo nextest run -p jackin-console`) are the UI-behavior regression net — they must pass untouched (telemetry is invisible to rendering).

## Done criteria

- [ ] `cargo nextest run --workspace --all-features --locked` exits 0
- [ ] Step 6 grep clean — old screen-span mechanism gone from production code
- [ ] Keyboard/mouse equivalence test passes
- [ ] `conformance_no_screen_spans` passes; UI privacy negatives pass
- [ ] `cargo xtask lint --strict` exits 0
- [ ] `plans/unified-otel-observability/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:
- Anything outside telemetry semantically depends on `ScreenGuard`/`current_screen_name` (e.g. debug chip rendering reads it — check `run.rs:244,295,420,702` `is_debug_mode` region) — report the coupling.
- The action-guard scoping around async effects (launch) cannot keep the guard bounded (action root must END when the synchronous dispatch completes or hand off via `.instrument` — if neither works for some effect, report it rather than holding the guard open).
- `jackin-tui` gaining a `jackin-telemetry` dep trips the arch gate (it must not — T1→T0 is legal; if `--strict` disagrees, the tier table drifted).
- Render measurement in `drive_frame` shows >5% regression on `console_frame` bench.

## Maintenance notes

- New screens/tabs/actions require schema-registry additions FIRST — the closed-set tests will fail otherwise; this is intentional friction.
- Plan 010 reuses the exact widget/action event defs for capsule tabs/panes; plan 013 deletes the legacy `jackin.*` UI metric dims.
- Reviewer focus: no span survives a full console tick loop; the only spans opened in the loop are action-bounded.
- Cross-cutting TUI docs rule: behaviorally visible changes here are none (telemetry only), but if any hint/keybinding surface changed accidentally, the matching `docs/content/docs/reference/tui/` page must change in the same PR.
