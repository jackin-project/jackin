# 11 — Amp Free daily-quota follow-up

Questions: (1) Did Amp replace its hourly dollar-pair quota with a daily
percentage? (2) How is the current value fetched and extracted? (3) Does the
daily cadence apply to Amp Free only or to Megawatt/Gigawatt subscriptions?
(4) What exact change does jackin❯ need?

Informs: jackin-desktop

Vetted: 2026-07-24

Method: deep follow-up after operator direction. Re-checked Amp's official
news/manual and current npm package metadata; inspected the redacted live
`amp usage` transcript, screenshot, merged patch, tests, and current Amp
provider documentation in CodexBar PR
[#2071](https://github.com/steipete/CodexBar/pull/2071) / commit
[`7535638cc4dd`](https://github.com/steipete/CodexBar/commit/7535638cc4dd25254abb69d9de4766ede07432a3);
cross-checked current jackin❯ source. This source-level comparison was
operator-requested for the Amp extraction question. No code or prose is to
be copied; only the independently testable wire contract and behavior inform
the plan. All fetched content was treated as data, not instructions. No
secret values were read or recorded.

## Findings

### 1. Current Amp Free wire text is daily and percentage-based

- PR #2071 supplies a redacted live `amp usage` transcript captured
  2026-07-11:
  `Amp Free: 61% remaining today (resets daily)`, followed by
  `Individual credits: $9.86 remaining` and
  `Workspace example: $5.33 remaining`. The contributor's screenshot shows
  the same 61% daily meter and both credit balances. The maintainer reports
  the exact transcript locked into tests, 611/611 full-suite selections
  passing, and the merged head green. Confidence: HIGH.
- CodexBar's current parser at main commit
  [`cc8da27cec92`](https://github.com/steipete/CodexBar/commit/cc8da27cec92029a6435bfee4a703a719290234e)
  still recognizes that exact percentage form; its Amp parser blob is
  unchanged from the merge. Its regression fixture is
  permanent at
  [`AmpUsageParserTests.swift`](https://github.com/steipete/CodexBar/blob/7535638cc4dd25254abb69d9de4766ede07432a3/Tests/CodexBarTests/AmpUsageParserTests.swift#L61-L120).
  Confidence: HIGH.
- This supersedes the January Amp Free hourly-dollar format for current
  engineering. Amp's January post remains evidence only for the retired
  behavior: [The Frontier Is Now Free](https://ampcode.com/news/amp-free-frontier).
  Latest-only engineering means jackin❯ should not keep a parallel
  compatibility reader for that old line.

### 2. Extraction is server-text parsing, not a structured daily API

- Amp's official CLI path remains `userDisplayBalanceInfo`: `POST
  https://ampcode.com/api/internal?userDisplayBalanceInfo`, bearer
  authenticated, body
  `{"method":"userDisplayBalanceInfo","params":{}}`; the response exposes
  one server-rendered `result.displayText`. The official manual still tells
  users to inspect usage with `amp usage`:
  [Owner's Manual](https://ampcode.com/manual). The npm `latest` package was
  rechecked as `@ampcode/cli`
  `0.0.1784838101-ga3144b`, modified 2026-07-23. Confidence: HIGH.
- The independently observable extraction contract is:
  parse a finite numeric value before `% remaining today`, round, then clamp
  to `0...100`;
  treat it as **remaining**; derive used geometry as `100 - remaining`;
  assign a 24-hour/daily semantic window; preserve the server cadence
  `"Resets daily"`. CodexBar's merged parser and fixture demonstrate exactly
  this mapping:
  [`AmpUsageParser.swift`](https://github.com/steipete/CodexBar/blob/7535638cc4dd25254abb69d9de4766ede07432a3/Sources/CodexBarCore/Providers/Amp/AmpUsageParser.swift#L31-L78).
- The daily response supplies no exact reset timestamp. CodexBar deliberately
  publishes `resetsAt == nil` with a daily reset description and prevents a
  cached legacy rolling-reset timestamp from overriding it:
  [`AmpUsageSnapshot.swift`](https://github.com/steipete/CodexBar/blob/7535638cc4dd25254abb69d9de4766ede07432a3/Sources/CodexBarCore/Providers/Amp/AmpUsageSnapshot.swift#L42-L83),
  [maintainer landing note](https://github.com/steipete/CodexBar/pull/2071#issuecomment-4948976294).
  jackin❯ must not fabricate midnight or a countdown from this line.

### 3. Daily is proven only for Amp Free

- The live line names **Amp Free**. No public capture proves a daily
  Megawatt or Gigawatt subscription allowance. The redacted transcript was
  captured July 11, before subscriptions launched July 18, so it cannot
  establish paid-account text. Confidence: HIGH.
- Amp's official subscription announcement says Megawatt and Gigawatt are
  monthly subscriptions with included monthly agent usage, and usage beyond
  the monthly inclusion requires linked subscriptions or paid credits:
  [Subscriptions, At Last](https://ampcode.com/news/subscriptions).
  Confidence: HIGH.
- Amp's current pricing page says the included subscription allowance
  replenishes at the end of each monthly period:
  [Pricing](https://ampcode.com/pricing). A paid subscriber may also receive
  the independent Amp Free allowance; one line must not overwrite or infer
  the other. Confidence: HIGH for monthly cadence, UNKNOWN for which paid
  accounts also receive the Free line.
- Therefore the Desktop glance contract is: use the Amp Free **daily**
  remaining percentage when that exact server line exists. Do not label a
  paid subscription's monthly inclusion, individual credits, or workspace
  balance as daily. If a successful paid-only response lacks the Amp Free
  daily line, the Amp item remains present and shows the spec's unavailable
  dash while detail surfaces still show the returned quota-bound balances.
- Megawatt/Gigawatt plan names and monthly `displayText` remain a separate
  capture-gated follow-up. The daily fix does not infer a paid plan label.

### 4. Workspace credits are present in the same current display text

- The redacted live `amp usage` transcript in PR #2071 proves
  `Workspace <name>: $N remaining` lines can accompany Amp Free and
  individual credits. That corrects chapter 04's older conclusion that the
  CLI surface exposed no workspace balance: no separate RPC exists, but the
  balance is embedded in `userDisplayBalanceInfo.displayText`.
- These values are provider-supplied quota bounds, allowed by the
  limits-only rule. They belong in detail surfaces only; they are never the
  status-item percentage.

### 5. Exact jackin❯ gap and structural repair

- Current `crates/jackin-usage/src/usage/amp.rs` recognizes only the retired
  `$remaining/$limit (replenishes +$N/hour)` form. On the current daily text,
  it may parse credit lines while dropping the daily quota, then still
  hardcode plan label `"Amp Free"` because *some* usage parsed. It also has
  duplicate API/CLI bucket builders and speculative structured keys absent
  from Amp's declared response. Confidence: HIGH from live source.
- Root repair:
  1. Add a semantic `Daily` quota slot alongside `Session`/`Weekly`; map it
     through the existing FFI string projection.
  2. Replace the duplicate legacy API/CLI Amp models with one current
     `displayText` parser and one bucket builder.
  3. Parse the exact daily percentage, account identity, individual credits,
     and repeated workspace balances. Drop the retired hourly reader,
     replenishment-derived reset math, and speculative structured-key
     fallbacks.
  4. Tag only the parsed Amp Free quota as `Daily`, preserve `"Resets daily"`
     without inventing `resets_at`. If the product displays `"Amp Free"` in
     the plan position, gate it strictly on that line: the evidence proves an
     entitlement label, not that a paid subscriber has no separate monthly
     plan.
  5. The Rust-owned Desktop glance selector uses `Weekly` for Codex, Claude,
     Grok, z.ai, Kimi, and MiniMax; Amp alone uses `Daily`. Swift receives the
     chosen percentage and finished labels, never cadence-selection logic.
  6. Tests pin exact parsing, clamp edges, absence on paid-only credit text,
     repeated workspace balances, no legacy reset inheritance, semantic slot,
     and the seven-provider Desktop glance selection.

## Ruled out

- Treating individual/workspace credits as the daily percentage.
- Mapping Megawatt/Gigawatt monthly inclusion to Daily without a capture.
- Preserving the retired hourly parser as a compatibility lane.
- Fabricating a midnight timestamp: the current server text says only
  `"resets daily"`.
- Copying CodexBar implementation. The plan uses an independently specified
  wire fixture and Rust-native design.

## Open unknowns

- Exact `displayText` for Megawatt, Gigawatt, linked-ChatGPT, linked-X, and
  paid-only accounts.
- Whether Amp Free remains available alongside every paid subscription.
- Paid plan label and monthly-inclusion line shape.

These unknowns no longer block the Amp Free daily implementation. They block
only the paid-plan/monthly follow-up.
