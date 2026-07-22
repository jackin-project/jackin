# Plan 008: Extend the Rust view-model and FFI so every jackin❯ Desktop v1 string is Rust-owned

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. Swift stays display-only; this plan is Rust + regenerated bindings only — no Swift UI work (Plans 009–011 consume what this plan exports). If anything in "STOP conditions" occurs, stop and report. When done, update this plan's row in `plans/native-macos-usage-menu-bar/README.md`.
>
> **Drift check (run first)**: `git diff --stat be6fb79e..HEAD -- crates/jackin-usage crates/jackin-usage-ffi crates/jackin-protocol/src/control.rs crates/jackin-capsule/src/tui/components/dialog_widgets/usage.rs native/Sources/JackinUsageBridge`
> On a mismatch with the "Current state" excerpts below, STOP and report.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED (touches shared view shaping consumed by Capsule, CLI, and the app)
- **Depends on**: none (parallel with Plan 007; Plans 009–011 depend on this)
- **Category**: direction
- **Planned at**: commit `be6fb79e`, 2026-07-22

## Why this matters

The jackin❯ Desktop v1 spec (roadmap `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`, "product spec" section) is governed by two invariants: **Capsule parity** (Desktop renders the same DTO strings the Capsule usage dialog shows) and **Rust-first** (Swift never computes percentages, pacing, severity, thresholds, or label text — ADR-011). Every new v1 display need is therefore first a Rust view-model extension plus an FFI export. This plan delivers all of them in one pass — one bindings regeneration, one review of the truth layer — so the three Swift plans that follow are pure rendering: status-item modes (pinned / strip / depleted-reset), format flags (used↔left, countdown↔clock), overview rows for the glance popover and Usage-window sidebar, per-surface worst severity, next-refresh-due label, and the estimate-honesty caption. It also fixes a latent parity hazard: the provider display-label remap (Codex→OpenAI …) currently lives inside the Capsule TUI, where the app cannot reach it — the roadmap explicitly forbids a second Swift mapping.

## Current state

- `crates/jackin-usage/src/host.rs` — `HostUsageRuntime`. Key excerpts:
  - `HostSurfaceId` enum + `ALL` order (`host.rs:24-55`); `compact_prefix()` (`host.rs:88-100`): `Cl Cx Am Gr ZA Ki MM OC`; `label()` (`host.rs:72-85`): `Claude, Codex, Amp, Grok Build, GLM / Z.AI, Kimi, MiniMax, OpenCode`.
  - `compact_status_bar_label()` (`host.rs:500-530`) — the worst-surface selection this plan generalizes:

    ```rust
    let Some(remaining) = view.buckets.iter()
        .filter_map(|bucket| bucket.remaining_percent).min() else { continue; };
    match best {
        Some((best_remaining, _)) if remaining >= best_remaining => {}
        _ => best = Some((remaining, surface)),
    }
    // …
    let used = 100u8.saturating_sub(remaining);
    format!("{} {used}%", surface.compact_prefix())
    ```

  - `refresh_due()` (`host.rs:434-439`) from `last_refresh: Option<Instant>` + `refresh_floor_secs` (`host.rs:244-246`, floor clamped ≥60 at `host.rs:276`).
  - `snapshot()` (`host.rs:442-452`) → `UsageCache::focused_snapshot`; `status_bar_label()` (`host.rs:455-465`); `merged_status_bar_label()` (`host.rs:468-491`).
- `crates/jackin-usage/src/usage/format.rs` — label composition: `reset_label` (`format.rs:82-91`) always emits both forms — `"Resets in {countdown} ({local clock})"`; `used_percent_label` (`format.rs:49-56`) → `"{used}% used"` (uncapped over 100); `quota_pace_label` (`format.rs:111-138`) → `On pace` / `N% in reserve` / `N% in deficit`; `local_timestamp_label` (`format.rs:104-109`) → `"%b %-d, %H:%M"`.
- `crates/jackin-usage/src/usage/view.rs` — `provider_tabs(active)` (`view.rs:452-472`) builds `FocusedUsageView.tabs` with `surface.label()` labels; `enrich_provider_tabs` (`view.rs:474+`) fills account/plan/status/source from cached snapshots.
- `crates/jackin-capsule/src/tui/components/dialog_widgets/usage.rs:78-86` — the display remap **stranded in the Capsule**:

  ```rust
  pub(crate) fn usage_provider_display_label(label: &str) -> &str {
      match label {
          "Codex" | "OpenAI / Codex" => "OpenAI",
          "Claude" | "Anthropic / Claude" => "Anthropic",
          "Grok Build" | "xAI / Grok" => "xAI",
          "GLM / Z.AI" => "Z.AI",
          other => other,
      }
  }
  ```

- `crates/jackin-protocol/src/control.rs` — `QuotaBucketView` (`control.rs:591-628`: `remaining_percent: Option<u8>`, `reset_label`, `resets_at: Option<i64>`, `pace_label`, `used_money`/`limit_money`, `severity: UsageSeverity`); `UsageSnapshotStatus` (`control.rs:648-666`: Fresh/Stale/NeedsLogin/NeedsSecret/Unsupported/Unavailable/Error); `UsageSource` + `UsageConfidence` (`control.rs:668-696`); `UsageProviderTab` (`control.rs:630-646`).
- `crates/jackin-usage-ffi/src/bridge.rs` — `#[uniffi::export] impl UsageMenuBarBridge` (`bridge.rs:22`), every entry wrapped in `catch_entry` (panic containment, `error.rs:45-54`). Existing methods: `create/open_runtime/list_surfaces/set_enabled/refresh/set_refresh_floor_secs/refresh_due/snapshot/status_bar_label/merged_status_bar_label/compact_status_bar_label/next_events/refresh_floor_secs/shutdown/panic_probe`.
- `crates/jackin-usage-ffi/src/dto.rs` — `OpenConfig` (`dto.rs:13-20`: `data_dir`, `refresh_floor_secs`, `enabled_surface_ids` — **no format prefs today**); `UsageViewDto` (`dto.rs:59-75` — **no `tabs`, no caption field**); enum-to-string flattening (`dto.rs:133-200`), severity map `dto.rs:173-178` (`"normal" | "warn" | "danger"`).
- Bindings: proc-macro UniFFI (`uniffi::setup_scaffolding!()`, `lib.rs:12`); regenerate via `cargo build -p jackin-usage-ffi --release && ./scripts/generate-usage-swift-bindings.sh`; never hand-edit `native/Sources/JackinUsageBridge/jackin_usage_ffi.swift` or `native/Generated/`.
- Test conventions (hard rules, `crates/AGENTS.md`): tests live in sibling `tests.rs` files (`host/tests.rs`, `usage/tests.rs`, `bridge/tests.rs`) — no inline test modules, no new `mod.rs`. Existing golden pattern: `crates/jackin-usage/src/host/tests.rs` — `codex_fixture_view()` builder (`tests.rs:18`), `compact_status_bar_label_*` cases (`tests.rs:195-229`).
- Target strings this plan must reproduce exactly (from the roadmap spec, S1/S2 sketches and conventions list): `Cl 63% · Cx 41% · ZA 12%` (strip, worst-first, ` · ` separator), `Cl resets 1h 21m` (depleted), overview row `Anthropic  Fable 68% left · Resets in 2d 12h` with exact clock `(Jul 24, 07:00)` available separately, footer `Updated 2m ago · Next update in 4m`, caption `Estimated from token usage · not a subscription bill`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Rust tests | `cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-protocol -p jackin-capsule --locked` | all pass |
| Clippy | `cargo clippy -p jackin-usage -p jackin-usage-ffi -p jackin-capsule --all-targets -- -D warnings` | exit 0 |
| Regenerate bindings | `cargo build -p jackin-usage-ffi --release && ./scripts/generate-usage-swift-bindings.sh` | exit 0; deterministic diff |
| Swift compile check | `cd native && swift build -c release` | exit 0 (bridge compiles against new bindings) |
| Merge readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope:** `crates/jackin-usage/src/{host.rs, host/tests.rs, usage.rs, usage/view.rs, usage/format.rs, usage/tests.rs}`, `crates/jackin-usage/README.md` (module/API table), `crates/jackin-usage-ffi/src/{bridge.rs, dto.rs, bridge/tests.rs}`, `crates/jackin-usage-ffi/README.md`, `crates/jackin-capsule/src/tui/components/dialog_widgets/usage.rs` (delete the local remap, call the lifted one) + its tests, regenerated `native/Sources/JackinUsageBridge/jackin_usage_ffi.swift` + `native/Generated/*`, `native/Sources/JackinUsageBridge/PresentationStore.swift` **only if** the new bindings force a projection stub to keep the build green (prefer not), plan/README status rows.

**Out of scope (do NOT touch):**

- Provider probes, refresh scheduling, cooldown semantics, snapshot persistence — display strings are *added*; nothing existing changes meaning or serialized shape.
- Any Swift view file under `native/Sources/JackinDesktop/` (or `JackinUsageMenuBar/` if Plan 007 has not run) — Plans 009–011 own rendering.
- Provider scope: `HostSurfaceId::ALL` is frozen (ADR-011). No new surfaces.
- Removing or renaming existing DTO fields, existing FFI method signatures, or protocol serde shapes (`jackin-protocol` wire compatibility with the Capsule: a wire-format change requires aligning both binaries — avoid protocol struct changes entirely; everything here composes from existing protocol fields).

## Git workflow

- Active feature branch; from `main` propose `feature/desktop-rust-view-extensions` and wait for confirmation.
- Signed Conventional Commits (`git commit -s`), push after every commit. Suggested subjects: `feat(usage): host display-mode labels + shared provider display remap`, `feat(usage-ffi): export desktop v1 view extensions`.

## Steps

### Step 1: Lift the provider display remap into `jackin-usage`

Move the `usage_provider_display_label` match (excerpt above) from `crates/jackin-capsule/src/tui/components/dialog_widgets/usage.rs:78-86` into `jackin-usage` as a `pub fn provider_display_label(label: &str) -> &str` (natural home: `crates/jackin-usage/src/usage.rs` near `UsageSurface::label()`, or `usage/view.rs` — follow the module ownership the README table implies). The Capsule function becomes a thin `pub(crate)` re-export/call so its call sites (`usage_tab_strip_labels` at `dialog_widgets/usage.rs:63-76` and any others found by `rg -n "usage_provider_display_label" crates/jackin-capsule`) keep compiling. One mapping, two consumers.

**Verify**: `cargo nextest run -p jackin-usage -p jackin-capsule --locked` → all pass; `rg -n '"OpenAI"' crates/jackin-capsule/src` shows no remaining local remap match arms.

### Step 2: Host display-mode labels — pinned, strip, depleted-reset

In `crates/jackin-usage/src/host.rs`:

1. Factor the worst-bucket selection out of `compact_status_bar_label` into a private helper returning, per surface, the driving bucket's `(remaining_percent, resets_at)` — the min-`remaining_percent` bucket exactly as `host.rs:510-517` picks it today.
2. `pub fn compact_status_bar_label_for(&mut self, surface_id: &str) -> Result<Option<String>, String>` — pinned mode: the same `"{prefix} {used}%"` format for one surface; `Ok(None)` when the surface is disabled or has no numeric remaining (never invent numbers).
3. `pub fn compact_status_bar_strip(&mut self, max: u32) -> Result<String, String>` — worst-first (ascending remaining, ties in `ALL` order — reuse the sort the existing method implies), capped at `max` (clamp 1..=8), joined with `" · "` → `Cl 63% · Cx 41% · ZA 12%`. Empty string when nothing numeric.
4. Depleted branch, applied inside all three compact variants (focus/pinned/strip entries): when the driving bucket's `remaining_percent == Some(0)` **and** it has `resets_at`, render `"{prefix} resets {duration}"` using `compact_duration_label` (reuse from `usage/format.rs` — export it `pub(crate)` upward if needed) → `Cl resets 1h 21m`. Without `resets_at`, keep `"{prefix} 100%"` (honest, no invented countdown).

**Verify**: new goldens in `host/tests.rs` (pattern: `compact_status_bar_label_*` at `tests.rs:195-229`) covering: pinned known/disabled/no-data, strip ordering + cap + separator, depleted with and without `resets_at`, all-empty. `cargo nextest run -p jackin-usage --locked` → all pass.

### Step 3: Format preferences resolved in Rust view shaping

Add a prefs type in `jackin-usage` (e.g. `host.rs`): `UsageFormatPrefs { percent_style: PercentStyle::{Left, Used}, reset_style: ResetStyle::{Countdown, ExactClock} }`, default `Left`/`Countdown` (current shipped behavior — defaults must be byte-identical to today's output). Store on `HostUsageRuntime`; setter `set_format_prefs`. Apply at the **single existing formatting layer** (`usage/format.rs` + the label-composition call sites in `usage/view.rs` and the host compact labels):

- `percent_style`: `{n}% left` ↔ `{n}% used` for percent-bearing quota rows and compact labels. Money windows keep their shipped `{used}% used` semantics regardless (roadmap: money is always used-side; do not flip it).
- `reset_style`: `Resets in 6d 22h (Jul 28, 17:02)` (current `reset_label`, `format.rs:82-91`) ↔ `Resets Jul 28, 17:02` (clock-led). Keep both raw ingredients (`resets_at` epoch + `compact_duration_label`) — the style only picks the rendering.

Threading: prefs reach view shaping as a parameter with a `Default` impl so `UsageCache` callers outside the host runtime (Capsule, CLI) compile unchanged and keep today's output. Do **not** add prefs to protocol structs or persisted snapshots — presentation-time only.

**Verify**: goldens in `usage/tests.rs` + `host/tests.rs`: default prefs reproduce today's exact strings (assert against the existing fixtures' expectations unchanged); `Used` flips `97% left`→`3% used` on the same fixture; `ExactClock` drops the countdown. `cargo nextest run -p jackin-usage -p jackin-capsule --locked` → all pass with zero edits to existing expected strings.

### Step 4: Next-refresh-due label + overview rows + estimate caption

Still in `jackin-usage`:

1. `pub fn next_refresh_label(&self) -> String` on `HostUsageRuntime`: from `last_refresh + refresh_floor_secs` → `"Next update in {compact_duration}"`; when due now/never refreshed → `"Next update due"` (pick one honest phrase and golden it). Pure derivation from existing fields (`host.rs:244-246`).
2. `pub struct HostOverviewRow` + `pub fn overview_rows(&mut self) -> Result<Vec<HostOverviewRow>, String>`: one row per **enabled** surface in `ALL` order with fields `{ surface_id: String, display_label: String /* via Step 1 remap over provider_label */, headline: String /* e.g. "97% left" or "Fable 68% left" — bucket label prefixed only when the driving bucket is not the first/default window, per roadmap Overview convention */, reset_label: Option<String> /* countdown form */, exact_reset: Option<String> /* "(Jul 28, 17:02)" clock form */, status_word: String /* storage label: fresh/stale/needs_login/… — used verbatim when no numeric headline exists */, severity: String /* worst bucket severity: normal|warn|danger */ }`. Compose entirely from `focused_snapshot` views + Step 3 prefs; reuse the Step 2 driving-bucket helper.
3. Estimate caption: `pub fn estimate_caption(view: &FocusedUsageView) -> Option<String>` in view shaping — `Some("Estimated from token usage · not a subscription bill")` when `confidence == Estimated || source == LocalLogs`; `None` for authoritative provider data. (Exact trigger set: derive from `UsageSource`/`UsageConfidence` — `control.rs:668-696`; golden every variant.)

**Verify**: goldens for all three in `host/tests.rs` / `usage/tests.rs`; `cargo nextest run -p jackin-usage --locked` → all pass.

### Step 5: FFI exports + regenerated bindings

In `crates/jackin-usage-ffi`:

- `dto.rs`: `UsageFormatPrefsDto { percent_style: String, reset_style: String }` (string enums, matching the crate's flattening convention at `dto.rs:161-178`); `OverviewRowDto` mirroring `HostOverviewRow`; add `estimate_caption: Option<String>` to `UsageViewDto` (additive; populate in `view_dto`).
- `bridge.rs` (each wrapped in `catch_entry`, following the `compact_status_bar_label` shape at `bridge.rs:121-126`): `set_format_prefs(prefs)`, `compact_status_bar_label_for(surface_id) -> Option<String>`, `compact_status_bar_strip(max: u32) -> String`, `overview_rows() -> Vec<OverviewRowDto>`, `next_refresh_label() -> String`.
- Regenerate bindings; commit the deterministic diff of `native/Sources/JackinUsageBridge/jackin_usage_ffi.swift` + `native/Generated/*`. Update both crates' `README.md` public-API tables (hard rule: same PR).

**Verify**: `cargo nextest run -p jackin-usage-ffi --locked` → all pass including a new `bridge/tests.rs` round-trip exercising `overview_rows` + `set_format_prefs` on the fixture view (pattern: `fixture_snapshot_round_trip_via_bridge`, `bridge/tests.rs:35`); run `./scripts/generate-usage-swift-bindings.sh` twice → second run produces no diff; `cd native && swift build -c release` → exit 0.

### Step 6: Full gate

**Verify**: `cargo clippy -p jackin-usage -p jackin-usage-ffi -p jackin-capsule --all-targets -- -D warnings` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

- Goldens (inline `assert_eq!`, no snapshot framework — repo convention) in `host/tests.rs`: pinned/strip/depleted matrix (Step 2), prefs matrix incl. default-equals-today (Step 3), `next_refresh_label`, `overview_rows` (numeric row, named-bucket row `Fable 68% left`, status-word row `unsupported`, severity propagation).
- `usage/tests.rs`: `provider_display_label` cases (all four remaps + passthrough), `estimate_caption` × every `UsageSource`/`UsageConfidence` variant.
- `bridge/tests.rs`: FFI round-trip for every new export; existing tests unmodified.
- The critical regression guard: **no existing expected string changes anywhere** — default prefs are the shipped formats. If an existing test needs its expectation edited, that is a STOP (you changed shipped output).

## Done criteria

- [ ] `cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-protocol -p jackin-capsule --locked` all pass; zero pre-existing expectations edited.
- [ ] Capsule's remap arms deleted; one `provider_display_label` in `jackin-usage` serves both consumers.
- [ ] New FFI surface exactly: `set_format_prefs`, `compact_status_bar_label_for`, `compact_status_bar_strip`, `overview_rows`, `next_refresh_label`, `UsageViewDto.estimate_caption` — nothing else added or removed.
- [ ] Bindings regenerated deterministically; `swift build -c release` green.
- [ ] Both crate READMEs' API tables updated; clippy clean; `cargo xtask ci --fast` exit 0.

## STOP conditions

- Implementing prefs (Step 3) without editing existing expected test strings proves impossible — the formatting layer is not as centralized as `format.rs` suggests. Report the actual composition sites; do not fork a second formatter.
- Any change would alter `jackin-protocol` struct shapes or serde output (wire contract with the Capsule).
- Bindings regeneration is nondeterministic, or the regenerated Swift breaks `PresentationStore` compilation in a way a trivial stub cannot absorb.
- A new label would require Swift-side arithmetic or a second provider mapping to render (design violation — the Rust API is wrong, fix it here instead).

## Maintenance notes

- Plans 009 (status item + Settings), 010 (Usage window), 011 (glance popover) consume this surface; if an executor there requests "one more string", it lands here-shaped (Rust fn + FFI export + golden), never in Swift.
- `overview_rows` is the single source for both the popover strip and the Usage-window sidebar/Overview pane — keep it that way; two row-shaping paths would reintroduce the parity-drift class this program exists to kill.
- Reviewer focus: default-prefs byte-parity with shipped output; the depleted branch never inventing a countdown without `resets_at`; remap lift leaving zero Capsule-local mapping arms.
