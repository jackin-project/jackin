# Unified Agent Usage Experience

- **Status**: BLOCKED — Plan 008 external release authorization required
- **Slug**: unified-agent-usage
- **Created**: 2026-08-20 · **Updated**: 2026-08-21
- **Plan**: [plans/unified-agent-usage/](../../plans/unified-agent-usage/)

## Intent

Finalize one agent usage experience across jackin❯ desktop, `jackin console`, the `jackin usage` command, and `jackin-capsule`. When this ships, every applicable surface presents the same Rust-owned canonical accounts, quota windows, freshness, lifecycle truth, and refresh authority—without duplicated accounts—through its confirmed native interaction grammar.

## Vocabulary

- **Initialized agent**: An agent for which at least one session has been started in the current Capsule. _Avoid_: using “initialized” to mean that a usage account or capability was resolved.
- **Agent-uninitialized error**: The typed Capsule-only lifecycle error
  `agent_uninitialized`, emitted when a fully resolved launch-config agent has no
  started session. A quota preview may accompany it, but must not downgrade it
  to success or turn it into a provider-refresh failure.
- **Capsule quota preview**: Subscription or quota limits shown by `jackin-capsule` before an agent's first session, using a resolved usage capability when one exists. _Avoid_: applying this lifecycle state to the console, CLI, or jackin❯ desktop; model context-window tokens; token prices; or historical token usage.
- **Provider display name**: The provider-only label shown on usage surfaces: OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax, or OpenCode. _Avoid_: exposing agent/runtime names such as Codex, Claude, or Grok as part of a usage provider label; those remain internal identifiers or separate Capsule lifecycle identity when required.

## Decisions

- 2026-08-20 — **Bare `jackin usage` opens the host-wide deduplicated overview, while `jackin usage <instance> …` remains available for inspecting a particular Capsule instance.** Because the normal operator path should show all host usage immediately without removing instance-scoped inspection.
- 2026-08-20 — **Capsule shows every agent in the current fully resolved instance launch configuration; an agent with no started session carries the typed `agent_uninitialized` lifecycle error, with a quota preview when a resolved usage capability exists.** The lifecycle error remains visible beside any preview, is distinct from usage resolution or refresh failure, and never blocks launch.
- 2026-08-20 — **The host-wide `jackin usage` and `jackin console` views include all eight provider surfaces: OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax, and OpenCode; jackin❯ desktop retains the same catalog except OpenCode.** Because the host surfaces must cover every discovered agent and configuration while the desktop catalog remains a separate settled product boundary.
- 2026-08-20 — **`jackin usage` and the `jackin console` usage screen consume the same Rust-owned canonical inventory, deduplication, refresh, cache, and projection; only their presentation differs.** The CLI renders human-readable or JSON output, while the console renders the TUI, so both surfaces stay behaviorally consistent.
- 2026-08-20 — **One host broker owns provider refresh and durable canonical-account freshness.** Local processes and views may retain immutable projections for presentation, but they never probe providers, own retry deadlines, or queue duplicate refresh generations, because concurrent callers must share cached and in-flight work.
- 2026-08-20 — **Current read-only discovery across global, role, workspace, and workspace-role scopes owns host inventory membership; durable history only enriches current members, and unsupported or undiscovered providers do not appear as empty rows.** Because host-wide usage should reflect presently available configurations without resurrecting stale accounts or fabricating availability.
- 2026-08-20 — **Usage is a top-level route in `jackin console`, opening on Overview in the console's established left-list/right-detail structure.** Because usage is a primary host-wide operator surface and should reuse familiar console navigation rather than hide behind a workspace or modal.
- 2026-08-20 — **The console TUI orders provider groups by the settled eight-surface list and canonical accounts beneath them; it explicitly represents loading, refreshing, empty, stale last-good, partial-provider error, and global failure.** Selection drives detail, `R` refreshes, Back/Escape follows the shared navigation contract, and active keys appear in footer hints, because every state and action must remain visible and predictable.
- 2026-08-20 — **Human `jackin usage` output renders provider groups, one canonical row per account, then that account's limit windows, with explicit stale and error annotations; `--format json` exposes the same projection as a stable machine-readable envelope.** Because CLI and TUI must express the same truth without flattening canonical accounts back into duplicated window rows.
- 2026-08-20 — **Instance `accounts` and `verify` retain their Capsule inspection and verification intent; every host read is moved onto the canonical broker projection, and cache, `--no-refresh`, `--sync-host-cache`, or snapshot forms that preserve an independent freshness authority or misleading bypass are removed or redefined rather than kept as compatibility shims.** Because diagnostic value must survive without preserving the architecture that permits duplicate or stale authority.
- 2026-08-20 — **When a Capsule receives multiple launch-forwarded accounts for one fully resolved launch-config agent, its quota preview shows every deduplicated canonical account and supports account detail or selection.** Because collapsing to a surface-only request is ambiguous and hiding accounts would make the preview incomplete.
- 2026-08-20 — **The Capsule quota preview orders rows by fully resolved launch-config agent then canonical account and distinguishes `agent_uninitialized`, loading, available limits, no-capability, stale last-good, usage resolution failure, and refresh failure.** Its single refresh action joins active broker work, and selection survives the transition to an initialized session, because lifecycle and data freshness are independent and should not disrupt operator context.
- 2026-08-20 — **Quota data is informational and never authorizes or blocks Capsule agent launch or session actions, including when limits are exhausted, unknown, stale, or failed.** Because usage observation and launch policy are separate responsibilities, while explicit state remains sufficient for informed operator choice.
- 2026-08-20 — **Every usage surface consumes Rust-owned remaining/used conventions, rounding, countdowns, stale markers, missing-plan fallbacks, and money-cap units verbatim, adapting layout only.** Because presentation code must not infer or reinterpret quota meaning and create cross-surface disagreement.
- 2026-08-20 — **jackin❯ desktop derives a filtered view from the same canonical host discovery and account graph, excluding OpenCode and applying its frozen OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax order plus native presentation settings without a second identity or discovery pipeline.** Because one account graph preserves deduplication and broker authority while retaining the settled seven-provider desktop boundary and order.
- 2026-08-20 — **Bare `jackin usage` renders every usable row with explicit partial-provider stale or error state and exits successfully; it exits nonzero only for an invalid invocation or when no usable projection can be produced.** A valid completed membership evaluation is usable even when empty or unresolved-only; current or stale last-good rows make partial output usable. When every current member failed with no last-good row, human and JSON output still preserve structured failures but exit nonzero, because partial degradation is usable output while total provider failure is not.
- 2026-08-20 — **jackin❯ desktop retains Rust-ranked provider-focused status items with icon-only, worst-provider, pinned-provider, and bounded-strip modes.** Clicking an item focuses that provider and its selected canonical account, while the Usage window remains the all-provider overview, because glanceable urgency and complete exploration serve different operator moments.
- 2026-08-20 — **jackin❯ desktop preserves its native popover, retained two-pane Usage window, and Settings information architecture unless prototype evidence proves a specific structural defect.** Because the existing structure is native and coherent, while design work should target evidenced usability and craft gaps instead of decorative replacement.
- 2026-08-20 — **Finalizing jackin❯ desktop includes Developer ID signing, notarization, a public artifact, and Homebrew cask installation proof.** Because the native application is not complete until its release path satisfies the platform trust boundary and an operator can install the shipped artifact.
- 2026-08-20 — **The jackin❯ desktop Usage window keeps alternative A (Grouped Overview, Provider Detail) without the H popover remix, as selected by Alexey Zhokhov.** A is the structure the running incumbent prototype already proves buildable; every recorded baseline defect is a row/composition failure, not a structural one, so the fix is targeted rather than a re-architecture. B moves too many long account labels into navigation, G adds an urgency destination without baseline evidence that reaching depleted/stale rows is slow, and H removes complete quota-window detail from a popover whose baseline hierarchy already passed visual review. Full selection record: [`native/Design/UnifiedAgentUsage/Alternatives.md`](../../native/Design/UnifiedAgentUsage/Alternatives.md).
- 2026-08-21 — **Usage selection is preserved only while its stable canonical account remains present; if that account disappears, interactive surfaces return to Overview and show `Selected account is no longer available.` rather than silently choosing another account.** Because substituting a different account would misrepresent operator intent.
- 2026-08-21 — **The Codex adapter includes `individual_limit` / `spend_control.individual_limit` from `/wham/usage` as a monthly money-cap limit window on every applicable usage surface.** Because it is a provider-supplied subscription quota bound, not token pricing or spend history.
- 2026-08-21 — **The canonical broker owns an adaptive refresh cadence: 2 minutes after direct usage interaction, 5 minutes while recently active, 15 minutes when idle, and 30 minutes during long idle or Low Power Mode; menu/popover opening requests refresh only when policy says due.** Manual and automatic callers still join active work and never create parallel provider calls, because cadence must not weaken the single-authority invariant.
- 2026-08-21 — **A short quota window shows Rust-owned `Not started` when provider evidence explicitly distinguishes an untouched window; no surface infers it from 100% remaining alone.** Because identical percentages can represent different lifecycle states.
- 2026-08-21 — **Rust may derive `Runs out in N d` from quota-window pace and expose it only on rich Console, Capsule, and desktop detail surfaces when confidence is sufficient; the simple CLI and stale, not-started, or statistically weak windows omit it.** Because a bounded limits-only exhaustion estimate helps operator judgment without introducing cost or historical-usage surfaces.
- 2026-08-21 — **Browser-cookie decryption, session-cookie import, authenticated dashboard WebViews, and web scraping are outside the usage credential boundary; supported lanes remain agent CLI files, explicit Keychain items, and environment/config references.** Web-only quota windows remain unavailable until a provider exposes a supported API or existing trusted credential lane, because usage observation must not broaden browser-session trust.
- 2026-08-21 — **Console Usage uses the shipped ` jackin❯  · usage` frame, master-detail panels, focus/footer grammar, and Capsule-style full-width quota composition; Overview and Account Detail are its only destinations, with responsive compact rows inside the same split frame.** Because the new route must feel native to `jackin console`, not like a macOS window translated into text.
- 2026-08-21 — **Human `jackin usage` output is intentionally simpler than the rich Console and Capsule TUI: provider, canonical account, then compact one-line limits, with no bars, cards, animation, pace projection, or interactive chrome.** Because the CLI is a direct readout while JSON carries complete machine detail.
- 2026-08-21 — **Capsule Usage retains its existing modal and agent-tab grammar, adding a conditional canonical-account tab strip only for a multi-account resolved agent.** Because this preserves learned Capsule interaction while representing every launch-forwarded canonical account without global or duplicate provider tabs.
- 2026-08-21 — **Usage surfaces display provider names only—OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax, and host-only OpenCode—in that order; agent/runtime names such as Codex, Claude, and Grok are not appended to provider labels.** Because quota ownership should be named consistently without conflating providers and their agent products.
- 2026-08-21 — **Each account's Overview/sidebar summary uses its first available Rust-ranked limit in the order long-range weekly/daily/monthly, model-specific, session, then other; label, percentage, and reset come from that same window, and no aggregate is computed across unlike windows.** Stale or unavailable lifecycle truth overrides the quota summary, because a compact glance must remain semantically traceable to one real provider limit.
- 2026-08-21 — **Capsule Overview renders each fully resolved launch-config agent and every deduplicated canonical account beneath it, with provider name, account identity, independent agent lifecycle, and the confirmed account summary limit; it never invents a provider aggregate.** Because lifecycle and quota truth must remain visible per selectable account.
- 2026-08-21 — **Opening Capsule Usage from a focused agent's usage chip selects that resolved agent and its last valid canonical account; absent or removed focus falls back to Overview, and valid selection persists through modal refresh.** Because entry should preserve local operator context without silently substituting identity.
- 2026-08-21 — **Within each provider, canonical accounts sort by full display label using locale-aware case-insensitive order, with stable canonical account ID only as the tie-breaker; discovery source, workspace, freshness, severity, and selection never reorder them.** Because navigation must remain deterministic through refresh.
- 2026-08-21 — **When account removal returns an interactive surface to Overview, a surface-native inline `Selected account is no longer available.` notice remains until the operator selects another destination, starts another refresh, or leaves Usage; it is never a timed toast.** Because identity loss must remain visible long enough to understand the navigation change.

## Capabilities

- Provide agent usage CLI output through `jackin usage`.
- Preserve explicit Capsule-instance account inspection and verification while eliminating host-side cache and refresh bypasses.
- Make agent usage available inside `jackin console`.
- Show subscription and quota usage limits only, including remaining or used percentage, reset countdowns, plan and status, and provider-supplied limit windows such as money caps when they are quota bounds, as required by the repository's root agent rules.
- Ship jackin❯ desktop with Developer ID signing, notarization, a public artifact, and verified Homebrew cask installation.

## Screens

### Console usage

- **Schematic — Overview (console-frame correction confirmed 2026-08-20 by Alexey Zhokhov)**:

  ```text
   jackin❯  · usage

  ┌──────────────────────────┐┌ Overview ───────────────────────────────────────┐
  │▸  Overview               ││    OpenAI · personal@example.test               │
  │   OpenAI                 │└─────────────────────────────────────────────────┘
  │     personal@example.test│┌ Overview ───────────────────────────────────────┐
  │     team@example.test    ││    ███████████████████████░░░░░░░░░░░ 57% left │
  │   Anthropic              ││                                      Resets 3d │
  │     personal@example.test││                                                 │
  │   Amp              100%  ││    OpenAI · team@example.test                   │
  │                          ││    █████░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 12% left │
  │                          ││                                      Resets 1h │
  │                          ││                                                 │
  │                          ││    Anthropic · personal@example.test            │
  │                          ││    █████░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 12% left │
  │                          ││    Partial failure: Z.AI needs login             │
  │                          │└─────────────────────────────────────────────────┘
  │                          │
  │                          │
  └──────────────────────────┘

                   ↑↓ select · ↵ open   R refresh   Esc back · Ctrl-Q quit
  ```

- **Schematic — Account detail (confirmed 2026-08-20 by Alexey Zhokhov)**:

  ```text
   jackin❯  · usage

  ┌──────────────────────────┐┌ Account ─────────────────────────────────────────┐
  │   Overview               ││    Anthropic                 personal@example.test│
  │   OpenAI                 ││    Updated now                                   │
  │     personal@example.test││    Plan  Max                                     │
  │     team@example.test    │└─────────────────────────────────────────────────┘
  │   Anthropic              │┌ Limits ──────────────────────────────────────────┐
  │▸    personal@example.test││    Fable                                         │
  │   Amp              100%  ││    ███████████████████████░░░░░░░░░░░░░░░░░░░   │
  │                          ││    57% left                       Resets in 4d     │
  │                          ││                                                  │
  │                          ││    All models                                    │
  │                          ││    ██████████████████████████████████░░░░░░░░░   │
  │                          ││    85% left                       Resets in 4d     │
  │                          ││    28% in reserve             Lasts until reset   │
  │                          ││                                                  │
  │                          ││    Session                                       │
  │                          ││    ███████████████████████████████████░░░░░░░░   │
  │                          ││    89% left                       Resets in 2h     │
  │                          ││    34% in reserve             Lasts until reset   │
  └──────────────────────────┘└─────────────────────────────────────────────────┘

         ↑↓ select · ⇥ enter detail   R refresh   Esc back · Ctrl-Q quit
  ```

- **Purpose**: Show a basic usage overview plus detailed views per provider and account across all available agents and configurations.
- **States**: Loading; refreshing; empty; Overview; account detail; stale last-good; partial-provider error; global failure.
- **Key interactions**: Enter the top-level Usage route; select provider/account rows in the left list; inspect right-side detail; press `R` to refresh; use the shared Back/Escape behavior; follow active footer hints.
- **Navigation**: Enter through the console's top-level Usage route. The left list owns account navigation; Overview is the initial destination. Multi-account provider headings are taxonomy only and their canonical accounts are destinations; a single-account provider is a direct destination. Providers use the settled display order; accounts use locale-aware case-insensitive full-label order with stable canonical ID as tie-breaker. Back/Escape follows the shared console contract.
- **Quota composition**: Account detail matches the existing Capsule grammar: provider-owned limit label, full-width remaining meter, remaining/reset row, optional pace/run-out detail, one blank row between ordinary limits, and separators only for Rust-designated credit/reset sections. Overview uses the same full-width meter grammar for every canonical account and is named `Overview`, never `All accounts`. Rust owns labels, semantic order, values, lifecycle text, and severity; the console only adapts width.
- **Overview summary**: Each canonical account uses one real limit—the first available Rust-ranked long-range, model-specific, session, then other window—and keeps that window's label, percentage, and reset together. It never aggregates unlike windows. Stale or unavailable state replaces the current summary claim.
- **Responsive behavior**: Preserve the console's split layout through the proven 80×24 baseline; do not create separate narrow destinations. The left list retains its fixed selection prefix and horizontal label scrolling. When the right pane is too narrow for honest Capsule-style meter composition, each limit becomes one compact semantic row such as `Session  89% left · Resets 2h`; meters, pace, reserve, run-out, and lower-priority detail disappear before required identity, remaining, reset, or lifecycle text. Footer hints wrap through the shared console footer.
- **Loading and refreshing**: Before the first projection, the left list contains only selected `Overview` and a right panel titled `Usage` shows `Loading usage…`. Refresh with existing data never blanks the projection, resets selection, or opens a modal: account rows and limits remain visible while activity reads `Refreshing…`. Refresh joins the broker's active generation; the footer does not advertise a second actionable refresh while work is active.
- **Empty inventory**: When membership evaluation succeeds with no canonical accounts, the left list contains only selected `Overview`; a right `Usage` panel says `No usage accounts found.` and directs the operator to configure a supported agent account, then refresh. This is a successful empty state, not an error. It offers `R refresh` and `S settings` in the footer and never fabricates unsupported-provider rows.
- **Stale and partial failure**: Healthy accounts remain usable. Stale accounts retain last-good limits and meters while activity explicitly reads `Stale · Updated <age>`; reset text remains visibly cached context. An account with no cached limits replaces percentage and meter claims with the Rust-owned lifecycle text (`Needs login`, `Needs secret`, `Unavailable`, or `Error`). Selecting it shows the cause inline in the right panels. Partial failures never open a blocking modal or hide healthy accounts; `R refresh` retries through the shared broker.
- **Global failure**: Only when no current or stale last-good account can be rendered, the left list contains selected `Overview` and a right panel titled `Usage unavailable` says that no usable projection could be produced, followed by the sanitized Rust-owned cause. It offers `R retry`, fabricates no account rows, and opens no modal.
- **Design**: The shipped console frame is authoritative: a top-left ` jackin❯  · usage` route header, one blank spacer row, an untitled focused master list beside individually titled detail panels, then one blank spacer row and centered contextual hints. There is no outer page border, permanent global navigation rail, duplicate page title, or footer logo. Shared TermRock owns focus borders, selected rows, panels, scrolling, and hints. Quota composition follows the existing `jackin-capsule` usage dialog; canonical account grouping and account-only multi-account navigation follow the blessed [desktop prototype signoff](../../native/Design/Prototypes/UnifiedAgentUsage/SIGNOFF.md).

### Desktop usage

- **Schematic/reference — Usage window**: The rendered F00–F29 window matrix at 800×520, 1000×680, and 1200×760 is authoritative and blessed in [prototype signoff](../../native/Design/Prototypes/UnifiedAgentUsage/SIGNOFF.md); region ownership and production seams are fixed in [production mapping](../../native/Design/Prototypes/UnifiedAgentUsage/PRODUCTION_MAPPING.md). No ASCII redraw supersedes it.
- **Purpose**: Provide the agent usage experience as a native macOS app.
- **States**: Loading; Overview; account detail; multi-account; stale; unavailable with Retry; empty inventory; sidebar expanded/collapsed; minimum/default/wide window; active/inactive; Reduce Motion; Reduce Transparency; Increase Contrast; Differentiate Without Color; hover; keyboard focus; full screen; secondary display.
- **Key interactions**: Current baseline includes provider and account selection, Refresh, opening the retained Usage window from the popover, toggling its sidebar, and opening Settings through the application menu.
- **Navigation**: Overview is the initial Usage-window destination. Multi-account provider headings are taxonomy only and accounts are selectable; a single-account provider remains a direct destination. Sidebar collapse preserves the absolute-centered `jackin❯` identity. Settings is the existing application destination, not a new screen owned by this item.
- **Design**: Blessed dark-only native reference; Swift and system-owned AppKit/SwiftUI Liquid Glass own chrome, while authored quota content remains opaque. Rust-ranked provider-focused status items support icon-only, worst-provider, pinned-provider, and bounded-strip modes; the Usage window remains the all-provider overview.
- **Glance behavior**: Clicking a provider status item focuses that provider and its selected canonical account in the popover before deeper navigation.
- **Structure rule**: Human selection 2026-08-20 (Alexey Zhokhov): alternative A — Grouped Overview, Provider Detail — without the H popover remix; the native popover, two-pane Usage window, and Settings architecture are retained.

### Desktop status popover

- **Schematic/reference**: The rendered 380×520 popover-bearing fixture matrix is authoritative and blessed in [prototype signoff](../../native/Design/Prototypes/UnifiedAgentUsage/SIGNOFF.md); the display-local anchor and native ownership contract are fixed in [production mapping](../../native/Design/Prototypes/UnifiedAgentUsage/PRODUCTION_MAPPING.md).
- **Purpose**: Provide a fast provider/account quota glance from the clicked menu-bar status item and a direct path into the Usage window.
- **States**: Loading; current; stale; unavailable; compact multi-account content; active/inactive app; secondary-display and rightmost-menu-bar anchoring.
- **Key interactions**: Click a provider-focused status item; inspect its selected canonical account; refresh through the native command; open Usage; dismiss through standard popover behavior.
- **Navigation**: The clicked status item chooses provider/account context. `Open Usage` opens or focuses the retained Usage window at that destination. Popover placement is relative to the clicked status button on its display.
- **Design**: Native `NSPopover` and standard controls, always showing the centered official `jackin❯` signature. No custom glass imitation, `j>` substitute, or duplicate desktop subtitle.

### CLI usage output

- **Schematic — human output (confirmed 2026-08-20 by Alexey Zhokhov)**:

  ```text
  Agent usage · Updated now

  OpenAI
    personal@example.test · Pro
      Weekly   57% left · resets in 3d
      Session  89% left · resets in 2h

    team@example.test · Team · stale 47m
      Weekly   12% left · resets in 1h

  Anthropic
    personal@example.test · Max
      Fable    85% left · resets in 4d

  Z.AI
    work@example.test · needs login
  ```

- **Purpose**: Render the shared host-wide usage projection through bare `jackin usage`.
- **States**: Current, stale last-good, partial-provider error, empty, and global failure. Waiting on bounded broker work is not a rendered stdout state.
- **Key interactions**: Use human output by default or `--format json` for the stable envelope; pass an instance for Capsule-scoped inspection.
- **Navigation**: Headless command output; it never enters raw mode, opens an alternate screen, or provides interactive navigation.
- **Design**: Deliberately simpler than the Console and Capsule TUI: provider group, each canonical account exactly once, then compact one-line Rust-ordered limits. Human output has no bars, cards, animation, pace projections, or TUI chrome. Stale/error state belongs concisely on the account heading; unavailable accounts make no current quota claim. stdout contains only the final readout. JSON emits the complete stable canonical envelope and never contains ANSI or rendered bars; transient diagnostics belong on stderr.
- **Empty and failure**: Successful empty membership prints `Agent usage` then `No usage accounts found.` and exits zero. Total failure prints `Agent usage unavailable` followed by concise provider lifecycle rows, exits nonzero, and reserves detailed diagnostics for stderr. Partial failure stays inline beside usable account output.
- **Exit contract**: Empty or unresolved-only inventory exits zero when membership evaluation completed; partial stale or failed providers with current or last-good rows exit zero; all current members failed with no last-good row, invalid invocation, or failure to construct a schema-valid envelope exits nonzero. JSON retains structured failures whenever an envelope can be constructed.

### Capsule usage

- **Schematic — Overview (confirmed 2026-08-21 by Alexey Zhokhov)**:

  ```text
         ┌ Usage ─────────────────────────────────────────────────────────────┐
         │       Overview   Anthropic   OpenAI                               │
         │      ──────────                                                    │
         │────────────────────────────────────────────────────────────────────│
         │  Anthropic                                                         │
         │    personal@example.test   57% left · Resets 4d                   │
         │                            Agent not initialized                    │
         │    team@example.test       12% left · Resets 1h                   │
         │                            Agent not initialized                    │
         │                                                                    │
         │  OpenAI                                                            │
         │    work@example.test       85% left · Resets 4d                   │
         └────────────────────────────────────────────────────────────────────┘
  ```

- **Schematic — multi-account agent (confirmed 2026-08-20 by Alexey Zhokhov)**:

  ```text
         ┌ Usage ─────────────────────────────────────────────────────────────┐
         │       Overview   Anthropic   OpenAI                               │
         │                  ─────────                                         │
         │────────────────────────────────────────────────────────────────────│
         │       personal@example.test   team@example.test                   │
         │       ─────────────────────                                        │
         │────────────────────────────────────────────────────────────────────│
         │  Anthropic                                   personal@example.test │
         │  Agent not initialized · Updated now                              │
         │────────────────────────────────────────────────────────────────────│
         │  Plan  Max                                                        │
         │                                                                    │
         │  Fable                                                            │
         │  ██████████████████████████████████░░░░░░░ 85% left               │
         │                                                     Resets in 4d   │
         │                                                                    │
         │  Session                                                          │
         │  ███████████████████████████████████░░░░░░ 89% left               │
         │                                                     Resets in 2h   │
         └────────────────────────────────────────────────────────────────────┘
  ```

- **Purpose**: Show quota limits for every agent in the current fully resolved instance launch configuration and its canonical accounts, including an optional preview before the first session starts.
- **States**: `agent_uninitialized`; loading; empty launch configuration; available limits; no-capability explanation; stale last-good; usage resolution failure; refresh failure; initialized session.
- **Key interactions**: Select an agent and canonical account; inspect its windows; use one refresh action that joins active broker work; retain selection when the agent becomes initialized.
- **Navigation**: Preserve the existing modal's agent tab strip. Opening from a focused agent's usage chip selects that resolved agent and its last valid canonical account; missing or removed focus opens Overview. A second account tab strip appears only for a selected agent with multiple canonical accounts. Focus moves agent tabs → account tabs when present → scrollable quota content; BackTab/Escape reverses through those owners before closing according to the shared dialog contract. Valid selection persists through refresh.
- **Design**: Preserve the current Capsule Usage modal, panel, meter, responsive, scroll, and footer-hint grammar. Replace fixed global provider tabs with fully resolved launch-config agents only. Order agents by resolved launch configuration and accounts canonically; keep `agent_uninitialized` separate from quota availability or freshness. Never render unresolved, global, or capability-only rows.
- **Overview composition**: Group by fully resolved launch-config agent, then show every deduplicated canonical account once with provider-only display name, account identity, independent lifecycle, and the confirmed summary limit. Do not render a provider aggregate.
- **Empty launch configuration**: When the fully resolved launch configuration contains zero agents, show only the `Overview` tab and `No agents configured for this Capsule.` This is successful empty state: no global provider tabs, host-discovered accounts, fabricated rows, or Retry action.

## Flows

1. **Host Overview through Console.** The operator enters the top-level Console Usage route; the Console requests the canonical host projection and initially shows Console Loading. On success it opens Console Overview with each canonical account once. The operator selects a direct single-account provider or an account beneath a nonselectable multi-account provider heading, opening Console Account Detail without new discovery or provider work. `R` joins broker work and preserves visible last-good data. Partial failures stay inline; completed zero membership shows Console Empty; no usable current or stale projection shows Console Global Failure. If refresh removes the selected account, selection returns to Overview with the confirmed inline notice.
2. **Host read through CLI.** The operator runs bare `jackin usage`; the command requests the same canonical host projection, joins bounded broker work, then emits only the confirmed compact human readout or the stable JSON envelope. Completed empty membership prints the confirmed empty message and exits zero. Partial current/stale/error rows remain usable and exit zero. Total current-member failure with no last-good row preserves concise structured failures and exits nonzero. Invalid invocation or inability to build a schema-valid envelope also exits nonzero. CLI never enters raw mode or becomes a refresh authority.
3. **Instance inspection through CLI.** The operator runs `jackin usage <instance> …`; the command resolves that Capsule instance and reads its current instance-scoped projection for inspection or verification. A missing or ambiguous instance and an invalid subcommand fail explicitly. Existing `accounts` and `verify` diagnostic intent remains, but no cache, snapshot, `--no-refresh`, or sync form may bypass the host broker, create a parallel freshness authority, or silently convert instance data into host truth.
4. **Capsule pre-session quota preview.** The operator opens Capsule Usage from the current instance. The modal derives tabs only from the fully resolved launch configuration, opens Overview or the current agent, and exposes a conditional account tab strip when that agent has multiple canonical accounts. Before the first session, `agent_uninitialized` remains visible beside any available quota preview and never blocks launch. No capability produces an explanation, provider or refresh failure remains distinct from lifecycle, and stale last-good limits retain explicit age. When the first session starts, only the lifecycle error clears; account selection and quota state persist. Zero resolved agents shows the confirmed Capsule Empty state.
5. **Desktop glance to detail.** The operator clicks a Rust-ranked provider-focused status item on any display. The native popover opens relative to that clicked item, shows the official centered `jackin❯` signature and selected canonical-account glance, and requests refresh only when broker policy says due. `Open Usage` focuses the retained Usage window at that typed destination. Multi-account provider headings remain nonselectable; account selection opens detail, while single-account providers remain direct. Sidebar collapse, resize, appearance/accessibility changes, stale or unavailable transitions, and account removal preserve native chrome and either preserve stable selection or return explicitly to Overview.
6. **Shared refresh and recovery.** Any automatic or manual caller requests refresh from the one broker. The adaptive 2/5/15/30-minute policy determines automatic due work; every concurrent caller joins the active generation. Presentation surfaces retain immutable last-good projections while work runs. Provider-local failure updates only affected accounts; process exit, cancellation, or a second caller never transfers provider ownership to presentation code. Recovery publishes one new canonical generation atomically to all consumers.

## Data & integrations

- Rust owns the shared host provider/account projection consumed by both `jackin usage` and `jackin console`; their output adapters do not rediscover, rededuplicate, or refresh accounts independently.
- The host projection covers all eight usage surfaces. A derived filtered jackin❯ desktop projection remains limited to its fixed seven-provider catalog by excluding OpenCode.
- Rust owns desktop discovery, account identity, deduplication, broker coordination, quota shaping, and immutable projections; the boltffi boundary exports sanitized display data, and Swift remains display-only.
- Current read-only configuration discovery owns host inventory membership; history may enrich but never create membership.
- Capsule inventory membership is a separate instance-scoped filter derived only
  from the current fully resolved launch configuration. A resolved usage
  capability enriches an eligible agent with preview rows but never creates an
  agent row by itself.
- Rust owns all quota labels and formatting semantics; CLI, TUI, Capsule, FFI, and Swift adapt only layout and never infer usage meaning.
- Provider-only display order is OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax, then host-only OpenCode. Internal adapter/agent identifiers remain code-only except when Capsule must separately identify agent lifecycle.
- Overview/sidebar summaries use one Rust-ranked real limit, never an aggregate. Canonical accounts sort by full display label using locale-aware case-insensitive comparison and stable account ID as tie-breaker.
- External provider touchpoints and currently demonstrated alternatives are enumerated in Research below. Browser-cookie and authenticated-dashboard scraping are forbidden; only supported API/CLI, explicit Keychain, and environment/config credential lanes are eligible.

### Planning-owned technical closure

The following are technical contracts for `tailrocks-plan` to freeze from the
linked research matrices, repository evidence, fixtures, and bounded spikes.
They are not open product decisions and must not be guessed during execution:

- canonical JSON schema/version, fields, nesting, enum representation, ordering,
  structured failures, and pre-1.0 evolution;
- stable account identity construction, alias/collision/merge precedence,
  discovery-scope precedence, history enrichment, retention, and sanitization;
- broker activation/lifetime, lease and crash recovery, storage and atomic
  generation publication, TTL/staleness, deadlines, cancellation, retry/backoff,
  rate-limit behavior, force/manual refresh, and precise adaptive-activity
  thresholds under the settled 2/5/15/30-minute policy;
- exact Rust rounding, countdown, money-cap, missing-plan, lifecycle mapping,
  severity/ranking, worst-provider, run-out confidence, and `Not started`
  transition algorithms, each fixture-locked before consumers change;
- exact retained instance CLI grammar and removal/redefinition of every cache,
  snapshot, `--no-refresh`, and `--sync-host-cache` bypass;
- terminal wrapping, Unicode/plaintext fallback, localization, focus/scroll
  breakpoints, status-line lifetime, and error aggregation consistent with the
  confirmed screen grammar;
- production release ownership, signing identities by location/type only,
  notarization, artifact/version channel, Homebrew cask update, and executable
  acceptance commands.

If evidence cannot determine one of these without changing a settled product
decision, planning must return that contradiction to shaping; otherwise planning
selects and records the technically correct contract without asking the user.

## References

- [`crates/jackin-capsule/`](../../crates/jackin-capsule/) — existing capsule usage experience named as the console TUI reference.
- [`native/`](../../native/) — native macOS application surface.

## Native design preparation

The blessed runnable dark-only prototype is the authoritative visual and interaction
reference. Adaptation rules live in
[production mapping](../../native/Design/Prototypes/UnifiedAgentUsage/PRODUCTION_MAPPING.md);
human approval remains tracked separately in
[prototype signoff](../../native/Design/Prototypes/UnifiedAgentUsage/SIGNOFF.md).
Fixture/store/harness code is never copied into production.

- [Experience brief](../../native/Design/UnifiedAgentUsage/ExperienceBrief.md) —
  archetype, jobs, hierarchy, actions, window model, accessibility, and release
  acceptance contract.
- [Native component map](../../native/Design/UnifiedAgentUsage/NativeComponentMap.md)
  — system-owned component choices, region classifications, interaction
  contracts, and explicit custom-component absence.
- [Structural alternatives](../../native/Design/UnifiedAgentUsage/Alternatives.md)
  — eligible A, B, and G Usage-window directions, optional H popover remix,
  rejected counter-directions, and the recorded human selection of A without H
  (2026-08-20).
- [Anti-reference corpus](../../native/Design/UnifiedAgentUsage/AntiReferences.md)
  — explicit rejected states, failure reasons, corrections, and learned rules;
  pending eligible directions remain human-owned.
- [Canonical fixture matrix](../../native/Design/UnifiedAgentUsage/Fixtures.md) —
  deterministic F00–F24 scenario/task definitions, future prototype subscenario
  IDs and launch contract, status-item projections, and live/post-signoff
  coverage.
- [Prototype handoff](../../native/Design/UnifiedAgentUsage/PrototypeHandoff.md)
  — exact selection preconditions, skill invocation, revision ledger, package,
  live blessing, `SIGNOFF.md`, `Regions.md`, and post-signoff QA gates.
- [Legacy baseline visual QA](../../native/Design/UnifiedAgentUsage/BaselineVisualQA.md)
  — running-app evidence, Increased Contrast hard failure, missing automation,
  restoration proof, and states still requiring final QA.
- [Swift project readiness audit](../../native/Design/UnifiedAgentUsage/SwiftProjectReadiness.md)
  — generation, toolchain, CI, test, signing, binding, and Rust-core remediation
  inputs.
- [Swift best-practices review](../../native/Design/UnifiedAgentUsage/SwiftBestPracticesReview.md)
  — concurrency, ownership, typed boundary, adaptive sizing, accessibility,
  AppKit, localization, and failure-path remediation inputs.

Human structural selection and prototype blessing are recorded (Alexey
Zhokhov, 2026-08-20): alternative A without H, with the complete dark-only
operator matrix passed in `SIGNOFF.md`. Production remains unimplemented;
The separate delivery plan remains unclaimed.

## Research

- [Agent usage platform research](../../research/agent-usage-platform/README.md) — vetted architecture, Apple-native, reference-implementation, cache-authority, identity, projection, and delivery evidence.
- [Research verification ledger](../../research/agent-usage-platform/05-verification-ledger.md)
  — exact source searches, test commands/results, expected assertions, and
  explicit zero-test/unavailable proof gaps.
- A static code-path trace found that Capsule provider work is correctly restricted to launch-forwarded capabilities derived from the resolved workspace, role, profiles, and credential environment ([relay capability construction](../../crates/jackin-runtime/src/usage_relay.rs#L189-L215), [exact scope filtering](../../crates/jackin-runtime/src/usage_relay.rs#L385-L419)). Confidence: HIGH.
- The same trace found that the Capsule usage dialog still displays all seven provider tabs rather than filtering its display by those capabilities ([fixed provider tabs](../../crates/jackin-usage/src/usage/view.rs#L470-L489)); unavailable tabs fail closed at the relay instead of disappearing. Confidence: HIGH.
- The target contract removes those fixed tabs: Capsule presentation membership
  must equal the current fully resolved instance launch configuration, with no
  global, unresolved, or capability-only rows.
- The host runtime already exposes a canonical deduplicated account inventory and atomic grouped provider/account projection ([canonical identity](../../crates/jackin-usage/src/host/accounts.rs#L19-L57), [inventory and projection](../../crates/jackin-usage/src/host.rs#L1163-L1303)); the current CLI instead renders raw account-window rows and can duplicate one account across sources or windows ([cache identity](../../crates/jackin/src/cli/usage/store.rs#L75-L93), [flat rendering](../../crates/jackin/src/cli/usage.rs#L403-L424)). Confidence: HIGH.
- Host usage has eight surfaces, including OpenCode, while jackin❯ desktop has a frozen seven-provider catalog that excludes OpenCode ([host and desktop surface sets](../../crates/jackin-usage/src/host.rs#L54-L98)). Confidence: HIGH.
- `jackin console` currently has no usage route, state, component, or effect, but its workspace screen already establishes a left-list/right-detail navigation pattern ([current routes](../../crates/jackin-console/src/tui/model/stage.rs#L11-L36), [workspace split layout](../../crates/jackin-console/src/tui/screens/workspaces/view.rs#L105-L175)). Confidence: HIGH.
- jackin❯ desktop already ships native status items and a popover, a retained two-pane Usage window with Overview and provider/account detail, and Settings; system AppKit and SwiftUI controls own Liquid Glass rather than explicit glass effects ([window host](../../native/Sources/JackinDesktop/UsageWindowController.swift#L41-L80), [overview table](../../native/Sources/JackinDesktop/UsageWindow/OverviewListView.swift#L33-L108), [Liquid Glass enforcement](../../native/Tests/JackinUsageBridgeTests/ArchitectureTests.swift#L81-L104)). Confidence: HIGH.
- Desktop, `jackin usage host snapshot`, and the Capsule relay converge on one broker generation for the same data directory and canonical account capability; even forced callers join active work rather than starting a parallel provider call ([active-generation join](../../crates/jackin-usage/src/coordinator.rs#L281-L303), [CLI broker path](../../crates/jackin/src/cli/usage.rs#L228-L255), [Capsule relay path](../../crates/jackin-runtime/src/usage_relay.rs#L497-L563)). Confidence: HIGH.
- The single-authority invariant is not complete: `jackin usage host snapshot --no-refresh` bypasses broker state, Capsule can queue a forced manual refresh behind active work, anonymous ordinal identities can fragment one real account into parallel capabilities, broker leadership dies with its first owning process, and `jackin-capsule usage claude-cli` probes directly ([no-refresh path](../../crates/jackin/src/cli/usage.rs#L223-L260), [queued Capsule refresh](../../crates/jackin-capsule/src/daemon/multiplexer_utils.rs#L243-L281), [anonymous capability identity](../../crates/jackin-usage/src/host/discovery.rs#L811-L842), [broker process ownership](../../crates/jackin-usage/src/host/broker.rs#L507-L576), [direct diagnostic probe](../../crates/jackin-usage/src/usage.rs#L1148-L1169)). Confidence: HIGH for the mechanisms; runtime occurrence of duplicate anonymous sources remains MEDIUM.
- Instance-scoped `accounts` and `verify` read the Capsule daemon's current local projection and do not themselves start provider work; `--sync-host-cache` copies those rows into a separate SQLite projection rather than the broker's durable account state ([instance read path](../../crates/jackin/src/cli/usage.rs#L335-L400), [projection store](../../crates/jackin/src/cli/usage/store.rs#L15-L42)). Confidence: HIGH.
- Reference-implementation survey 2026-08-20 ([CodexBar provider catalog](https://github.com/steipete/CodexBar/blob/main/docs/providers.md), [OpenUsage provider docs](https://github.com/robinebers/openusage/tree/main/docs/providers), local clones at `~/Projects/github/{CodexBar,openusage}`): both apps converge on our window model (used %, reset timestamp, window duration, plan, account); their edge over the crate is provider-specific extra windows, simpler probe sources for two providers, and adaptive refresh. All cost/spend/trend surfaces they ship are excluded by the limits-only Must-not and were not catalogued as gaps.
- Codex: the `/wham/usage` payload the crate already parses also carries `individual_limit` / `spend_control.individual_limit` (`{limit, used, remaining_percent, resets_at}`), a monthly team/enterprise credit-pool lane CodexBar surfaces as a money-cap window; the crate does not extract it ([codex adapter](../../crates/jackin-usage/src/usage/codex.rs)). Quota-bound, so allowed under the limits-only rule. Confidence: HIGH.
- Claude: CodexBar parses OAuth window keys beyond the crate's set (`seven_day_oauth_apps`, `iguana_necktie`, multi-key routines/cowork aliases) ([ClaudeOAuthUsageFetcher](https://github.com/steipete/CodexBar/tree/main/Sources/CodexBarCore/Providers/Claude)); unknown plain `{utilization, resets_at}` windows currently fall through unless they carry dollar budgets. Confidence: MEDIUM.
- OpenUsage classifies Codex and Z.AI windows by window *duration* rather than response slot names, surviving provider renames of `primary_window`/`secondary_window`; the crate's Codex adapter keys on slot names. Confidence: HIGH for the mechanism.
- Grok: OpenUsage reads billing through plain REST (`GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`, `…/v1/settings`) where the crate spawns `grok agent stdio` ACP RPC and falls back to a grpc-web protobuf wire-scan ([grok adapter](../../crates/jackin-usage/src/usage/grok.rs)). Confidence: MEDIUM.
- OpenCode: the settled eight-surface host inventory includes OpenCode, but the crate ships usage adapters only for Codex, Claude, Amp, Grok, Z.AI, Kimi, and MiniMax. OpenUsage demonstrates a working limits API: `GET https://opencode.ai/zen/go/v1/usage` with the `opencode-go` Bearer key from `~/.local/share/opencode/auth.json`, returning rolling/weekly/monthly percents with `resetsAt` ([OpenUsage OpenCode docs](https://github.com/robinebers/openusage/tree/main/docs/providers)). Confidence: HIGH.
- Z.AI: OpenUsage additionally calls `GET https://api.z.ai/api/biz/subscription/list` for plan detection and classifies `CREDIT_LIMIT` entries by window duration; the crate reads plan fields from the quota response only and handles `TOKENS_LIMIT`/`TIME_LIMIT` ([zai adapter](../../crates/jackin-usage/src/usage/zai.rs)). Confidence: MEDIUM.
- MiniMax: the F22 fixture's money-cap window ("Monthly credit allowance — $6 available of $20 cap") has no crate support; `minimax.rs` extracts percent/count windows and plan titles only, no money fields ([minimax adapter](../../crates/jackin-usage/src/usage/minimax.rs)). Confidence: HIGH.
- CodexBar scales refresh cadence by interaction recency and power state (2 min after menu interaction → 5 min warm → 15 min idle → 30 min long-idle or Low Power Mode, plus a refresh on menu open); the crate's broker applies a fixed 300-second floor ([adaptive policy](https://github.com/steipete/CodexBar/blob/main/Sources/AdaptiveRefreshCore/AdaptiveRefreshPolicyCore.swift)). Confidence: HIGH.
- CodexBar derives a signed pace delta versus even burn and a "runs out" exhaustion projection from limit windows only — no cost data — where the crate's projection already carries `pace_label` but no exhaustion estimate ([pace engine](https://github.com/steipete/CodexBar/blob/main/Sources/CodexBarCore/UsagePace.swift)). Limits-only compatible. Confidence: HIGH.
- OpenUsage distinguishes an untouched 5-hour window ("Not started") from a full one; the crate renders both as 100 % remaining. Confidence: MEDIUM.
- Multi-account: OpenUsage ships none and CodexBar manages parallel `CODEX_HOME` directories / `cswap` slots; the settled canonical deduplicated account graph already exceeds both, so no gap. Confidence: HIGH.
- CodexBar's widest provider coverage relies on browser-cookie import (Keychain Safe Storage decryption, WebView dashboard scrapes); the crate's credential ladders stop at CLI files, Keychain items, and environment variables. Recorded as a boundary question, not an automatic gap.

## Must not

- MUST NOT display duplicated accounts in the console usage interface — each account should appear once across available agent configurations.
- MUST NOT show token unit prices, session cost estimates, spend-over-time history, usage trends, aggregate-spend charts, or cost rankings — the repository's root agent rules restrict usage surfaces to subscription and quota limits.
- MUST NOT let a CLI, console, desktop, Capsule, diagnostic, or presentation-cache path call a provider directly, queue a refresh behind active canonical-account work, or become an independent freshness/retry authority — one broker owns provider work.
- MUST NOT use unstable source ordinals as durable canonical account identity when they can fragment one account or alias persisted broker state.
- MUST NOT disable or block Capsule launch or session actions based on exhausted, unknown, stale, or failed quota observations.
- MUST NOT downgrade `agent_uninitialized` to a neutral/success state merely
  because a quota preview is available, or confuse that lifecycle error with a
  provider usage failure.
- MUST NOT populate Capsule presentation from the fixed provider catalog, global
  host discovery, unresolved configuration, or usage capability alone; the
  fully resolved instance launch configuration owns membership.
- MUST NOT show agent/runtime names as part of provider labels; visible provider labels are OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax, and host-only OpenCode.
- MUST NOT aggregate unlike quota windows for Overview/sidebar summaries or reorder accounts by severity, freshness, discovery source, workspace, or selection.
- MUST NOT silently substitute another account when selection disappears; return to Overview with the confirmed persistent inline notice.
- MUST NOT import/decrypt browser cookies, embed authenticated dashboard WebViews, or scrape authenticated web pages for quota data.
- MUST NOT infer `Not started` from 100% remaining or expose `Runs out in N d` on CLI, stale, untouched, or weak-confidence windows.
- MUST NOT add bars, cards, animation, pace projection, raw mode, or interactive chrome to human `jackin usage` output.
- MUST NOT copy prototype fixture, store, scenario, or harness code into production; only the blessed behavior and production ownership mapping transfer.

## Quality bar

- The console TUI satisfies the repository's TUI decisions for non-blocking rendering, visible loading/refresh state, keyboard navigation, footer hints, focus and scroll behavior, modal geometry, and shared component reuse, with render-conformance fixtures for its major states.
- The desktop app uses system-owned native components and Liquid Glass, answers “Where am I?”, “What can I do?”, and “Where can I go from here?” in every state, passes the macOS design rubric with zero hard failures, and has running-app visual evidence plus accessibility audits across required appearance and Reduce-settings states.
- Concurrent usage reads and refreshes for one canonical account reuse shared cached data and join one in-flight refresh generation instead of issuing parallel duplicate provider requests; every usage surface must be verified against this invariant and any bypass fixed before shipping.
- CLI, console, Capsule, and desktop fixtures prove identical Rust-owned labels and values for the same projection, with only surface-appropriate layout differences.
- Console render-conformance fixtures cover confirmed Overview, Account Detail, 80×24 compact rendering, loading, refreshing-with-last-good, empty, stale, partial failure, global failure, focus, scroll, and removed-selection notice using shipped `jackin console` chrome.
- CLI golden output covers current, multi-account, stale/partial failure, empty, total failure, JSON success/failure, TTY/non-TTY, and Unicode/plaintext fallback while proving stdout remains simple and machine-safe.
- Capsule render-conformance fixtures cover Overview, focused-agent entry, conditional account tabs, `agent_uninitialized` with and without preview, initialization transition, zero-agent empty, stale, resolution/refresh failure, narrow/wide, focus, scroll, and removed-selection fallback.
- Release evidence proves Developer ID signing, notarization, public artifact publication, and installation through the Homebrew cask.

## Open questions

## Researched directions awaiting implementation fixtures

- Across captured Codex and Z.AI payload fixtures, which classifier—provider slot name, provider-supplied duration, or an explicit combination—preserves correct semantic window identity under slot renames and malformed or missing durations?
- Across authenticated fixtures and failure-path probes, which supported Grok source—the `cli-chat-proxy.grok.com` REST billing endpoints, ACP stdio RPC, or grpc-web API—provides the most complete and stable quota contract without widening the credential boundary?
- Which supported OpenCode authentication and `zen/go/v1/usage` response contract supplies stable account identity, rolling/weekly/monthly limits, resets, and failure semantics required by the settled eight-surface host inventory?

Research chapter 07 closes the source and classifier directions. Plan 004 owns
authenticated and failure fixtures before shipping; these are not product questions.

## Deferred

- **Provider catalog expansion beyond the settled eight host / seven desktop surfaces.** Deferred by Alexey Zhokhov on 2026-08-21 because this roadmap must first ship and prove parity for its named catalog and canonical broker contract. Revisit after every settled provider passes cross-surface parity and a requested provider has both a supported quota API and a trusted credential lane; do not add placeholders or empty rows meanwhile.
- **Provider status-incident badges from public status pages.** Deferred by Alexey Zhokhov on 2026-08-21 because operational-health data is separate from canonical account quota truth. Revisit after unified quota usage ships, through a separate roadmap that settles authoritative status sources, freshness, outage semantics, and failure behavior; never mix status-page failures into account quota state here.
- **Per-credit Codex reset-credit expiry timelines and expiry notifications.** Deferred by Alexey Zhokhov on 2026-08-21; this roadmap retains the current available-credit count and next-expiry quota facts. Revisit after unified usage ships and provider data reliably exposes individual expiries, under a separate roadmap that settles notification timing, permission, delivery, and suppression policy.
- **Codex code-review quota window.** Deferred by Alexey Zhokhov on 2026-08-21 because its currently demonstrated source requires the excluded browser-cookie/dashboard scraping lane. Revisit when Codex exposes it through `/wham/usage`, another supported API, or an existing trusted CLI/Keychain/config credential path; omit it rather than guessing or scraping meanwhile.

## Log

- 2026-08-20 — tailrocks-idea — created (DRAFT).
- 2026-08-20 — tailrocks-brainstorm — moved to SHAPING after settling the default CLI overview behavior.
- 2026-08-20 — tailrocks-brainstorm — closed shaping session after settling shared CLI/TUI behavior and Capsule pre-session usage; remaining decisions recorded with recommendations.
- 2026-08-20 — tailrocks-research — completed and linked vetted architecture, Apple-native, reference-implementation, and delivery research.
- 2026-08-20 — tailrocks-swift-project-setup — completed the read-only native project-readiness audit.
- 2026-08-20 — tailrocks-swift-best-practices — completed the read-only Swift architecture and implementation-practices review.
- 2026-08-20 — tailrocks-macos-visual-qa — recorded the failed incumbent running-app baseline and honest missing-state matrix.
- 2026-08-20 — tailrocks-macos-design — completed the preselection brief, component map, alternatives, and deterministic fixture contract; human selection remains mandatory.
- 2026-08-20 — tailrocks-record-decision — recorded the human structural selection of Usage-window alternative A without H (Alexey Zhokhov); propagated the selection record, anti-reference rejections, brief approval, and prototype-handoff preconditions; status stays SHAPING pending prototype blessing.
- 2026-08-20 — reference survey — surveyed the CodexBar and OpenUsage clones against the `jackin-usage` crate; recorded extraction gaps (Codex spend-control lane, OpenCode adapter, Z.AI subscription endpoint, Claude window keys, Grok REST source, MiniMax money cap), display candidates (adaptive refresh, pace exhaustion, "Not started"), the cookie-scraping boundary question, and the deferred out-of-scope catalog.
- 2026-08-20 — prototype reference stabilization — made the dark-only prototype
  the explicit interaction/visual reference, documented the production ownership
  seam, added contract tests, and phased the future desktop adaptation without
  claiming human visual blessing or implementation completion.
- 2026-08-20 — prototype operator signoff — Alexey Zhokhov completed and
  blessed the dark-only matrix in `SIGNOFF.md`; roadmap status remains SHAPING
  pending `$tailrocks-finalize`.
- 2026-08-21 — tailrocks-finalize — confirmed Console, CLI, Capsule, and
  desktop screen families; resolved provider naming, summary selection,
  account ordering/recovery, adaptive refresh, quota additions, credential
  boundaries, and deferrals; fresh-context planning dry run reported no
  inventions or user questions; moved to READY.
- 2026-08-21 — tailrocks-plan --deep — froze coverage, specifications,
  broker/provider/surface research, eight sequential plans, and the goal gate;
- 2026-08-21 — Plan 003 — shipped the demand-activated sibling broker executable,
  lease/idle/retry policy, projection operations, atomic publication envelope, and
  process lifecycle proof; Plans 004–008 remain sequentially pending.
- 2026-08-21 — Plan 004 — completed provider parity: duration-first Codex and
  Z.AI classifiers, Codex individual cap, Grok CLI-proxy REST, OpenCode Go limits
  with provisional identity, provider-only labels, and sanitized fixtures.
- 2026-08-21 — Plan 005 — shipped the simple broker-backed host CLI and the
  native Console Usage route. Human output stays compact; JSON exposes the
  canonical projection. Console uses the shipped `jackin❯ · usage` frame,
  grouped provider/account navigation, account detail, and full-width
  Capsule-style meters. Focused tests, clippy, formatting, and lint pass.
- 2026-08-21 — Plan 006 — kept Capsule usage broker/relay-backed, made the
  resolved launch membership boundary explicit and deduplicated, added truthful
  zero-agent copy without Retry, and removed the direct Claude diagnostic
  bypass. Focused Capsule/runtime tests, clippy, and formatting pass.
- 2026-08-21 — Plan 007 — completed the production desktop usage surface on the
  shared sanitized projection: native status modes/popover, dark-only Usage
  window matrix, account-aware selection/recovery, centered brand, minimum-size
  envelope, accessibility variants, and native format/lint/test/CI gates pass.
- 2026-08-21 — Plan 008 — completed noncredentialed parity, authority, native,
  ad-hoc artifact, and real-app UI proof (19/19 UI tests). Rejected only the
  external release segment because the operator prohibited merge/tag/
  publication, signing/notarization, and Homebrew-tap mutation; PR #898 remains
  open and unmerged.
  moved to PLANNED. All implementation remains on the current branch and PR #898.
- 2026-08-21 — Plan 001 — froze secret-free V1 contract fixtures, the surface-state
  matrix, classified provider-call inventory with injection detection, reserved test
  ownership, and passing repository format/lint/test baselines. PR #898 remains open.
- 2026-08-21 — Plan 002 — shipped the additive canonical projection V1, typed
  provider/account evidence, stable capability aliases, current-discovery membership,
  provider-only identity, ICU4X Rust ranks, immutable publication reads, typed quota
  categories, and removed-selection normalization. Full workspace and OrbStack E2E
  gates pass; PR #898 remains open and unmerged.
