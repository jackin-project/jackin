# Design Fixtures — Unified Agent Usage

Status: DRAFT

These records are the canonical successor contract shared by every schematic
alternative and later prototype. F00–F14 reuse the incumbent catalog's stable
scenario IDs, but `VisualQAFixtures.swift`, `VisualQALaunchOptions.swift`, and
their bespoke `--fixture`/`--window-size` flags are legacy baseline input and do
not implement this contract. After human structural selection, create the
separate committed SwiftPM package
`native/Design/Prototypes/UnifiedAgentUsage/`; do not retrofit the incumbent app
into the design prototype. Its fixture source consumes every record in this
file, and its `default` scenario is an exact alias to F02. Its harness implements
only the standard five arguments:
`--tr-scenario`, `--tr-appearance`, `--tr-window`, `--tr-reduce`, and
`--tr-backdrop`; unknown scenario names and invalid sizes fail at launch.
Until sign-off and later baseline capture, current-app captures remain legacy
evidence only. Rust-owned strings are immutable display input; a prototype may
change layout only.

The complete precondition, revision, package, `SIGNOFF.md`, and `Regions.md`
sequence is frozen in [PrototypeHandoff.md](PrototypeHandoff.md).

Frozen environment:

- Time: `2026-08-12T12:00:00+07:00`
- Base locale and layout direction: `en_US`, left-to-right. F19 overrides both.
- Calendar: Gregorian
- Time zone: `Asia/Ho_Chi_Minh`
- Window sizes: 800 × 520 minimum, 1000 × 680 typical, 1200 × 760 wide
- Appearance: dark only
- Every quota window carries explicit semantic category metadata:
  `longRange`, `model`, `general`, `session`, or `other`. The prototype sorts
  by category and preserves fixture order within a category. Production Rust
  emits final order; Swift never infers category from display labels.
- Popover: 380 × 520

## Frozen desktop provider order

1. OpenAI / Codex
2. Anthropic / Claude
3. Amp
4. xAI / Grok
5. Z.AI / GLM
6. Kimi
7. MiniMax

OpenCode is intentionally absent from jackin❯ desktop while remaining present
in host CLI and console fixtures. The native subtree contract freezes this order;
human structural selection does not reopen it. The host order remains the
separately settled eight-provider order.

## Core records

### OpenAI / Codex

Provider state: current.
Provider summary: `57% left`, `Resets in 3d`.

Canonical accounts:

| Key | Account | Plan | Remaining | Selected | State |
|---|---|---|---:|---|---|
| `codex-personal` | `personal@example.test` | Plus | 57% | default in F02 | current |
| `codex-plus` | `team@example.test` | Plus | 0% | selected in F03/F05 | depleted |
| `codex-organization` | `organization-production-sandbox@example.test` | Enterprise | 88% | optional | current |

Quota windows for `codex-personal`:

| Stable row | Label | Display | Meter | State |
|---|---|---|---:|---|
| `bucket:weekly` | Weekly | `57% left · Resets in 3d` | 57 | warning |
| `bucket:five-hour` | Five-hour | `63% left · Resets in 2h` | 63 | normal |
| `bucket:credits` | Credits | `3 manual resets available · Next expires in 3d 4h` | — | normal |

Quota windows for `codex-plus`:

| Stable row | Label | Display | Meter | State |
|---|---|---|---:|---|
| `bucket:weekly` | Weekly | `0% left · Resets in 3d` | 0 | depleted |

### Anthropic / Claude

Provider state: current or nearly depleted.
Provider summary: `12% left`, `Resets in 1h`.

Canonical account: `personal@example.test`, Pro plan, 12% remaining.

Quota windows:

| Stable row | Label | Display | Meter | State |
|---|---|---|---:|---|
| `bucket:session` | Session | `74% left` | 74 | normal |
| `bucket:weekly` | Weekly | `12% left · Resets in 1h` | 12 | danger |

### Remaining normal catalog

| Provider | Account label | Remaining | Reset | State |
|---|---|---:|---|---|
| Amp | `default` | 100% | `Resets in 18h` | current |
| xAI / Grok | `default` | 72% | unavailable | current |
| Z.AI / GLM | `default` | 81% | unavailable | current |
| Kimi | `default` | 45% | unavailable | current |
| MiniMax | `default` | 33% | unavailable | current |

Missing reset values display the Rust-owned fallback; they never become zero or
an inferred date.

## Required fixture matrix

### F00 — No providers

- Providers: none.
- Accounts: none.
- State: not loading, no global error.
- Required copy: “No providers detected” plus a concrete next step.
- Required controls: Settings/configuration route if available; no fake Refresh
  loop against an empty capability set.

### F01 — Single normal

- Provider: OpenAI / Codex.
- Account: `codex-personal`.
- Summary: 57% left, reset in 3 days.
- Detail: all OpenAI quota windows above.
- Purpose: prove calm single-account hierarchy without empty columns or unused
  account chrome.

### F02 — Full normal catalog

- Seven desktop providers in canonical order.
- One account each.
- All values from the normal catalog.
- Purpose: typical dark, inactive-window, sidebar, popover, and overview
  evidence.

### F03 — Multi-account provider

- Provider: OpenAI / Codex only.
- Accounts: personal 57%, team 0%, organization 88%.
- Selected account: team / `codex-plus`.
- Selected detail: exhausted weekly window.
- Purpose: prove deduplication, account selection, stable IDs, and exact handoff
  from popover to Usage.

### F04 — Nearly exhausted

- Provider: Anthropic / Claude.
- Account: personal / Pro.
- Remaining: 12%, reset in 1 hour.
- Purpose: warning remains legible without color and does not disable launch or
  navigation.

### F05 — Exhausted

- Provider: OpenAI / Codex.
- Account: team / Plus.
- Remaining: 0%, reset in 3 days.
- Purpose: depleted is informational, explicit, and never a disabled-state
  substitute.

### F06 — Stale last-good

- Provider: OpenAI / Codex.
- Account: personal, 57% last-good.
- Age: `Updated 47m ago`.
- Error: `Codex provider usage unavailable; cached quota is stale`.
- Purpose: preserve usable values, label stale state, and place Retry locally.

### F07 — Refreshing last-good

- Base: full normal catalog.
- OpenAI generation: refreshing.
- Last-good values remain visible.
- Purpose: busy state never erases data, shifts layout, or blocks other
  navigation; repeated Refresh joins existing work.

### F08 — Partial provider timeout

- Base: full normal catalog.
- Kimi state: unavailable.
- Error: `usage provider probe timed out`.
- Other six providers: usable current rows.
- Purpose: provider-local failure, structured partial success, global command
  remains successful.

### F09 — Permission denied

- Provider: Anthropic / Claude.
- Accounts: none usable.
- State: unavailable.
- Error: `Claude Keychain access denied`.
- Purpose: explicit permission state and recovery without a modal alert or
  leaked credential path/value.

### F10 — Offline cached

- Provider: Kimi.
- Account: default, stale last-good 45%.
- Age: `Updated 1h ago`.
- Error: `Kimi billing endpoint unavailable; local presence only`.
- Purpose: offline and stale remain distinguishable from permission failure and
  empty inventory.

### F11 — Long labels

- Provider: `OpenAI Organization Production Sandbox — Southeast Asia`.
- Account: `organization-production-sandbox@example.test`.
- Plan: `Enterprise workspace with centrally managed weekly limits`.
- Window: `Organization-wide weekly accelerated-model allocation`.
- Value: `57% left`.
- Reset: `Resets Tuesday, 18 August 2026 at 23:59 Indochina Time`.
- Error: `Provider response could not be refreshed; showing the last successful quota snapshot`.
- State: stale.
- Purpose: 800 × 520 wrapping/truncation, complete accessibility text, and no
  overlapping columns.

### F12 — Layout envelope / large dataset

- Exactly 42 canonical accounts: six under each desktop provider in frozen order.
- Surface key `S` is exactly one of `codex`, `claude`, `amp`, `grok`, `zai`,
  `kimi`, or `minimax`, in that order.
- For provider surface `S` and one-based account index `NN`, stable account ID is
  `S-load-NN` and label is `S-NN@example.test`, except
  `claude-load-03`, whose label is exactly `Research workspace`.
- Plan labels by account index 01–06 are exactly `Free`, `Plus`, `Pro`, `Team`,
  `Enterprise`, and `Default`.
- Remaining values follow the exact global account-order cycle
  `[88, missing, 28, 0, 12, 57, 100]`; index zero is `codex-load-01`, then
  account index advances before provider index.
- Selected provider/account: Anthropic / Claude, `claude-load-03` /
  `Research workspace`.
- Every account has exactly eight windows with stable IDs and labels:
  `limit-01` / `Hourly`, `limit-02` / `Daily`, `limit-03` / `Daily`,
  `limit-04` / `Weekly`, `limit-05` / `Monthly`, `limit-06` / `Model`,
  `limit-07` / `Organization`, and `limit-08` / `Credits`. Duplicate `Daily`
  labels intentionally retain distinct IDs.
- Window remaining values rotate the same seven-value cycle by
  `(globalAccountIndex + windowIndex) mod 7`, with both indices zero-based.
  Reset labels by zero-based window index 0–7 are exactly `Resets in 1h`,
  `Resets in 6h`, `Resets in 18h`, `Resets in 3d`, `Resets Sep 1`,
  `Reset unavailable`, `Resets Tuesday 23:59`, and `No reset supplied`.
- The committed Rust-produced status-bar projection is exactly
  `[claude, codex, amp]`; the fixture loader does not recompute it.
- Purpose: minimum/typical/wide geometry, native scrolling, disclosure stability,
  selection survival, and deterministic ordering.

### F13 — Initial loading

- Providers/accounts: none yet.
- State: loading true, no error.
- Purpose: reserved layout with native indeterminate progress; no blank window,
  disabled app, or shifting controls.

### F14 — Global bridge error

- Providers/accounts: no usable projection.
- Error: `Usage presentation is unavailable`.
- Purpose: `ContentUnavailableView`, one Retry, and normal access to Settings and
  Quit.

### F15 — Accepted preference mutation

- Base projection: F02.
- Before phase: percent style `left`; status rows carry the F02 Rust-owned
  remaining strings.
- Mutation: percent style changes from `left` to `used`.
- After phase: percent style `used`; the accepted Rust projection supplies all
  used-percent strings while preserving F02 status-row membership and order.
- Rust accepts mutation and returns the next projection.
- Purpose: values change only from Rust-supplied strings; selected setting and
  all surfaces update together.

### F16 — Rejected preference mutation

- Base projection: F02.
- Setting: refresh floor changes from 5 to 1 minute.
- Rust rejects the operation with a typed recoverable error.
- Expected: control returns to accepted 5-minute value; contextual message and
  exact Retry remain beside Refresh settings.
- Purpose: prevent silent optimistic persistence or invisible global error.

### F17 — Reordered mutation completion

- Base projection: F02; accepted refresh floor starts at 5 minutes.
- Mutation A requests 10 minutes and receives intent revision 41. Mutation B
  immediately requests 15 minutes and receives revision 42. B completes first;
  A completes last.
- Expected: 15 minutes and revision 42 remain accepted in Rust projection,
  Settings, and persistence; A cannot overwrite newer intent.
- Purpose: prove task ownership, generation ordering, and shutdown guards.

### F18 — Accessibility display settings

- Data: F02 and F11.
- Prototype process-local reductions for each data subscenario are exactly:
  no `--tr-reduce` argument, `--tr-reduce transparency`, `--tr-reduce motion`,
  and `--tr-reduce transparency,motion`.
- Post-signoff real-settings visual QA adds Increase Contrast, Differentiate
  Without Color, Full Keyboard Access, dark, and key/inactive window while
  preserving snapshot-and-restore evidence.
- Expected: opacity adapts through system material; all rows remain separated;
  every state has non-color identity; focus remains visible; no spatial/blur
  animation survives Reduce Motion.

### F19 — Localization and direction

- Base projection: F02. The following literal tuples are fixture input; provider
  strings remain Rust-owned and Swift-owned chrome comes from the test catalog.
- `en_US`, left-to-right, 2× expansion: provider
  `OpenAI Organization Production Sandbox — Southeast Asia`; account
  `organization-production-sandbox@example.test`; plan
  `Enterprise workspace with centrally managed weekly limits`; reset
  `Resets Tuesday, 18 August 2026 at 23:59 Indochina Time`; error
  `Provider response could not be refreshed; showing the last successful quota snapshot`;
  actions `Refresh Refresh` and `Open Usage Open Usage`.
- `ar_SA`, right-to-left: provider `أوبن إيه آي`; account
  `team-01@example.test`; plan `فريق`; reset `تتم إعادة الضبط خلال ٣ أيام`;
  error `تعذّر تحديث الاستخدام؛ تظهر آخر لقطة ناجحة`; actions `تحديث` and
  `فتح الاستخدام`.
- `ja_JP`, left-to-right: provider `OpenAI`; account `研究チーム@example.test`;
  plan `エンタープライズ`; reset `8月18日火曜日 23:59にリセット`;
  error `使用量を更新できないため、最後に成功した値を表示しています`;
  actions `更新` and `使用状況を開く`.
- `de_DE`, left-to-right: provider `OpenAI`; account
  `forschung@example.test`; plan `Unternehmen`; reset
  `Zurücksetzung am Dienstag, 18. August 2026 um 23:59 Uhr`; error
  `Schlüsselbundzugriff verweigert`; actions `Aktualisieren` and
  `Nutzung öffnen`.
- Expected: system mirroring, no clipped primary action/identity, locale-safe
  value grouping, complete accessibility summaries.

### F20 — Destructive pending sentinel

- Base projection: F02.
- No destructive action exists in the usage experience.
- Expected: no confirmation dialog, destructive tint, Delete/Remove/Buy action,
  or quota-based launch disablement appears.
- Purpose: keep future implementations inside the informational product boundary.

### F21 — Keyboard and VoiceOver task completion

- Starting projection: F03 inventory with `codex-personal` initially selected
  and a single `[codex]` status-bar projection.
- Starting point: provider status item focused through the macOS menu bar.
- Sequence: open the popover; hear provider/account/value/reset/state; move
  through account picker, Refresh, and Open Usage; select `codex-plus`; open
  Usage; confirm the same account; traverse provider group, account row, quota
  windows, then move to the F06 `codex-personal` stale/error detail and Retry;
  close Usage and dismiss the popover.
- Async event: Retry creates generation 44, retaining the exact F06 last-good row
  while refreshing; generation 45 then replaces the provider inventory with exact
  F01 current data. The removed `codex-plus` selection reconciles to the sole
  `codex-personal` account. The announcement is concise and does not restart the
  entire table.
- Expected: no anonymous groups, duplicate row summaries, focus trap, pointer-only
  action, or lost focus. Escape returns focus to the originating status item;
  reopening restores the accepted selection. Status-item reconciliation is
  deferred while its popover owns focus, so selecting depleted `codex-plus`
  cannot destroy the originating control before focus returns. Generation 45
  retains `[codex]` as the terminal status-bar membership.

### F22 — Provider-supplied money cap

- Provider: MiniMax.
- Account: default / Pro.
- Window stable ID: `bucket:monthly-credit-cap`.
- Label: `Monthly credit allowance`.
- Display: `$6 available of $20 cap · Resets Sep 1`.
- Purpose: prove a provider-supplied money cap can be presented as a quota bound
  without token prices, cost estimates, spend history, charts, ranking, or an
  inferred amount spent.

### F23 — Physical display and restoration

- Base projection: F03 inventory with `codex-personal` initially selected and
  `[codex]` as the committed status-bar projection.
- Displays: built-in 2× plus external 1× and external 2×, each tested with and
  without its own menu bar where the system permits.
- Sequence: open from each clicked status item; verify popover anchoring; move
  Usage between displays; resize; hide/show sidebar; select a provider/account;
  close/reopen; disconnect the last display; relaunch.
- Expected: the unique Usage window stays fully visible, restores safe geometry
  and selection, and never opens on a removed display. Popover remains anchored
  to the clicked item rather than app-owned coordinates.

### F24 — Continuous resize and overflow

- Sweep the Usage window continuously from 1200 × 760 to 800 × 520 and back,
  including 900 and 860-point candidate thresholds.
- Live prototype subscenarios use F02, F11, and F12 with sidebar shown/hidden and
  toolbar items forced into overflow. Post-signoff visual QA repeats each under
  the real Increase Contrast setting with restoration proof.
- Expected: no overlapping or concatenated text, horizontal scroll for the
  primary job, hidden focus, selection loss, oscillating layout, or inaccessible
  action. Every toolbar action remains available in its menu.

### F25 — Multi-account rich overview

- Providers: Codex with three canonical accounts (personal Plus 57 % warning,
  team Plus depleted, organization Enterprise 88 % with a monthly credit-pool
  money cap), Claude with two accounts (personal Pro 12 % danger, work Team
  91 % with a not-started session window).
- Status rows: `[claude, codex]`.
- Purpose: prove the Overview cards and every detail surface render every
  canonical account per provider without collapsing duplicates, across plan
  tiers, states, and window mixes.

### F26 — Needs login

- Provider: Claude; state `needsLogin`; no accounts; error
  `Claude sign-in expired — sign in again to resume quota updates`.
- Purpose: expired/revoked credential presentation — distinct from a probe
  failure, with last-updated provenance.

### F27 — Needs secret

- Provider: Z.AI / GLM; state `needsSecret`; no accounts; error
  `No Z.AI API key found — set ZAI_API_KEY to enable quota tracking`.
- Purpose: key-only provider with no discovered credential anywhere.

### F28 — Unsupported credential

- Provider: Codex; state `unsupported`; no accounts; error
  `OpenAI API-key subscription quota is unavailable`.
- Purpose: presence-only credential that exposes no quota surface.

### F29 — Rate limited with backoff

- Provider: Grok; state `rateLimited`; last-good account rows stay visible;
  error `Grok billing endpoint rate limited · Retry in 12m`.
- Purpose: provider 429 with a Retry-After deadline — honest backoff marker
  over preserved last-good data, never a silent gap.

## Status-item projection contract

Unless a row below overrides it, Settings use percent style `left`, reset style
`countdown`, strip mode with maximum three, and no pinned surface. The Rust
`statusBarGlanceRows` list is committed fixture input, never Swift-ranked. The
visible list is the exact result after applying the named presentation mode.

| Fixture | Rust `statusBarGlanceRows` | Mode/settings | Visible surface IDs |
|---|---|---|---|
| F00 | `[]` | icon only | `[]` |
| F01 | `[codex]` | pinned, `codex` | `[codex]` |
| F02 | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F03 | `[]` | icon only | `[]` |
| F04 | `[claude]` | worst provider | `[claude]` |
| F05 | `[]` | worst provider | `[]` |
| F06 | `[codex]` | pinned, `codex` | `[codex]` |
| F07 | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F08 | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F09 | `[]` | pinned, `claude` | `[]` |
| F10 | `[kimi]` | pinned, `kimi` | `[kimi]` |
| F11 | `[codex]` | worst provider | `[codex]` |
| F12 | `[claude, codex, amp]` | strip, max 3 | `[claude, codex, amp]` |
| F13 | `[]` | icon only | `[]` |
| F14 | `[]` | icon only | `[]` |
| F15-before | `[claude, amp, codex]` | strip, max 3, `left` | `[claude, amp, codex]` |
| F15-after | `[claude, amp, codex]` | strip, max 3, `used` | `[claude, amp, codex]` |
| F16 | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F17 | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F18-f02 | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F18-f11 | `[codex]` | worst provider | `[codex]` |
| F19-en-US | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F19-ar-SA | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F19-ja-JP | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F19-de-DE | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F20 | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F21 | `[codex]` throughout | pinned `codex`; deferred reconcile | `[codex]` |
| F22 | `[minimax]` | pinned, `minimax` | `[minimax]` |
| F23 | `[codex]` | pinned, `codex` | `[codex]` |
| F24-f02 | `[claude, amp, codex]` | strip, max 3 | `[claude, amp, codex]` |
| F24-f11 | `[codex]` | worst provider | `[codex]` |
| F24-f12 | `[claude, codex, amp]` | strip, max 3 | `[claude, codex, amp]` |

## Executable scenario IDs

Scenario names are case-sensitive. The harness accepts exactly `F00` through
`F17`, `F20` through `F23`, `F18-f02`, `F18-f11`, `F19-en-US`, `F19-ar-SA`,
`F19-ja-JP`, `F19-de-DE`, `F24-f02`, `F24-f11`, `F24-f12`, and `default`.
`default` resolves byte-for-byte to F02. F18, F19, and F24 are documentation
matrix headings, not launchable aliases; an attempt to launch them fails loudly.
F15 is one scenario with deterministic before/after phases, not two launch IDs.
No locale, data-base, or status-row choice comes from an undeclared argument.

## Prototype walkthrough and later capture coverage

Preselection ASCII schematics use only exact core or named-fixture records;
OpenAI multi-account examples use F03 tuples. After human structural selection,
the user walks every executable scenario listed above, including the `default`
alias. Every scenario that opens Usage runs at exactly 800 × 520, 1000 × 680, and
1200 × 760 in dark only; every scenario that opens the popover also
runs at its fixed 380 × 520 in the dark appearance. F18 repeats that matrix for
`F18-f02` and `F18-f11` with the four exact process-local reduction settings.
F19 runs all four locale/direction subscenarios. F23 and every F24 subscenario
complete their full live interaction sequences at each relevant size/display
state. This walkthrough uses the running material and produces no screenshots.
`SIGNOFF.md` must enumerate each scenario/appearance/size result and record the
user's approval before the design can advance.

Only after recorded sign-off does `tailrocks-macos-visual-qa` drive the
prototype through the same five-flag launch contract, freeze baseline captures,
exercise the real accessibility-settings matrix, and apply the region-aware
match policy. No fixture is deferred from the live blessing gate to final visual
QA; the later lane adds durable evidence rather than substituting for review.
