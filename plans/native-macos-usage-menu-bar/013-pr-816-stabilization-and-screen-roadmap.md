# Plan 013: Stabilize PR #816 and finish jackin❯ Desktop usage surfaces

## Status

- **Priority**: P0 correctness and merge readiness, then P1 screen coherence, then P2 additive screens
- **Effort**: L
- **Risk**: HIGH — account quota truth, UI responsiveness, release readiness, and a 112-file PR are involved
- **Audited branch**: `docs/native-macos-usage-menu-bar`
- **Audited HEAD**: `5f96ab6594bb7bb914ef5f51f756e5754bc6730c`
- **Merge base**: `6def05c1a743b4f2076ecc17875881e4c127d12d`
- **PR**: [#816](https://github.com/jackin-project/jackin/pull/816), 112 files, 20,201 additions, 194 deletions, 91 commits, no submitted review at audit time
- **Audit date**: 2026-07-22

## Executive assessment

PR #816 contains a credible native macOS v1: one menu-bar status item, a glance popover, a resizable Usage window, Settings, multi-account selection, eight frozen usage surfaces, Rust-backed quota views, arm64 app assembly, and a fail-closed signing/notarization path. The app can preview current account quota limits for Claude, Codex, Amp, Grok Build, GLM / Z.AI, Kimi, MiniMax, and OpenCode. It does not preview individual running agent processes, per-session token consumption, or arbitrary provider analytics; the product model is **usage surfaces and provider/account limits**, not agent-session observability.

The PR is not merge-ready. Two correctness defects violate the architecture promised by the same PR: provider refresh calls execute synchronously on the main actor, and current/live account snapshots can bypass a newer shared host/container snapshot. Swift also composes usage percentages and money strings despite the display-only contract. The current UI and documentation disagree over whether the popover is a compact glance or a second full dashboard. CI has independent native, formatting, lint, docs-link, and spelling failures. Fix these before adding screens.

No live credentialed side-by-side visual acceptance was completed by the existing acceptance record or this source audit. `ACCEPTANCE-012.md` explicitly records structural parity only. Treat S1–S6 as implemented in code, not production-proven on a notarized app with every provider/state.

## Definition of “agent usage complete”

For this program, **agent usage** means the current subscription/quota limits attached to a supported jackin❯ usage surface and account. Completion means an operator can answer all of these questions without opening a Capsule:

1. Which supported usage surfaces are enabled and available?
2. Which provider and account supplies each surface’s quota?
3. How much quota remains or has been used in every provider-supplied limit window?
4. When does each window reset, and is current use on pace?
5. Is the displayed value fresh, stale-last-good, refreshing, unavailable, unsupported, or blocked by authentication?
6. Which known account is selected, and what are the limits for every other known account?
7. When will jackin❯ Desktop refresh next, and can the operator safely force a refresh?
8. Do jackin❯ Desktop, Capsule, and host CLI show the same account snapshot and display strings?

Completion does **not** mean showing what an individual running agent process consumed. Per-process context, task, token, or session telemetry is a different product domain. If desired later, create an **Agent Activity** roadmap item backed by runtime/session identifiers and lifecycle data; do not infer it from provider quota deltas.

## Target architecture

```text
Provider APIs / local credentials
             │
             ▼
   jackin-usage probes + cooldown
             │
             ▼
 Account-keyed authoritative snapshots ◀──── Capsule writers
             │
             ▼
 Rust presentation projection
  • surface/provider/account identity
  • state and severity
  • exact display strings
  • normalized geometry values
             │
             ▼
 jackin-usage-ffi immutable DTO snapshot
             │
             ▼
 Serialized background bridge executor
             │
             ▼
 Main-actor SwiftUI projection only
  • status item
  • compact glance
  • Usage: Overview / Accounts / Status / Detail
  • Settings + preview
```

There is one source of provider truth, one account-keyed cache resolution algorithm, one Rust display projection, and one serialized bridge operation stream. Swift owns macOS lifecycle, layout, navigation, accessibility wiring, screen-share detection, and Login Items only.

## Complete implementation program

| Work package | Outcome | Depends on | Delivery boundary |
|---|---|---|---|
| W0 Contract lock | Freeze terminology, catalog, allowed quota fields, and forbidden telemetry | — | First commit in PR #816 stabilization |
| W1 Background bridge | No blocking FFI/probe call on main actor; ordered refresh/account/settings operations | W0 | PR #816 blocker |
| W2 Authoritative account truth | Newest valid same-account snapshot wins across Desktop, host, durable store, and Capsule | W0 | PR #816 blocker |
| W3 Rust presentation API | Every displayed quota string/state is Rust-owned; Swift receives immutable DTOs | W1, W2 contract can proceed in parallel | PR #816 blocker |
| W4 Store lifecycle | Explicit loading/refresh/coalescing/stale-response behavior and atomic main-actor projection | W1, W3 | PR #816 blocker |
| W5 Status item | All four modes remain truthful for every state and account | W3, W4 | PR #816 v1 |
| W6 Compact glance | Overview rows only; details open Usage | W3, W4 | PR #816 v1 |
| W7 Usage Overview + Detail | Primary quota dashboard and complete provider/account detail | W2–W4 | PR #816 v1 |
| W8 State experience | Consistent loading, auth, unsupported, stale, and failure presentation | W3, W4 | PR #816 v1 |
| W9 Settings | Persistent refresh/display policy, truthful terminology, Rust-backed preview | W3, W4 | PR #816 v1 |
| W10 All Accounts | Cross-surface account quota view | W2, W3, W7 | Follow-up PR |
| W11 Status & Sources | Cross-surface health/auth/freshness view | W3, W8 | Follow-up PR |
| W12 Acceptance + docs | Automated matrix, real macOS visual/accessibility proof, docs parity | W1–W11 as applicable | Each owning PR; final sweep before release |
| W13 Signed release | Notarized/stapled immutable ZIP and cask production proof | W1–W9, W12, Apple credentials | Existing plans 003–004 |

### W0 — Lock the product and vocabulary contract

1. Keep the canonical catalog in `HostSurfaceId::ALL`: Claude, Codex, Amp, Grok Build, GLM / Z.AI, Kimi, MiniMax, OpenCode.
2. Define and document three distinct concepts: **usage surface** (catalog/navigation identity), **provider** (quota issuer/display identity), and **account** (provider identity whose snapshot is shown).
3. Rename Swift-local `agent` collections and accessibility labels to `surface` unless the value truly identifies an agent runtime.
4. Use “provider” for account quota cards and “usage surface” in Settings/catalog controls. Never expose internal routing slugs.
5. Freeze allowed fields to quota windows, remaining/used percentage, reset, pace, state, plan, account identity, auth-origin summary, freshness, and provider-supplied hard money caps.
6. Keep all forbidden price/history/commercial surfaces listed under Explicit non-goals.

### W1 — Implement a serialized non-main bridge runtime

1. Define a bridge command enum or equivalent operation boundary: open, refresh-all, refresh-one, set-enabled, set-account, set-format, set-floor, poll, and shutdown.
2. Execute commands on one serial background executor. The UniFFI runtime must never be called concurrently unless Rust explicitly guarantees that operation.
3. Model lifecycle explicitly: `closed → opening → ready ↔ refreshing → shuttingDown → closed`, with failure state retaining the last immutable projection when available.
4. Assign an operation generation. Ignore an obsolete projection when a later account/format/config mutation has already committed.
5. Coalesce periodic polls. During an active refresh, retain at most one pending forced refresh and one pending projection request.
6. Keep the UI responsive during startup and manual refresh. First launch shows cached data or an explicit loading state while network work proceeds in background.
7. Make shutdown ordered and idempotent; do not destroy the bridge while work is crossing FFI.

### W2 — Implement one account snapshot resolution algorithm

1. Gather candidates from process-local live cache, durable host store, and shared Capsule snapshots for the requested surface.
2. Derive the opaque account key through one canonical function.
3. Filter by exact resolved usage surface/provider before comparing accounts.
4. For the selected key—or live key when no explicit selection exists—choose the newest valid candidate by `fetched_at_epoch`.
5. Define deterministic tie-breaking for equal timestamps: prefer authoritative provider source, then local live source, then durable/shared order only if source quality also ties.
6. Preserve stale status based on age/source rules; “newest” must not incorrectly relabel stale data as fresh.
7. Fall back to live only when the selected key is absent from every valid candidate; expose that fallback through state so UI does not silently imply the requested account was found.
8. Persist selected account atomically and validate that it belongs to the selected surface.

### W3 — Define the complete Rust-to-Swift presentation contract

The final FFI projection should contain enough information that Swift never derives quota meaning. Exact type names may follow repository conventions, but the contract must cover:

**Surface presentation**

- Stable surface id, surface label, provider display label, enabled flag, and stable order.
- Selected account key/label, username when allowed, plan label, and auth-origin display string.
- Presentation state kind, severity, title, detail, last-good flag, updated label, exact fetched timestamp, and permitted action intent.
- Rust-owned status-item focus label, chip lines, overview headline, reset/freshness subtitle, and accessibility label.
- Ordered bucket presentations and account summaries.

**Bucket presentation**

- Stable bucket id independent of display label; duplicate labels must not collide in SwiftUI identity.
- Display label, primary value, reset text, exact reset text, pace left/right strings, secondary limit string, status, severity, and accessibility label.
- A clamped normalized remaining fraction for bar geometry; Swift must not compute used percentage or infer depleted state.
- Provider-supplied hard-cap money text already formatted by Rust. No raw minor-unit formatting in Swift.

**Account summary**

- Surface id, opaque account key, display label, selected flag, plan, state/severity, freshness, Rust-owned driving quota label, normalized fraction when available, and accessibility label.
- No credential values, raw tokens, source file contents, or account-key material beyond the existing opaque hash.

**Global presentation**

- Next-refresh label, refresh-in-progress state, global error/state, status-item mode projections, and settings-preview projections.

Regenerate UniFFI bindings only through the repository’s canonical command and test generated/handwritten boundary consistency.

### W4 — Rebuild `PresentationStore` as atomic projection state

1. Keep one published immutable app projection rather than independently mutating bar labels, surfaces, rows, and accounts across many calls.
2. Apply a completed projection once on the main actor so status item, popover, Usage, and Settings cannot briefly disagree.
3. Expose operation state for disabling duplicate refresh actions and announcing progress accessibly.
4. Preserve the last-good projection on bridge/provider failure and overlay the new state/error metadata.
5. Keep selections stable when a refresh reorders or temporarily omits data; fall back visibly when the selected surface/account disappears.
6. Persist only UI preferences in `UserDefaults`; provider/cache policy remains Rust-owned. Remove the pre-release migration shim if no released build consumed it, consistent with pre-1.0 latest-only policy.

### W5–W9 — Finish every v1 surface

**Status item (W5)**

- Retain all-provider strip, worst surface, pinned surface, and icon-only modes.
- Use only Rust-provided chip/value/accessibility strings.
- Show reset instead of bare zero when supplied, preserve stale values with degraded styling, and collapse values during screen sharing.
- Bound width for eight surfaces and ensure missing numeric data renders `—` rather than disappearing ambiguously.

**Compact glance (W6)**

- Render one concise row per enabled surface in canonical order.
- Include provider identity, selected account only when useful, driving quota/status, reset/freshness, and severity.
- Open Usage on row activation; retain pinned footer actions and next-refresh text.
- Remove duplicated metric cards, account pills, tile selector, money formatting, pace parsing, and full-detail scrolling.

**Usage Overview and Detail (W7)**

- Overview shows every enabled surface, selected account, all important quota windows up to a documented density cap, status, and freshness.
- Detail shows provider/account identity, account switcher, every bucket, provider hard-cap rows, pace, reset, last-good/error state, and estimate honesty caption supplied by Rust.
- Use stable bucket ids and preserve all non-numeric value-only rows.
- Keep navigation/sidebar chrome separate from standard-material content.

**State experience (W8)**

- One component renders disabled, initial loading, refreshing-without-data, refreshing-with-last-good, fresh, stale-last-good, needs-login, needs-secret, unsupported, provider-error-with-data, provider-error-empty, and no-enabled-surfaces.
- Action intents are limited to Refresh, Open Settings, or none. jackin❯ Desktop does not authenticate or edit credentials.
- Raw internal errors may appear only in a contributor/debug disclosure, not as the sole operator message.

**Settings (W9)**

- Persist display mode, strip cap, pinned surface, percent/reset style, privacy, launch-at-login, enabled surfaces, and refresh floor through their authoritative owners.
- Add a Rust-backed non-live preview for each status-item mode.
- Disable invalid pinned choices and explain disabled surfaces.
- Show refresh operation/floor semantics honestly; relaunch must preserve the chosen floor.
- Keep About concise: limits source, shared-truth statement, privacy, version/build from a Rust/build-info projection.

### W10–W11 — Add the two missing Usage views

**All Accounts (W10):** grouped surface/provider rows for every known account, selected state, plan, freshness, status, and driving quota. Row activation explicitly changes the selected account and opens Detail. Provide a clear way to return to the live/default account.

**Status & Sources (W11):** one row per surface showing state, last successful update, selected account, safe auth-origin summary, whether last-good data is displayed, and available Refresh/Settings action. This is diagnostics for quota availability, not a credential inspector.

### W12 — Prove parity, responsiveness, accessibility, and documentation

1. Add deterministic Rust fixtures for the full catalog, every state, duplicate bucket labels, long localized-looking strings, dual/many buckets, hard money caps, and multiple accounts.
2. Add bridge concurrency tests with controllable blocking calls and operation-order assertions.
3. Add static architecture checks that detect Swift percentage arithmetic, unit/currency formatting, semantic reset/pace parsing, provider catalogs, probe imports, and unauthorized macOS availability gates.
4. Add pure Swift rendering/harness tests for all status modes and navigation projections without credentials.
5. Run real Apple Silicon visual/accessibility acceptance from P1.5 and retain redacted evidence.
6. Compare Desktop, Capsule, and `jackin usage` against identical serialized snapshots; display parity mismatches block release.
7. Update the public guide, roadmap, ADR if architecture changed, crate/native READMEs, and acceptance record in the same PR as behavior.
8. Keep current CI hygiene green: fmt, Clippy, strict xtask lint, spelling, links, architecture, desktop build/test/verify, and `cargo xtask ci --fast`.

### W13 — Release and production proof

1. Preserve the publish-only credential boundary and required Apple environment.
2. Build the app once, sign with the expected Developer ID, verify certificate/team, require accepted notarization, staple, run Gatekeeper, create the final ZIP after stapling, then extract and release-verify that ZIP.
3. Generate checksum, signing bundle, SBOM, and provenance only from final ZIP bytes.
4. Publish only from `main`; never upload the validation-mode ad-hoc ZIP as a release artifact.
5. Install the downloaded release ZIP and Homebrew cask on Apple Silicon; exercise status item, glance, Usage, Settings, refresh, relaunch persistence, and at least one live provider plus one stale/error state.
6. Only then close plans 003–004 and describe jackin❯ Desktop as available.

## Canonical UI interface and screen specification

This section is the source of truth for every jackin❯ Desktop UI interface. It supersedes the older S1–S6 shorthand wherever that shorthand conflicts. Implementation and public documentation must use these IDs during acceptance.

### Interface map and navigation

| ID | Interface | Entry | Destination/actions |
|---|---|---|---|
| M1 | Menu-bar status item | App launch | Opens G1; values collapse for screen sharing |
| G1 | Glance popover | Click M1 | Opens U1 or U4, opens Settings, refreshes, quits |
| U1 | Usage — Overview | G1 “Open Usage…” or Usage window default | Opens U4 for selected surface |
| U2 | Usage — Accounts | Usage sidebar | Selects account and opens U4 |
| U3 | Usage — Status & Sources | Usage sidebar | Opens U4, refreshes, or opens Settings |
| U4 | Usage — Provider Detail | G1 row, U1 card, U2 account, U3 row, or Usage sidebar surface | Switches account; returns through sidebar |
| S1 | Settings — General | G1 Settings or ⌘, | Launch/privacy behavior |
| S2 | Settings — Menu Bar | Settings navigation | Status-item mode and preview |
| S3 | Settings — Usage Surfaces | Settings navigation | Enable surfaces and set refresh policy |
| S4 | Settings — About | Settings navigation | Version, privacy, architecture summary |
| E1 | First Run / No Enabled Surfaces | Empty G1 or U1 | Opens S3 |
| E2 | Global Loading / Failure Banner | G1 and Usage chrome when applicable | Announces state; refreshes when allowed |

```text
M1 Status Item
      │
      ▼
G1 Compact Glance ────────▶ S1–S4 Settings
      │
      ├───────────────▶ U1 Overview ───────▶ U4 Provider Detail
      │                      │                       ▲
      │                      ├────▶ U2 Accounts ─────┤
      │                      └────▶ U3 Status ───────┘
      │
      └─ empty ─────────▶ E1 First Run ───────────▶ S3 Usage Surfaces
```

### Shared visual and interaction rules

1. **One selected account per usage surface.** M1, G1, U1, U3, and U4 always project the same selected account. U2 is the only aggregate account selector.
2. **Stable navigation.** Usage sidebar order is Overview, Accounts, Status, then enabled surfaces in Rust catalog order. Selection survives refresh and window reopen when the destination still exists.
3. **Rust-owned content.** Provider/account labels, quota values, percentages, resets, pace, money-cap strings, state copy, freshness copy, and accessibility labels arrive ready to display. Swift owns layout and native controls only.
4. **State never destroys last-good data.** Refreshing, stale, and provider error remain visible as metadata while known buckets remain rendered.
5. **No surprise writes.** The only data-changing UI actions are selecting an account, enabling/disabling a usage surface, changing presentation/refresh preferences, launch-at-login registration, and forcing refresh. No provider account, credential, reset, or commercial mutation exists.
6. **Liquid Glass boundary.** Glass is limited to menu/status controls, popover chrome, sidebars, toolbars, and pinned action bars through `GlassFallbacks`. Usage cards and Settings content use standard system materials.
7. **Keyboard baseline.** ⌘R refreshes from G1/U1–U4; ⌘, opens Settings; ⌘Q quits; Esc closes G1 or the front Usage window; Return/Space activates focused rows; standard arrows navigate lists/sidebars; Tab order follows visible hierarchy.
8. **Accessibility baseline.** Every severity color has text/state-equivalent meaning. Progress bars expose label/value/reset, selected account and sidebar rows expose selected traits, refreshing changes are announced once, and screen-share collapse announces “Usage values hidden while screen sharing.”
9. **Sizing baseline.** G1 remains compact with pinned footer; Usage has a 760×500 minimum and supports resizing; Settings fits at the minimum supported display scale without clipping. Long provider/account/plan strings truncate visually but remain complete in accessibility/help text.
10. **Privacy baseline.** Never display credential values, raw API responses, opaque account-key hashes, home-directory paths, or usernames in menu-bar text. G1 and Usage may show the safe account display label supplied by Rust.

### M1 — Menu-bar status item

**Purpose:** Passive, continuously visible warning and remaining-quota preview.

**Variants:**

```text
[j❯] [Cl 37%] [Cx 59%] [ZA 88%]   all-surface strip
[j❯] Cl 37%                         worst surface
[j❯] Cx 59%                         pinned surface
[j❯]                                icon only / screen-share privacy
[j❯] Cl resets 1h 21m               depleted driving bucket
[j❯] [Cl —] [Cx 59%]                enabled surface without numeric quota
```

**Content:** jackin❯ template mark; Rust-provided provider glyph/prefix; up to the configured surface cap; up to two quota lines per chip; depleted reset; degraded styling; accessible combined label.

**Actions:** Primary click opens G1. No context menu or secondary write actions in v1.

**States:** no enabled surfaces = icon only with accessibility hint to configure; loading without cache = icon plus quiet progress affordance; stale/error with last-good = retain values and dim/mark degraded; screen sharing = icon only regardless of mode.

**Acceptance:** Fits eight capped chips without rendering outside the menu bar’s allocated item; all modes update atomically with account/format changes; no number is composed in Swift; VoiceOver reads surface, quota, reset, and freshness without duplicated symbols.

### G1 — Compact glance popover

**Purpose:** Answer “which quota needs attention?” in one click, then route detail work to Usage.

```text
┌ jackin❯ Desktop ───────────────────────────────┐
│ Anthropic · personal       37% left   1h 21m  │
│ OpenAI · work              59% left   4d 02h  │
│ Amp                         fresh             │
│ xAI                         needs login       │
│ Z.AI                        88% left   stale   │
│ Kimi                        unsupported       │
├───────────────────────────────────────────────┤
│ Updated 2m ago · Next update in 3m            │
│ Open Usage…                              ↵     │
│ Refresh                                 ⌘R     │
│ Settings…                               ⌘,     │
│ Quit                                    ⌘Q     │
└───────────────────────────────────────────────┘
```

**Content:** One row per enabled surface in canonical order; provider display label; selected account only when useful for disambiguation; Rust-owned driving quota/status; reset or freshness; severity/state symbol; E2 banner only for global bridge failure.

**Actions:** Row opens U4 for that surface. Open Usage opens U1 unless the operator has an explicit selected G1 row. Refresh forces one coalesced refresh. Settings opens S1. Quit performs ordered shutdown.

**States:** E1 replaces rows when none are enabled. During refresh, rows retain last-good values and show one progress state. Disabled surfaces do not appear. Unsupported/needs-login remain visible because disappearance would look like success.

**Acceptance:** No metric cards, provider tile grid, account pills, charts, or full-detail blocks; six ordinary rows fit without scrolling; at eight rows only the row region may scroll while header/footer stay fixed; full keyboard and VoiceOver operation.

### U1 — Usage Overview

**Purpose:** Primary dashboard for every selected provider account and quota window.

```text
┌ Sidebar ─────────┬ Usage / Overview ─────────────────────────────┐
│ Overview         │ Anthropic · personal · Max                    │
│ Accounts         │ Session      ███████░ 37% left   resets 1h   │
│ Status           │ Weekly       █████████░ 81% left  resets 4d  │
│ ───────────────  │                                                │
│ Anthropic   !    │ OpenAI · work · Pro                           │
│ OpenAI           │ Session      █████░ 59% left     resets 4d   │
│ Amp              │ Weekly       ████████░ 74% left  resets 6d   │
│ xAI        login │                                                │
│ Z.AI       stale │ Amp · default                                  │
└──────────────────┴────────────────────────────────────────────────┘
```

**Content:** One card per enabled surface; provider and selected account; plan; state/freshness; up to the overview density cap of important numeric buckets; Rust-owned values/resets/pace; value-only fallback when numeric quota is absent.

**Actions:** Card opens U4. Toolbar Refresh forces one coalesced refresh. Settings opens S1. Sidebar routes U1–U4.

**States:** Cards use the W8 state component. Refresh preserves cards. A surface with more buckets than the density cap indicates additional limits in Detail without inventing a count unless Rust supplies one. E1 appears when none are enabled.

**Acceptance:** Every enabled surface appears exactly once; selected account matches M1/G1/U4; most constrained bucket is visible; cards remain understandable without color; overview never ranks spend or adds history.

### U2 — Usage Accounts

**Purpose:** Compare and select all known accounts across usage surfaces.

```text
┌ Sidebar ─────────┬ Usage / Accounts ─────────────────────────────┐
│ Overview         │ Anthropic                                     │
│ Accounts     ●   │ ● personal@example.com   Max   37% left  2m │
│ Status           │ ○ work@example.com       Team  72% left  8m │
│ ───────────────  │                                                │
│ Anthropic        │ OpenAI                                        │
│ OpenAI           │ ● work@example.com       Pro   59% left  1m │
└──────────────────┴────────────────────────────────────────────────┘
```

**Content:** Groups in canonical surface order; provider label; every valid live/durable/shared account once; selected trait; account display label; plan; Rust-owned driving quota/state; freshness; clear “Live/default account” identity where available.

**Actions:** Selecting a row persists that surface’s selected account and opens U4 after the new atomic projection commits. “Use live/default account” restores the live selection through a Rust API, not an empty-string convention hidden in Swift.

**States:** An account unavailable in the latest scan remains only if Rust deliberately exposes a stale durable snapshot and labels it stale. A requested account that falls back to live is shown as fallback/error, never selected silently.

**Acceptance:** No cross-provider key collision; deterministic freshness winner; duplicate source snapshots deduplicate; selected account propagates simultaneously to M1/G1/U1/U3/U4; keyboard and VoiceOver expose group and selection.

### U3 — Usage Status & Sources

**Purpose:** Explain missing or questionable quota data across all surfaces without inspecting credentials.

```text
┌ Sidebar ─────────┬ Usage / Status & Sources ─────────────────────┐
│ Overview         │ Anthropic   Fresh       OAuth       2m ago   │
│ Accounts         │ OpenAI      Refreshing  OAuth       8m ago   │
│ Status       ●   │ Amp         Stale       API key     43m ago  │
│ ───────────────  │ xAI         Needs login —          never     │
│ Anthropic        │ Z.AI        Error       API token  last-good │
└──────────────────┴────────────────────────────────────────────────┘
```

**Content:** One row per enabled surface; provider; selected account when safe/useful; W8 presentation state; last successful update; safe auth-origin summary; whether last-good values remain displayed; Rust-owned operator detail.

**Actions:** Row opens U4. Refresh retries the selected/all applicable surface. Open Settings routes S3. No credential reveal/copy/edit action.

**States:** All W8 states appear as explicit text and symbol. Internal errors are behind a debug disclosure only if a contributor-facing mode already exists; operator copy remains bounded and safe.

**Acceptance:** Useful when every surface is empty; never displays paths, secret names with values, raw errors as sole explanation, or provider payloads; state agrees with U1/U4 for the same projection generation.

### U4 — Usage Provider Detail

**Purpose:** Complete quota and account detail for one selected usage surface.

```text
┌ Sidebar ─────────┬ Anthropic · personal@example.com       Max ──┐
│ Overview         │ Updated 2m ago · OAuth · Fresh               │
│ Accounts         │ Accounts: [personal 37%] [work 72%]          │
│ Status           │                                                │
│ ───────────────  │ Session                                       │
│ Anthropic    ●   │ ███████░             37% left · resets 1h    │
│ OpenAI           │ 12% in reserve · lasts until reset             │
│ Amp              │                                                │
│                  │ Weekly                                        │
│                  │ █████████░           81% left · resets 4d     │
│                  │                                                │
│                  │ Last-good data · provider refresh failed       │
└──────────────────┴────────────────────────────────────────────────┘
```

**Content:** Provider/surface identity; selected account and optional username; plan; auth-origin display; state/freshness; horizontal account selector only when multiple accounts exist; every ordered bucket; numeric bar or value-only row; reset; exact reset; pace; hard money-cap text; estimate/source honesty; last-good/error state.

**Actions:** Account selector changes selection through W2. Toolbar Refresh refreshes this surface or all according to the final Rust API contract; choose one behavior and label it explicitly. Settings opens S3. No provider write actions.

**States:** Initial loading uses W8 empty-loading state; refresh retains last-good; unsupported/no-data renders state component rather than fake empty gauges; duplicate bucket labels remain distinct through stable ids; depleted buckets prioritize reset display.

**Acceptance:** Field parity with Capsule for identical snapshot; every Rust bucket renders once; account switch cannot be overwritten by an older refresh; long identity and many buckets scroll only content; no Swift arithmetic/formatting/parsing.

### S1 — Settings General

**Purpose:** macOS lifecycle and privacy behavior.

**Controls:** Launch at login with current `SMAppService` state and approval guidance; Hide values while screen sharing; optional “Open at login” explanation; no quota formatting controls.

**Actions/states:** Registration failure restores the real system value and shows bounded native error text. Screen-share setting updates M1 immediately. System Settings deep link is allowed only where macOS provides a stable API.

**Acceptance:** System truth is reread on every appearance; no cached toggle lies after external changes; controls have clear VoiceOver labels/help.

### S2 — Settings Menu Bar

**Purpose:** Configure and understand M1 before closing Settings.

```text
Display:  ● All surfaces  ○ Worst  ○ Pinned  ○ Icon only
Preview:  [j❯] [Cl 37%] [Cx 59%]
Maximum surfaces: 8
Pinned surface: OpenAI
Percent: ● Left  ○ Used
Reset:   ● Countdown  ○ Exact time
```

**Controls:** Four display modes; Rust-backed deterministic preview; strip cap shown only for all-surface mode; pinned surface shown only for pinned mode and limited to enabled surfaces; percent and reset style.

**Actions/states:** Changes update preview immediately and live M1 atomically. Preview never triggers provider refresh and uses explicit Rust fixture/presentation data, not fabricated Swift strings.

**Acceptance:** Every control affects all relevant surfaces consistently; hidden conditional controls are skipped in keyboard order; pinned mode cannot save an invalid/disabled surface without visible fallback.

### S3 — Settings Usage Surfaces

**Purpose:** Control quota scope and refresh policy.

**Controls:** One toggle per canonical Rust surface; provider subtitle where it clarifies routing; minimum refresh interval with exact persisted value; next-refresh/cooldown explanation; manual Refresh action with progress state.

**Actions/states:** Enabling a surface schedules/coalesces background refresh and does not block UI. Disabling removes it from M1/G1/U1–U4 after one atomic projection and handles pinned/selected fallback visibly. Refresh-floor changes persist in the Rust-owned policy store and survive relaunch.

**Acceptance:** No Swift provider list; all eight current surfaces appear from Rust; future supported surface descriptors appear without per-provider Swift branches; no control permits a floor below Rust’s minimum.

### S4 — Settings About

**Purpose:** Identify build and explain trust/privacy boundaries without implementation noise.

**Content:** jackin❯ Desktop name and logomark; marketing version and build; “Account quota limits from jackin❯ usage”; local-credentials/no-password-storage statement; shared Desktop/Capsule truth statement; links to operator guide, privacy/security documentation, licenses, and release verification where stable.

**Actions:** Open documentation links only. No update checker unless separately designed; cask-only update policy remains documented.

**Acceptance:** Version/build comes from canonical build-info projection; links pass docs checks; no secret paths or credential details.

### E1 — First Run / No Enabled Surfaces

**Purpose:** Recover from an intentionally or accidentally empty catalog.

**Content:** “No usage surfaces enabled”; one sentence explaining account quota preview; primary Open Usage Surfaces action; secondary Quit only in G1 footer. Do not ask for credentials because jackin❯ Desktop reuses existing host authentication.

**Placement:** Replaces G1 row area and U1 content area. M1 remains the logomark. U2/U3 show the same empty-state component with context-appropriate title.

**Acceptance:** One keyboard/VoiceOver action reaches S3; no dead-end screen; no fake provider examples presented as live values.

### E2 — Global Loading / Failure Banner

**Purpose:** Represent bridge-wide state that cannot be attributed to one provider.

**Content:** Rust-owned bounded title/detail, last successful global projection time when available, and Refresh action intent when retry is valid.

**Placement:** Compact banner above G1 rows and below Usage toolbar. Provider-specific failures remain on their rows/cards rather than becoming global banners.

**Acceptance:** Exactly one announcement per state transition; never obscures last-good content; no raw panic/debug payload; disappears atomically when a healthy projection commits.

### Screen acceptance ledger

| Screen | Data fixture | Keyboard | VoiceOver | Empty | Loading | Stale/error | Long text | macOS 14/26 |
|---|---|---|---|---|---|---|---|---|
| M1 | Full catalog + dual/depleted | Click target | Combined label | Icon | Progress | Retain/dim | Width cap | Required |
| G1 | Six/eight mixed states | Full | Rows/footer | E1 | Last-good | Explicit rows | Truncate/help | Required |
| U1 | All surfaces + many buckets | Full | Cards/sidebar | E1 | Last-good | W8 cards | Required | Required |
| U2 | Live/durable/shared accounts | Full | Groups/selection | Context empty | Last-good | Stale account | Required | Required |
| U3 | Every W8 state | Full | State/action | All-empty useful | Explicit | Explicit | Required | Required |
| U4 | Multi-account + every row shape | Full | Identity/buckets | W8 | Last-good | W8 + data | Required | Required |
| S1 | Login/privacy states | Full | Controls/errors | N/A | N/A | System failure | Required | Required |
| S2 | Four mode previews | Full | Controls/preview | N/A | N/A | Invalid pinned | Required | Required |
| S3 | Full catalog + floor bounds | Full | Toggles/progress | All disabled | Refresh | Per-surface | Required | Required |
| S4 | Build-info fixture | Full | Content/links | N/A | N/A | Missing link blocked | Required | Required |
| E1 | No enabled surfaces | Full | Action/context | Required | N/A | N/A | Required | Required |
| E2 | Global startup/failure | Refresh | Announcement | No cache | Required | Required | Required | Required |

Every ledger cell marked Required must have either a deterministic automated harness assertion or named redacted manual evidence in the PR acceptance record. “Looks correct from source” is not acceptance.

## Product boundary: what “agents + providers usage” means

`HostSurfaceId::ALL` is the canonical eight-item UI catalog. Six items correspond directly to supported agent runtimes; GLM / Z.AI and MiniMax are routed-provider usage surfaces. A tile therefore represents a quota surface, not a live process, task, workspace, or model invocation. The UI may show the provider/account identity and quota windows Rust supplies, but it must not imply that it measures consumption by an individual running agent.

Do not add an “Agents” activity screen in this PR. Such a screen would require a separately designed join between runtime/session status and account quota data. Without that model, it would mislabel provider-account quota as per-agent usage.

## Current screen inventory

| Surface | Current implementation | Assessment | Required direction |
|---|---|---|---|
| S1 Status item | `StatusItemLabel.swift`; strip, worst, pinned, icon-only, dual buckets, depleted countdown, degraded dimming, screen-share collapse | Feature-complete structurally; some displayed percent strings are composed in Swift | Keep modes; move every usage string/token to Rust DTOs; validate width, VoiceOver, stale, depleted, and screen-share states on real macOS |
| S2 Glance popover | `PopoverRoot.swift`; nine-tile grid, stacked full details for all enabled surfaces, focused details, account pills, footer actions | Overloaded and contradictory: roadmap/ADR direction says overview-only, while source and public guide make it a second dashboard | Restore the compact glance boundary: concise overview rows plus status/refresh/footer; open Usage for detail |
| S3 Usage overview | `UsageWindowRoot.swift` + `OverviewListView.swift`; sidebar and provider cards with up to four numeric buckets | Good primary dashboard foundation; empty and degraded states are too generic; selected account context is easy to miss | Make this the authoritative all-surface dashboard; improve state clarity and selected-account labeling |
| S4 Provider/account detail | `ProviderCardView.swift`; identity, account pills, buckets, reset, pace, limit copy, estimate/error | Strong coverage; account selection correctness is currently undermined by snapshot authority bug | Retain; fix data authority first; add explicit refreshing/stale/unavailable presentation and per-account freshness |
| S5 First-run/empty/error states | Inline text in popover, overview, and provider card | Implemented minimally and inconsistently; “No data” conflates disabled, signed out, refreshing, unsupported, and failed | Replace with a shared Rust-owned state projection and actionable native presentation |
| S6 Settings | `SettingsView.swift`; display mode, strip cap, formats, privacy, login, surface toggles, refresh floor, about | Broad enough for v1; refresh floor is not persisted across app restart and terminology mixes agents/providers/surfaces | Persist policy through the authoritative config owner or remove the false-persistent control; clarify terminology and previews |
| Distribution | `release.yml` + `desktop/sign_notarize.rs` | Publish path requires secrets, Developer ID, accepted notarization, stapling, Gatekeeper, release-mode verification, post-staple ZIP extraction verification, and publish-only artifact upload | Keep architecture; run real production proof once Apple credentials exist; do not claim stable installation before then |

## Merge blockers

### P0.1 Move all bridge I/O and provider refresh work off the main actor

**Evidence:** `PresentationStore` is `@MainActor`. `open()` synchronously calls `openRuntime`, then forces `refreshAll(force: true)`. `refreshAll`, `refresh(surfaceId:)`, `setEnabled`, account selection, polling `refreshDue`/`refresh`, event reads, and snapshot projection all synchronously cross UniFFI from the main actor. Provider probes may invoke network or provider CLIs, so status-item interaction, window rendering, keyboard commands, and accessibility can freeze.

**Implementation:** Introduce one serialized bridge executor owned by `PresentationStore` or the bridge module. It must run blocking UniFFI calls on a non-main executor while preserving one ordered mutation stream for open, refresh, enable/disable, format changes, and account selection. Return immutable `Sendable` projections to the main actor. Add a monotonically increasing operation/generation identifier so an older refresh cannot overwrite a newer account selection or settings change. Coalesce timer refreshes; allow one pending manual force refresh rather than concurrent probes. Cancellation must prevent UI projection, not abandon Rust cleanup midway.

**Do not:** wrap each call in unrelated detached tasks. That would trade freezes for out-of-order account selection, stale overwrites, and concurrent probe races.

**Likely files:** `native/Sources/JackinUsageBridge/PresentationStore.swift`, `native/Sources/JackinUsageBridge/UsageMenuBarBridge.swift`, bridge tests, architecture tests.

**Acceptance:** A test bridge that blocks refresh for at least one second does not block a main-actor heartbeat or status-item interaction; selecting account B while refresh A is in flight ends on B; repeated polling produces at most one active refresh; shutdown waits for/serializes bridge teardown safely.

### P0.2 Make shared snapshots authoritative for current/live accounts

**Evidence:** `HostUsageRuntime::snapshot` starts from process-local cache. `resolve_account_view` immediately returns that local `live` view when no account is selected, the selected key is empty, or it matches the live key. Shared snapshots participate in freshness comparison only through `collect_account_views`, after those early returns. A newer Capsule-written snapshot for the same current account can therefore be ignored by Desktop.

**Implementation:** Resolve the live/current account through the same account-keyed candidate collection and `fetched_at_epoch` freshness comparison as non-live accounts. The process-local view remains a candidate, not an unconditional winner. Preserve source/status honesty and stale downgrade semantics. Keep account identity matching provider-safe so equal-looking labels from distinct surfaces cannot collide.

**Likely files:** `crates/jackin-usage/src/host.rs`, `crates/jackin-usage/src/host/accounts.rs`, `crates/jackin-usage/src/host/tests.rs`.

**Acceptance:** Table-driven tests cover: no selection + newer shared same-account view; explicit live key + newer shared view; newer local view; stale shared vs fresh local; durable-store vs shared collision; different account key; different surface/provider; unreadable shared file. Desktop and a container writer must converge on the newest valid same-account snapshot.

### P0.3 Enforce Rust ownership of every displayed quota string

**Evidence:** `PresentationHelpers.swift` computes used percentage as `100 - remaining`, appends `%`, creates `% left`/`% used`, parses reset text, splits pace strings, formats money, and chooses labels. This contradicts the native hard rule that Rust owns every usage/limit number and display string. The architecture test currently catches only direct `Text("…\(…)%")` interpolation, not helper-level composition.

**Implementation:** Extend Rust view DTOs with the exact compact tokens, primary metric labels, accessibility labels, depleted labels, bar fraction, and formatted money/limit text required by each native presentation. Swift may lay out supplied strings and use numeric normalized fractions solely for geometry; it must not perform complement arithmetic, add units, parse semantic strings, or infer quota meaning. Delete or narrow the helper functions after call sites consume Rust fields. Strengthen static architecture checks to reject percentage arithmetic/string suffixing and money formatting in all handwritten Swift display modules without relying on forbidden-provider words in comments.

**Likely files:** `crates/jackin-usage/src/host.rs`, Rust view shaping modules, `crates/jackin-usage-ffi/src/lib.rs`, UniFFI bindings, `PresentationStore.swift`, `PresentationHelpers.swift`, all three native surfaces, bridge and architecture tests.

**Acceptance:** Golden tests in Rust pin left/used, dual-bucket, depleted reset, no-data, money-cap, exact/countdown, and accessibility strings. A source scan proves Swift contains no `100 - remaining`, percent suffix composition, semantic reset parsing, pace splitting, or currency formatting.

### P0.4 Drain current CI failures without hiding them

At audit time the following failures were live:

1. Native Swift tests: `PresentationHelpers.swift` comment mentions `Gemini`, `Copilot`, and `Cursor`, triggering three architecture assertions. Remove the unnecessary names; do not weaken the provider-catalog guard. Then extend the guard for the real display-string ownership issue described above.
2. Rust checks: three denied `collapsible_if` findings in `host/accounts.rs` at CI-source lines 100, 125, and 227. Rust tests themselves completed before `cargo check`; the failing crate jobs mostly cascade from these shared warnings and must not be reported as independent behavioral test failures.
3. Formatting: `cargo fmt --check` reports diffs across `accounts.rs` and multiple desktop xtask files, including `bootstrap.rs`, `release_state.rs`, `sign_notarize.rs`, `tests.rs`, and `desktop.rs`. Run the formatter, inspect the resulting diff, then rerun the check.
4. Strict xtask lint: unsorted directory iteration in `desktop.rs` at lines 962 and 991. Use `crate::fs_util::read_dir_sorted` as the gate instructs; do not widen the ratchet.
5. Repo links: root `AGENTS.md` has two invalid references to crate AGENTS files and the roadmap MDX has an unlinked root `AGENTS.md` reference. Respect the no-cross-AGENTS-link rule rather than mechanically adding forbidden links: remove/restate the root references as needed, and use `<RepoFile path="AGENTS.md" />` only in the contributor MDX.
6. Spelling: source flags `remainings` and `codesign`; docs flag `killall`, `logomark`, and `remainings`. Prefer normal wording where possible; add only legitimate command/product terms to the dictionary.
7. README freshness advisory: inspect the full report after the blockers above and update only READMEs whose ownership/API/module layout changed.

**Acceptance:** every required PR check is green. Do not bypass or mark expected any current failure.

## P1 screen coherence improvements

### P1.1 Make the glance popover a glance again

**Decision:** Follow the roadmap’s explicit S2 boundary: overview-only in the popover; provider/account detail belongs in the Usage window.

**Why:** The current 340-point-wide popover stacks full cards for every enabled surface under a nine-tile grid. It duplicates the Usage window, demands long scrolling, and makes transient menu-bar interaction carry dashboard complexity. The roadmap text, S2 sketch, and acceptance record say detail opens a normal window. The public guide and current source disagree with that contract.

**Implementation:** Replace the tile-plus-full-detail body with a compact list of enabled usage surfaces in stable Rust order. Each row should show provider display identity, selected account when disambiguation is needed, Rust-owned driving quota/status, reset, freshness/severity, and a chevron. Clicking a row selects that surface and opens Usage. Keep Refresh, Open Usage, Settings, Quit, and next-refresh text pinned. Disabled/first-run state gets one clear Settings action. If the catalog switcher is valuable, keep it in the Usage window sidebar rather than maintaining a second selector.

**Likely files:** `PopoverRoot.swift`, popover harnesses/snapshots, public guide, roadmap wording where current claims conflict.

**Acceptance:** Common data fits without scrolling at six enabled surfaces on the smallest supported display scale; eight surfaces may scroll only the overview list while footer remains pinned; keyboard and VoiceOver can traverse rows and invoke actions; no provider card implementation remains duplicated in the popover.

### P1.2 Turn no-data/error conditions into explicit states

**Problem:** Current surfaces use variants of “No data”, “No quota data yet”, a top-level raw error, reduced opacity, and status badges. Disabled, refreshing, needs-login, needs-secret, unsupported, stale-last-good, provider failure, and genuinely empty quota are not consistently distinguishable.

**Implementation:** Add a Rust-owned presentation state DTO with stable kind, title, detail, whether last-good data remains valid, and allowed action intent. Swift maps action intents only to native navigation such as Open Settings or Refresh; it does not author diagnosis. Apply one state component to glance rows, overview cards, and provider detail. Keep last-good buckets visible under stale/error state with exact freshness. Never replace known quota with a spinner or blank card.

**Likely files:** Rust host/view shaping, FFI DTOs, `PresentationStore.swift`, `OverviewListView.swift`, `ProviderCardView.swift`, compact popover row.

**Acceptance:** Golden matrix covers disabled, initial loading, fresh, stale with last-good, needs login, needs secret, unsupported, provider error with last-good, provider error without data, and no enabled surfaces. Every state has truthful copy and only an action the app can actually perform.

### P1.3 Clarify usage-surface, provider, and account identity

**Problem:** Swift identifiers and UI copy alternate among agent, provider, and surface. The model includes direct agent surfaces and routed providers, so the words are not interchangeable. Current cards often use surface labels even though roadmap parity promises shared provider display labels.

**Implementation:** Define one Rust projection for `surface_label`, `provider_display_label`, and account identity. Use provider display identity for quota cards and overview rows, with agent/surface context only where it disambiguates routing. Settings may call the catalog “Usage surfaces” and explain that disabling one stops its quota refresh. Rename Swift-local `allAgents`/`enabledAgents` and accessibility label “Agents” to surface terminology. Do not expose the dummy `codex` probe slug used internally for Z.AI/MiniMax routing.

**Acceptance:** Claude/Anthropic, Codex/OpenAI, Grok/xAI, and routed Z.AI/MiniMax render without implying a false agent/provider relationship; accessibility uses the same terms as visible copy; Capsule and Desktop display labels remain byte-equivalent where parity is required.

### P1.4 Make Settings behavior honest and previewable

**Implementation:** Persist refresh-floor policy through Rust’s authoritative host configuration or remove the slider until persistence exists; opening the app must not silently reset a user-selected interval to 300 seconds. Add a compact, non-live preview of the chosen menu-bar mode using Rust fixture/display DTOs so strip cap, percent style, reset style, and pinned surface are understandable before closing Settings. Disable or explain pinned choices that are disabled. Group “Launch at login” and privacy separately from quota formatting. Keep settings content free of provider probe logic.

**Acceptance:** Relaunch preserves every setting presented as persistent; changing format updates status item, glance, overview, detail, and VoiceOver labels atomically; preview uses no invented percentages and performs no provider refresh.

### P1.5 Complete native interaction and visual acceptance

Run one real Apple Silicon matrix against a built app, not only source/harness tests:

| Axis | Minimum coverage |
|---|---|
| macOS | 14 fallback materials and 26 Liquid Glass |
| Appearance | Light, dark, Increase Contrast, Reduce Transparency |
| Input | Mouse, keyboard-only, VoiceOver |
| Data | no surfaces, one surface, all eight, dual bucket, more than four buckets, multi-account, very long account/plan/provider text |
| State | refreshing, fresh, stale-last-good, depleted with reset, needs login, unsupported, provider error |
| Window | smallest size, large size, remembered frame, open/close repeatedly, Settings and Usage open together |
| Privacy | screen sharing off/on transitions while values are visible |

Capture redacted screenshots for status item modes, compact glance, overview, provider detail, every state family, and Settings. Compare all strings to `jackin usage` or Capsule from the same snapshots. Production proof remains blocked until a notarized artifact exists.

## P2 additive screens after stabilization

### P2.1 Add an All Accounts view inside Usage

**Value:** Multi-account support exists, but today account pills are visible only after selecting each provider. Operators with personal/work identities cannot see which account is constrained across surfaces at once.

**Screen:** Add “Accounts” beside Overview in the Usage sidebar. Group by usage surface/provider. Each row shows account display label, selected marker, Rust-owned driving quota/status, freshness, and plan. Selecting a row opens that provider detail with the account selected. This is a quota-limit view, not spend ranking or history.

**Dependency:** P0.2 must land first. Extend the account descriptor in Rust with exact display fields rather than deriving them in Swift. Decide whether selecting a row mutates the globally selected account; make that effect explicit and reversible in UI.

**Effort:** M.

**Acceptance:** Current, durable, and shared accounts appear once each; freshness winner is deterministic; account labels are safely redacted in screenshot fixtures; keyboard/VoiceOver expose selected state; no account can be selected for the wrong provider surface.

### P2.2 Add a Status & Sources view inside Usage

**Value:** An aggregate operational view helps explain why a quota is absent without visiting eight cards. It uses existing limits-adjacent fields: status, updated time, auth origin, last error, and whether last-good data exists.

**Screen:** Add “Status” to the Usage sidebar only after the Rust presentation-state DTO exists. One row per enabled surface shows state, freshness, selected account, credential origin summary, and last successful update. Actions are limited to Refresh and Open Settings. Do not expose credential values, file contents, raw provider payloads, or internal account-key hashes.

**Effort:** S–M after P1.2.

**Acceptance:** The screen contains no additional probe path and no Swift diagnosis logic; it is useful in all-empty and partial-failure cases; sensitive credential data never appears.

### P2.3 Defer provider quick links and secondary-metric controls

Provider Usage Dashboard / Status Page links, provider incident lines, reset-credit expiry details, collapsible secondary metrics, and segmented capacity markers remain reasonable later improvements already named in the roadmap. Each requires Rust-owned metadata or a safe URL catalog first. Do not implement links as a Swift provider switch. Prioritize only after P0/P1 and the two Usage-window views above are validated.

## Explicit non-goals

- No token unit prices, model price tables, session cost estimates, or cost-of-usage screens.
- No historical usage/spend series, sparklines, bar charts, Today/Yesterday/30-day views, aggregate-spend donuts, or ranked spend legends.
- No Buy Credits, claim-reset, sign-in, OAuth, or credential-writing actions.
- No Cursor, Gemini, Copilot, or other provider-catalog expansion.
- No per-running-agent/session screen until runtime observability has an explicit data model and roadmap item.
- No Swift HTTP, OAuth, CLI scraping, provider mapping, usage arithmetic, semantic string parsing, or second cache.
- No release of an ad-hoc artifact and no claim of stable availability before notarized production proof.

## Recommended execution order

1. **Correctness branch within the existing PR:** P0.1 asynchronous serialized bridge; P0.2 shared snapshot authority; P0.3 Rust-owned display DTOs. Keep these as reviewable commits with focused tests.
2. **Mechanical gate repair:** formatter, clippy, sorted directory reads, architecture-comment false positives, links, spelling, README freshness. Do not mix these changes into the correctness commits.
3. **Re-run focused native/Rust gates:** establish that the base product is responsive, deterministic, and parity-safe.
4. **Screen-boundary correction:** P1.1 compact glance, P1.2 state system, P1.3 terminology, P1.4 Settings persistence/preview. Update public guide and roadmap in the same commits that change behavior.
5. **Interactive acceptance:** P1.5 redacted screenshot and live parity matrix on Apple Silicon.
6. **Merge #816 only when required checks are green and a human reviewer has reviewed the high-risk bridge/cache/release boundaries.** The PR size and lack of review make self-attested acceptance insufficient.
7. **After #816:** implement P2.1 All Accounts, then P2.2 Status & Sources as separate PRs. Keep deferred metadata-dependent ideas separate.
8. **Distribution activation:** provision the named GitHub environment secrets/variables through the existing operator process, run publish mode from `main`, verify the downloaded immutable ZIP and Homebrew cask on Apple Silicon, then close plan 004.

## Verification commands

Run through repository entry points; do not substitute old shell assembly paths:

```bash
rtk cargo fmt --all -- --check
rtk cargo clippy -p jackin-usage -p jackin-usage-ffi -p jackin-xtask --all-targets --all-features --locked -- -D warnings
rtk cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-xtask --locked
rtk mise run desktop-test
rtk mise run desktop-build -- 0.6.0 1
rtk mise run desktop-verify
rtk cargo xtask lint --strict
rtk cargo xtask docs repo-links
rtk cargo xtask roadmap audit
rtk cargo xtask research check
rtk cargo xtask ci --fast
```

On full Xcode:

```bash
cd native && swift test -c release
```

Before merge, inspect current checks rather than relying on the audited run:

```bash
gh pr checks 816
```

## Done criteria

- [ ] No blocking UniFFI/provider operation runs on the main actor.
- [ ] Current/live account display selects the newest valid account-keyed snapshot across local, durable, and shared sources.
- [ ] Swift lays out Rust-owned quota strings and does not calculate or semantically parse usage display values.
- [ ] Glance is compact; detailed quota/account cards have one owner in the Usage window.
- [ ] Every loading/empty/auth/stale/error/unsupported condition is distinct and tested.
- [ ] Usage-surface/provider/account terminology is truthful for direct and routed surfaces.
- [ ] Settings shown as persistent survive relaunch and format changes update every surface consistently.
- [ ] S1–S6 pass real macOS 14/26, accessibility, long-content, multi-account, and degraded-state acceptance with redacted evidence.
- [ ] Required PR checks pass; no gate is bypassed or weakened.
- [ ] Public guide, roadmap, acceptance record, and implementation describe the same screen boundaries and known production-proof residual.
- [ ] A notarized, stapled, post-archive-verified artifact is installed and exercised before the product is called generally available.
