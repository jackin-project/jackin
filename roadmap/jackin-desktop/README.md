# jackin❯ Desktop — agent usage macOS status bar

- **Status**: IN EXECUTION
- **Slug**: jackin-desktop
- **Created**: 2026-07-24 · **Updated**: 2026-07-24
- **Plan**: [`plans/jackin-desktop/`](../../plans/jackin-desktop/README.md)

## Intent

Develop the jackin❯ Desktop application. Its first feature is an agent usage
macOS status bar, following examples like OpenUsage and CodexBar, using the
"jackin-agent-usage" crate (user's wording; resolved 2026-07-24 to the
existing `jackin-usage` / `jackin-usage-ffi` crates — see Data &
integrations) as the basic implementation. jackin❯ Desktop will only use Swift Native UI and Liquid
Glass best practices and design concepts. The implementation will be in
Rust; Swift will only be used to display the information.

Destination: this ships when jackin❯ Desktop is notarized and
Homebrew-installable, and on a host with agent credentials it
auto-detects the providers and shows — with limits-only data — each
provider's glance availability % in the menu bar (weekly for six
providers, Amp Free daily for Amp), the CodexBar-style popover glance,
and the Capsule-parity Usage window, for all seven providers.

## Vocabulary

- **Agent Usage preview**: the popover glance surface opened from the
  status item. _Avoid_: glance panel, preview window, dashboard.
- **Provider**: one agent vendor surface (codex, claude, zai, minimax,
  kimi, amp, grok). _Avoid_: agent (ambiguous with running agents),
  surface (FFI term).
- **Account**: one credential within a provider; exactly one is
  _selected_ per provider and drives status bar % and Overview row.
- **Enabled provider**: auto-detected — has resolvable credentials/usage
  data on this host. Not a user toggle (no Settings for now).
- **Usage window**: the native macOS detail window, Capsule-parity
  content. _Avoid_: usage dashboard (that names provider web pages).
- **Glance %**: remaining percent of the selected account's weekly limit
  window, except Amp uses its server-reported Amp Free daily percentage —
  the only number in each menu bar item.

## Decisions

- 2026-07-24 — **Shipped Desktop v1 code is a reference, not a finished
  implementation.** This item plans refactoring of it plus continued
  implementation of everything missed. Because the operator judges v1
  incomplete; current code serves as the baseline to refactor and extend.
- 2026-07-24 — **Popover displays availability only, from data providers
  expose directly via their APIs, with no action buttons.** All detailed
  information lives in native macOS windows — the Usage window already
  previewed in this repository's plan
  (`plans/native-macos-usage-menu-bar/010-usage-window.md`: native window,
  glass sidebar Overview + providers, full provider card, Capsule parity).
  Because the popover is a glance surface; actions and detail belong to
  real windows.
- 2026-07-24 — **Popover footer keeps only a Refresh button.** No Settings
  row, no other action/link rows. Because refresh is the one glance-level
  action worth keeping; everything else lives in native windows.
- 2026-07-24 — **Status bar shows all enabled providers, each icon + one
  percentage: weekly for Codex, Claude, Grok, z.ai, Kimi, and MiniMax;
  Amp Free daily for Amp.** No stacked dual percentages. Because Amp
  replaced its free-tier output with `N% remaining today (resets daily)`
  and exposes no weekly window; relabeling daily as weekly is wrong
  (research ch. 11).
- 2026-07-24 — **Usage window keeps Capsule parity** (plan 010 invariant:
  same fields, same strings, same order as the Capsule usage dialog).
  CodexBar-style display applies to the popover (and status bar), not the
  Usage window. Because parity with the Capsule dialog stays the detail
  surface's contract.
- 2026-07-24 — **No Settings surface for now.** Popover Settings row and
  the dedicated Settings window are both out of scope; configuration UI
  deferred. Because current focus is pure availability display.
- 2026-07-24 — **Everything must always match Capsule design.** Capsule
  design is the source of truth for every Desktop surface; any design
  that cannot match Capsule must always be discussed in detail with the
  operator before deviating. CodexBar remains a display reference, but
  Capsule design wins on conflict.
- 2026-07-24 — **Status bar % and Overview row follow the selected
  account** of each multi-account provider (the chip selected in the
  provider tab; existing `set_selected_account` FFI). Because glance
  surfaces should track the account the operator is actively using.
- 2026-07-24 — **Claude on macOS: read the credential from the macOS
  Keychain** (bare service for default config; Claude Code's hashed service
  suffix for normalized custom `CLAUDE_CONFIG_DIR`), with custom/default
  file/account/cache isolation, consent before shared coordination/provider
  timeout, and denial local-only/terminal per service. Headless
  interaction-unavailable retains file fallback; every Desktop bridge access
  is serialized off-main. Because default macOS installs are Keychain-only
  (file deleted by Claude Code) — only working path today (research
  ch. 09).
- 2026-07-24 — **Run-out producer = Variant A (linear-from-window-start)**,
  computed in Rust and emitted through the existing `pace_label` composite
  ("… · Runs out in …") that the capsule TUI and Swift splitter already
  consume. Because it needs zero new data, windows are verified fixed-slot,
  and the consumers already ship (research ch. 07/09/10).
- 2026-07-24 — **Grok plan label comes from the server fields**
  (`x.ai/billing.subscription_tier`, already resolved display-first from
  remote settings by the official client);
  the `auth_mode == "oidc"` → "SuperGrok" heuristic is retired. Because the
  heuristic mislabels Free/Premium OIDC logins and a first-party field
  exists (research ch. 05).
- 2026-07-24 — **Grok monetary quota fields follow the official signed,
  proto-zero model.** `{}` means zero, negative accounting values render
  their checked magnitude, and on-demand renders only when not disabled and
  a positive cap exists. Because used-without-cap is spend, not an allowed
  quota-bound surface (research ch. 05).
- 2026-07-24 — **Enabled providers are auto-detected**: every provider
  with resolvable credentials/usage data shows in status bar and popover;
  none can be hidden until a Settings surface returns. Because there is no
  Settings surface for now (prior decision) and auto-detect needs no
  configuration.
- 2026-07-24 — **Usage window entry: both paths.** Status item
  right-click opens a small context menu (Open Usage Window, Refresh,
  Quit); clicking a provider header row in the popover opens the Usage
  window focused on that provider. Because the popover stays button-free
  (navigation, not action buttons) and right-click is the standard macOS
  status-item pattern.
- 2026-07-24 — **Future work plans under `plans/jackin-desktop/`** (via
  tailrocks-plan), folding in the still-open 003/004 distribution plans;
  `plans/native-macos-usage-menu-bar/` 013's screen roadmap gets
  reconciled against this item; the old program retires as executed
  history. Because one item, one plan home.
- 2026-07-24 — **Adopt Amp Free's current daily `displayText` now.**
  Parse `N% remaining today (resets daily)` plus individual/workspace
  credit balances; retire the hourly-dollar reader, speculative structured
  fallbacks, unconditional "Amp Free" hardcode, and
  replenishment-derived reset label. Daily applies only when the Amp Free
  line exists. Megawatt/Gigawatt paid-plan names and monthly inclusion
  stay capture-gated. Because the public live transcript and merged
  regression fixture now prove Amp Free daily, while no paid-plan
  `displayText` is public (research ch. 11).
- 2026-07-24 — **Always use CodexBar as the display reference.** For every
  usage element, first understand how CodexBar displays it, then verify
  what `crates/jackin-usage/` already provides. Display implementation
  remains clean-room (concepts only; never copy code or provider lists).
  Operator-requested source inspection may corroborate a changed provider
  wire contract, as with Amp daily, but produces an independent
  Rust-native design and fixture. Because CodexBar is the target display
  reference, not an implementation dependency.

## Capabilities

- Agent usage shown in a macOS status bar.
- Core implementation in Rust; Swift is display-only.

Scope of this item (decided 2026-07-24, over the v1 reference baseline):

- Distribution: notarized public ZIP + Homebrew cask + production install
  proof (repo plans 003/004). Headless — no screen; acceptance lives in
  the quality bar (notarized, stapled, Gatekeeper-accepted artifact).
- Design/UX refresh of the shipped surfaces.
- Finish the Agent Usage preview, following the operator's screenshot
  examples (references in this folder, catalogued under Screens).

## Screens

### macOS status bar item

Reference: [`reference-status-bar.png`](reference-status-bar.png)
(CodexBar menu bar strip).

- **Purpose**: show agent usage at a glance.
- **Content** (operator, 2026-07-24): all enabled providers, each as
  provider icon + one percentage of available quota — weekly for six
  providers, Amp Free daily for Amp, nothing more. (Reference strip shows
  a stacked dual-percentage variant; decided against — single glance %
  per provider.) "Enabled" = auto-detected: providers with resolvable
  credentials/usage data (decided 2026-07-24).
- **Schematic**:

```text
 menu bar:  …  [⊙ 57%] [✳ 74%] [Z 31%]  ⌚ 09:41
              (one item per auto-detected provider:
               template monochrome icon + glance % left
               of the selected account)
```

- **States** (decided 2026-07-24):
  - default — icon + weekly % left, or Amp Free daily % left for Amp;
    template monochrome (macOS convention; severity color lives in the
    popover only).
  - stale/error — % shows last-known value, dimmed; item never
    disappears (stable layout).
  - never-fetched — "–" instead of %.
- **Key interactions**: left-click — toggle popover (opens on that
  provider context); right-click — context menu (Open Usage Window,
  Refresh, Quit).
- **Navigation**: entry point of the whole app; out to popover or
  context menu.

### Popover — Agent Usage preview

Reference: [`reference-popover-overview.png`](reference-popover-overview.png),
crop [`reference-provider-tabs.png`](reference-provider-tabs.png).

- **Purpose**: glance surface — availability per provider, one click from
  the menu bar; no actions except Refresh.
- **Schematic** (Overview tab selected / provider tab selected):

```text
┌──────────────────────────────┐  ┌──────────────────────────────┐
│ [Overview] Codex Claude z.ai │  │ Overview [Codex] Claude z.ai │
│ MiniMax  Kimi   Amp    Grok  │  │ MiniMax  Kimi   Amp    Grok  │
│  (tab grid: icon over name,  │  ├──────────────────────────────┤
│   thin per-provider bar)     │  │ [a@x.com]  b@y.com   ← chips │
├──────────────────────────────┤  ├──────────────────────────────┤
│ Codex   ▓▓▓▓▓▓░░░░  57% left │  │ Codex ›        a@x.com       │
│ Claude  ▓▓▓▓▓▓▓░░░  74% left │  │ Updated 4m ago       Pro 20x │
│ z.ai    ▓▓▓░░░░░░░  31% left │  ├──────────────────────────────┤
│ …one compact row/provider…   │  │ Weekly                       │
│ (headline %, severity color, │  │ ▓▓▓▓▓▓▓░░░░░░ (segmented)    │
│  reset countdown right)      │  │ 57% left      Resets in 4d   │
│                              │  │ 13% in deficit Runs out in 2d│
│                              │  │ Codex Spark Weekly …         │
│                              │  │ Limit Reset Credits  3 avail │
│                              │  │ Credits  0 left    1K tokens │
├──────────────────────────────┤  ├──────────────────────────────┤
│ ↻ Refresh                 ⌘R │  │ ↻ Refresh                 ⌘R │
└──────────────────────────────┘  └──────────────────────────────┘
```

- **States**:
  - default — data as above, per-window severity colors (normal/warn/danger).
  - loading — last-known data stays visible; freshness line shows refresh
    in progress (no blank flash).
  - stale — "Updated Xm ago" dims; last-good values keep rendering
    (error never overwrites last-good).
  - error — provider section shows the Rust-provided error line under the
    header; other providers unaffected.
  - empty — no providers auto-detected: single hint line ("no agent
    credentials found"); status item still present.
- **Key interactions**: status item left-click — toggle popover; tab
  click — switch Overview/provider; account chip click — select account
  (drives status bar % and Overview row); provider header row click —
  open Usage window focused on that provider; Refresh (⌘R) — force
  refresh; Esc/outside click — dismiss.
- **Navigation**: in from status item left-click; out via dismiss, or
  provider header → Usage window.

Kept from reference (operator, 2026-07-24):

- **Provider tab grid at top** — grid of provider tabs (Overview tab +
  one tab per provider: icon above name), each provider with a thin
  progress bar underneath its name; selected tab highlighted (Overview
  shown selected in reference).
- **Overview tab** — simple preview of spent usage and availability per
  agent provider, without too much detail: clear understanding of what
  glance percentage is available per provider (weekly except Amp Free
  daily), in a compact aggregated way (operator, 2026-07-24).
- **Multi-account support per provider** — reference
  [`reference-provider-tab-accounts.png`](reference-provider-tab-accounts.png):
  provider tab shows an account-switcher chip row at top (selected account
  highlighted), per-account usage below (operator, 2026-07-24). v1 already
  lists multi-account in product scope (`native/README.md`).
- **Provider tab detail (Codex example)** — reference
  [`reference-codex-tab-detail.png`](reference-codex-tab-detail.png)
  (operator, 2026-07-24): provider header (name, account email, freshness
  "Updated 4m ago", plan label "Pro 20x"); per-window segmented progress
  bars (Weekly, Codex Spark Weekly) each with % left, deficit/reserve pace
  line ("13% in deficit"), reset countdown ("Resets in 4d 22h"), run-out
  projection ("Runs out in 2d 18h"); Limit Reset Credits block ("3
  available" + per-credit reset countdowns "3d 4h · 8d 55m · 19d 22h").
- **Credits section** — reference
  [`reference-credits-section.png`](reference-credits-section.png)
  (operator, 2026-07-24): credit-balance quota bar with remaining label
  ("0 left") and bound label ("1K tokens"). Limits-only compliant: credit
  balance is a provider-supplied quota bound, not a price or trend.
- **Claude provider tab** (operator, 2026-07-24; screenshot shown in chat,
  no exported asset on disk): header (Claude, "Updated just now", plan
  "Max 20x"); windows each as segmented bar + labels — Session (65% left,
  16% in deficit, Resets in 4h 4m, "Projected empty in 1h 46m"), Weekly
  (58% left, 51% in reserve, Resets in 12h 4m, "Lasts until reset"),
  Daily Routines (100% left), "Fable only" per-model window (25% left,
  Resets in 12h 4m).
- **Amp provider tab** — reference
  [`reference-amp-tab-detail.png`](reference-amp-tab-detail.png)
  (operator, 2026-07-24): header (Amp, account email, plan "Amp Free");
  single "Amp Free" window (100% left, "Resets daily"); Credits section —
  individual and workspace credit balances from Amp. (Reference footer
  rows — Usage Dashboard, Settings, About, Quit — superseded by the
  Refresh-only / no-action-buttons decisions of 2026-07-24.)
- **Grok provider tab** — reference
  [`reference-grok-tab-detail.png`](reference-grok-tab-detail.png)
  (operator, 2026-07-24): header (Grok, account email, plan "SuperGrok");
  Weekly window (48% left, 5% in deficit, Resets in 3d 16h, "Runs out in
  3d 1h"). (Reference footer rows superseded by the same decisions.)

### Usage window (detail)

Content contract: Capsule parity (decided 2026-07-24 — plan 010
invariant: same fields, same strings, same order as the Capsule usage
dialog; numbers come from the same Rust views).

- **Purpose**: full detail surface — everything the Capsule usage dialog
  shows, natively on the host, plus all actions that are not glance-level.
- **Schematic** (per plan 010 S3/S4: glass sidebar + content pane):

```text
┌───────────────┬──────────────────────────────────┐
│ Overview      │  Codex — a@x.com        Pro 20x  │
│ ─────────     │  Updated 4m ago                  │
│ ▸ Codex       │  ┌ full provider card ─────────┐ │
│   Claude      │  │ all buckets, used/limit      │ │
│   z.ai        │  │ labels, pace, resets,        │ │
│   MiniMax     │  │ credits, money caps,         │ │
│   Kimi        │  │ estimate captions, errors —  │ │
│   Amp         │  │ field-for-field = Capsule    │ │
│   Grok        │  └──────────────────────────────┘ │
│ (Capsule tab  │  [account chips when multi]      │
│  order)       │                                  │
└───────────────┴──────────────────────────────────┘
```

- **States**: default (selected provider card); Overview (sidebar top —
  overview rows for all providers); stale/error — honest degradation,
  Rust-provided strings rendered verbatim, error never overwrites
  last-good; empty — no providers detected, hint line.
- **Key interactions**: sidebar row click — switch provider; account chip
  click — select account; window is a normal macOS window (close/minimize;
  reopens via right-click menu or popover header click).
- **Navigation**: in via status-item right-click menu or popover provider
  header; out via window close.

## Flows

1. **Glance** — menu bar provider item left-click → popover opens on that
   provider's tab → read → Esc/outside click dismisses. Failure: provider
   stale/error → dimmed freshness + error line, last-good values shown.
2. **Detail** — status item right-click → "Open Usage Window" (or popover
   provider header click) → Usage window opens (focused provider) →
   window close. Failure: same honest-degradation states in the window.
3. **Account switch** — popover provider tab → account chip click →
   selection persists (Rust `set_selected_account`) → provider tab,
   Overview row, and status bar % all follow the new account. Failure:
   selected account has no data → never-fetched/stale states.
4. **Refresh** — ⌘R in popover or context menu → Rust refresh under the
   ≥60s floor → freshness lines update. Failure: provider fetch fails →
   error line for that provider only; last-good kept; other providers
   unaffected.
5. **First launch / no credentials** — app starts, auto-detect finds no
   providers → status item present, popover shows "no agent credentials
   found" hint → operator logs into agent CLIs (or grants Claude Keychain
   access when prompted) → next refresh picks providers
   up without restart. Failure: Keychain consent denied → Claude absent;
   other providers unaffected.

## Data & integrations

- Usage data comes from the existing Rust crates `crates/jackin-usage/`
  (host probes + `HostUsageRuntime`) and `crates/jackin-usage-ffi/`
  (synchronous UniFFI facade) — `native/README.md` layout table. The user's
  "jackin-agent-usage" name has no matching crate; these two are the shipped
  implementation (fact, resolved 2026-07-24).
- Coverage verified against kept screenshot elements (2026-07-24):
  - **Providers**: modules exist for exactly the seven in the reference —
    codex, claude, zai, minimax, kimi, amp, grok
    (`crates/jackin-usage/src/usage/*.rs`).
  - **Multi-account**: `AccountDescriptorDto` (account_key/label,
    plan_label, selected, remaining_percent, status_word) —
    `crates/jackin-usage-ffi/src/dto.rs:105`.
  - **Overview rows**: `OverviewRowDto` (display_label, headline,
    reset_label, exact_reset, status_word, severity) — `dto.rs:93`.
  - **Per-window buckets**: `QuotaBucketDto` (label, used/limit labels,
    remaining_percent, reset_label, resets_at, status_slot
    session/weekly/spend, pace_label, severity, money caps) — `dto.rs:45`.
    Amp daily needs a new semantic `daily` slot; no free-text window-name
    inference in Swift.
  - **Deficit/reserve pace line**: `quota_pace_label` emits CodexBar-style
    "N% in deficit" / reserve / on-pace
    (`crates/jackin-usage/src/usage/format.rs:164`, test
    `quota_pace_label_uses_codexbar_reserve_deficit_onpace`).
  - **Codex Spark windows**: "Codex Spark 5-hour" / "Codex Spark Weekly"
    labels present (`usage/codex.rs`, `usage_snapshot_store.rs`).
  - **Limit Reset Credits**: `CodexResetCredits` +
    `fetch_codex_oauth_reset_credits` (`usage.rs:74-83`).
  - **Credits balance**: Amp `individual_credits`, `out_of_credits`
    disabled-spend phrase (`usage/amp.rs:135`, `usage.rs:1170`).
  - **Format prefs**: percent left/used, reset countdown/exact clock
    (`UsageFormatPrefsDto`).
  - **Claude windows**: "Session", "Weekly", "Daily Routines", per-model
    Weekly windows all modeled (`usage/claude.rs:112-283`); reserve
    phrase "N% in reserve" emitted (`usage/format.rs:187`).
  - **Grok**: plan label (`grok_plan_label`) + billing cycle filling the
    Weekly slot, no session window (`usage/grok.rs:198-262`).
  - **Gap**: run-out family phrases ("Runs out in 2d 18h", "Projected
    empty in 1h 46m", "Lasts until reset") not found in `jackin-usage` —
    only deficit/reserve pace labels exist; candidate new work.
  - **Gap**: Amp parser recognizes only the retired hourly-dollar format,
    lacks the daily semantic slot, and omits workspace lines from the
    now-proven current `displayText` (research ch. 11); candidate new work.
- CLI surface: `jackin usage <INSTANCE> accounts|verify` (human/json
  formats, `crates/jackin/src/cli/usage.rs`); local cache currently empty
  ("no cached usage accounts"), so live output not observable this session.
- Decision implications (2026-07-24): Claude credential resolution gains a
  macOS Keychain reader (default/custom service derived by the shared core
  helper; serialized consent before timeout/locks; explicit denial terminal,
  headless interaction-unavailable falls back) ahead of the file paths;
  run-out label produced in Rust
  (Variant A) and appended to `pace_label` as "… · Runs out in …"; Grok
  probe reads the billing response's resolved `subscription_tier`, current
  nested period/balance fields, bounded enabled on-demand fields with
  zero/signed-safe cents, and ignores history/product usage;
  provider set is auto-detected from resolvable credentials.

## References

- OpenUsage — named example for the usage status bar; already cited as
  clean-room architecture reference in
  `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`.
- CodexBar — named example for the usage status bar; already cited there as
  clean-room UX reference ("visual reference only", `native/README.md`).
- `crates/jackin-usage/` — existing usage crate in this repository.
- `crates/jackin-usage-ffi/` — existing FFI layer for the usage crate.
- `native/` — existing Swift package; jackin❯ Desktop v1 (native macOS usage
  menu bar) merged in PR #816 (commit e7c9412, 21k insertions): status item,
  popover, Settings, Usage window, Liquid Glass fallbacks, display-only
  Swift over UniFFI.
- `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`
  — repo's own roadmap page for this feature. Status "Partially
  implemented"; remaining: plan 003 (notarized public ZIP + Homebrew cask,
  blocked on Apple secrets in `release-macos`), plan 004 (production install
  proof, deferred until 003 ships).
- `plans/native-macos-usage-menu-bar/` — repo's own implementation plans
  001–012 for Desktop v1.
- ADR-011 (`/reference/adrs/adr-011-native-macos-usage-menu-bar/`) —
  shipped architecture decision record.
- [`reference-popover-overview.png`](reference-popover-overview.png) —
  operator's screenshot reference for the Agent Usage preview (added
  2026-07-24): CodexBar-style popover with provider tab grid (Overview +
  per-provider tabs), per-provider sections showing account email, plan
  label, "Updated just now", per-window bars (Session/Weekly/etc.) with
  % left, deficit/reserve notes, reset countdowns and run-out projections.
  Screenshot also contains cost/spend/token-history elements (Today/30d $,
  token totals, spend bar charts, "Top model") that fall under the
  repository's usage-surfaces hard rule and cannot be copied; the kept
  subset is catalogued under Screens (resolved 2026-07-24).

## Research

- [`research/agent-usage-provider-apis/`](../../research/agent-usage-provider-apis/README.md)
  — per-provider availability API map (endpoints, auth, fields, what must be
  client-computed), run-out phrase provenance + candidate formulas, jackin-usage
  coverage gaps (macOS Claude Keychain hole, Amp/Grok/z.ai/MiniMax/Kimi fixes),
  and the queued operator-gated probe list. Vetted 2026-07-24.

## Must not

- MUST NOT put logic beyond displaying information in Swift — user constraint:
  implementation is Rust, Swift display-only.
- MUST NOT put action buttons in the popover — popover is
  availability-glance only; detail and actions live in native macOS
  windows (operator, 2026-07-24).
- MUST NOT show token unit prices, spend/usage-over-time charts, trend
  sparklines, token/spend histories, aggregate-spend donuts, or cost-legend
  rankings — repository hard rule (CLAUDE.md "Usage surfaces = limits only"):
  usage surfaces show subscription/quota limits only; OpenUsage/CodexBar may
  show those, this project does not copy them.

## Quality bar

- Swift Native UI only; Swift renders Rust-provided strings verbatim —
  zero computed/reworded labels in Swift (existing architecture test
  gate).
- Liquid Glass on the navigation/control layer only, per Apple HIG;
  content on standard materials; macOS 14/15 and Reduce Transparency fall
  back to system materials (existing `GlassFallbacks` gate).
- Capsule parity checkable: any number visible in the Usage window equals
  the Capsule usage dialog's value for the same account at the same
  fetch — a disagreement is a bug by definition.
- Limits-only audit: no token unit price, spend history, trend, or
  cost-ranking string anywhere in the app.
- Degradation honesty: an error never overwrites last-good data; stale
  data is visibly dimmed with its age.
- CI: desktop build + verify + Swift tests green; release artifact
  notarized, stapled, Gatekeeper-accepted (distribution scope 003/004).

## Open questions

- ~~With no Settings surface, what defines an "enabled" provider?~~
  DECIDED 2026-07-24: auto-detect (see Decisions).
- ~~Accept macOS Keychain consent prompt for the Claude credential?~~
  DECIDED 2026-07-24: yes, Keychain read (see Decisions).
- ~~Run-out producer variant?~~ DECIDED 2026-07-24: Variant A via
  `pace_label` composite (see Decisions).
- ~~Grok plan label from server field?~~ DECIDED 2026-07-24: yes,
  heuristic retired (see Decisions).
- ~~Amp Free daily reparse?~~ DECIDED 2026-07-24: implement now from the
  public live capture and regression fixture (see Decisions/research
  ch. 11). Paid-plan parsing remains capture-gated.
- ~~Relation to plans/native-macos-usage-menu-bar/?~~ DECIDED 2026-07-24:
  new `plans/jackin-desktop/` home (see Decisions).

## Open research questions

- ~~Per provider: which API endpoint exposes the availability data; which
  kept elements have no direct API source~~ ANSWERED 2026-07-24 →
  [`research/agent-usage-provider-apis/`](../../research/agent-usage-provider-apis/README.md).
  Residue: the operator-gated probe list in that topic's "Open unknowns"
  (live captures via agent-browser + live-key curls) — run as a follow-up
  research session with the operator present.
- ~~Run-out family phrase semantics~~ ANSWERED 2026-07-24 → same topic:
  phrases confirmed CodexBar UI vocabulary (codexbar.app + release notes);
  no API supplies them; three candidate Rust-side formulas documented with
  trade-offs (chapter 07). Variant decided 2026-07-24 (Variant A, see
  Decisions).
- ~~Amp Free daily + workspace line shape~~ ANSWERED 2026-07-24 →
  [`research/agent-usage-provider-apis/11-amp-daily-followup.md`](../../research/agent-usage-provider-apis/11-amp-daily-followup.md).
  Residue: live capture of Megawatt/Gigawatt/linked-subscription
  `displayText` (operator-authenticated `amp usage` or
  `userDisplayBalanceInfo` response); this gates only paid-plan/monthly
  parsing, not Amp Free daily.

## Deferred

## Log

- 2026-07-24 — tailrocks-idea — created (DRAFT).
- 2026-07-24 — tailrocks-brainstorm — first touch (SHAPING); resolved crate
  fact (jackin-usage / jackin-usage-ffi, no jackin-agent-usage crate);
  mapped shipped Desktop v1 state into References.
- 2026-07-24 — tailrocks-brainstorm — grilling session: 8 reference
  screenshots captured; 10 decisions recorded (v1 = reference baseline;
  scope = distribution + design refresh + finish Agent Usage preview;
  CodexBar display reference under Capsule-design supremacy; popover =
  availability only, Refresh-only footer; status bar = all enabled
  providers, selected-account glance % (Weekly for six; Amp Free Daily);
  Usage window keeps Capsule
  parity; no Settings for now); jackin-usage coverage verified with two
  gaps (run-out phrases, Amp workspace credits); provider-API research
  method agreed (agent-browser with operator login).
- 2026-07-24 — tailrocks-research — topic
  [`agent-usage-provider-apis`](../../research/agent-usage-provider-apis/README.md)
  created (--deep: 10 chapters, 2 critic rounds, all vetted). Both open
  research questions answered; 4 new decision questions added (Keychain
  consent, run-out variant, Grok/Amp plan labels); operator-gated probe
  list queued in the topic. Status stays SHAPING.
- 2026-07-24 — tailrocks-record-decision — four decisions recorded
  (Claude macOS Keychain read; run-out Variant A via pace_label; Grok
  server plan field; enabled providers auto-detected); four open questions
  struck; status stays SHAPING.
- 2026-07-24 — tailrocks-finalize — closing interview: Usage-window entry
  decision (right-click menu + header click), status-bar degradation
  states, popover/status-bar/Usage-window schematics confirmed, five
  flows, checkable quality bar, destination sentence, vocabulary; last
  two open questions decided (plans home = plans/jackin-desktop; Amp
  reparse after capture); readiness gate passed → READY.
- 2026-07-24 — tailrocks-plan continuation — operator directed Amp Free
  daily adoption after CodexBar re-check; deep follow-up research chapter
  11 verified the live percentage line, daily reset semantics, and
  workspace balances; status-bar contract refined to weekly-for-six +
  Amp-Free-daily; paid subscription parsing remains capture-gated.
- 2026-07-24 — tailrocks-plan — generated and cold-reviewed 11 executable
  slices plus coverage/spec hub and autonomous
  [`GOAL.md`](../../plans/jackin-desktop/GOAL.md); current Amp Free Daily,
  Claude terminal-local Keychain policy, current Grok billing config,
  AppKit lifecycle, shared Capsule/Desktop detail presentation, verification,
  distribution, and reconciliation are fully planned → PLANNED.
- 2026-07-24 — execution — plan 001 shipped the provider-core correctness
  fixes (Codex camelCase/Bedrock account tags with soft decode degrade,
  MiniMax documented `www.minimax.io` host + tested fan-out helper, z.ai
  `data.level` plan label) and the Amp Free Daily contract (one shared
  `displayText` parser, semantic `StatusSlot::Daily`, workspace credit
  bounds detail-only) → IN EXECUTION.
- 2026-07-24 — execution — plan 002 shipped the Claude macOS Keychain
  credential read: shared `jackin-core` service derivation reused by instance
  provisioning + the usage probe, Keychain-first wave resolution with
  process-terminal denial cache, typed `UsageSnapshotPolicy` (Shared /
  LocalOnly) governing preservation/coordination/persistence/materialization
  and host history filtering, and a Swift `RefreshScheduler` serializing all
  bridge access off `@MainActor` with cold-open + refresh coalescing. Verified
  via `cargo xtask ci --fast` + `cargo xtask desktop test` + DesktopArchitectureLint.
  XCTest suites deferred to a full-Xcode environment (CLT-only host here).
- 2026-07-24 — execution — plan 003 shipped the current Grok billing decoder:
  server-resolved `subscription_tier` plan label (auth heuristic retired), one
  Weekly headline with pace from `currentPeriod`/`billingPeriod` windows, and
  prepaid-balance / on-demand quota bounds (limits-only). Rust-verified via
  `cargo nextest` + clippy; built on plan 005's shipped Step-1 balance-only
  bucket-presentation contract.
- 2026-07-24 — execution — plan 004 shipped the Variant A run-out producer:
  `quota_pace_label` appends `· Runs out in <duration>` from Rust (exact
  integer cross-products; TUI/Swift splitters unchanged). 9 pace tests.
- 2026-07-24 — execution — plan 005 shipped the per-provider status bar: the
  Rust-owned seven-provider glance contract (`provider_glance_rows`) over
  UniFFI (`ProviderGlanceRowDto`), and an AppKit `@main`/`StatusBarController`
  that renders one `NSStatusItem` per auto-detected provider (canonical order,
  in-place reconcile, jackin❯ fallback, stale/error dimming, one transient
  popover) displaying the Rust `barLabel` verbatim — replacing the SwiftUI
  `MenuBarExtra`. Verified via `ci --fast` + `desktop test` + `swift build` +
  a real `desktop build` and ephemeral launch smoke (app comes up as a
  menu-bar accessory without crashing). Legacy membership-toggle removal
  deferred to plan 006 (owns the popover); XCTest suites unavailable here.
