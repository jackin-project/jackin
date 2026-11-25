# Usage accounts redesign: final spec

Status: design decision. No repo code was changed. All mockups show the real data from `ux-before/usage-details-json.txt` (14 connections). They are rendered as of `generated_at = 1795534620` = Tue 24 Nov 2026 22:37 (+07). The mockups come from `ux-design/final/gen.py` (CLI) and `ux-design/final/tui.py` (TUI frame), which apply the rules in this document, so the numbers, ages, local times and alignment are computed, not typed. Where this text and a mockup disagree, the rule in this text wins. The goldens become the authority once they exist.

## 1. Decision

**Base: candidate B, "provider sections with account cards".** The requirement says: provider group, then account with plan, then main limit as a bar, then secondary windows indented beneath. B's card is that sentence. B is also the only candidate designed for 80 columns that still shows the plan. A drops PLAN at 80 columns. C's "top 3 + N more" hides secondary windows that the requirement says to show. B also uses one card function on every surface, which gives the cleanest split into parallel work.

**Grafted from A (dense dashboard):**

- Number grammar: below 100,000, exact with thousands separators (`19,290`, `4,000`). From 100,000 up, 3 significant figures with K/M/B/T (`420K`, `4.17M`, `31.5B`, `82B`). Real plurals (`1 ready`, never `1 resets`).
- Explicit main-limit roles (`Overall` / `Scoped` / `Addon`) in place of B's prose heuristic.
- One `ResetCredit` per reset, so Z.AI's "1 ready, 1 unavailable" is representable. Today the overview prints "2 resets".
- `account usage ls`: the columns PROVIDER, ACCOUNT, PLAN, STATUS, FLAGS. `--wide` adds FETCHED, ID and LAST ERROR. The provider name repeats on every row, so `grep` and `awk` work.
- Failures are reported inline on the card. The stray stderr line `personal@example.com: HTTP 403 SERVICE_DISABLED` goes away, because the card and the attention block both carry it.
- Rows never reorder during a TUI refresh.

**Grafted from C (attention-first):**

- A short **Needs attention** block at the top of `usage` and `snapshot`, with the word tags `FAILING`, `NEAR LIMIT` and `DUPLICATE`. The word carries the severity, so color is never needed. Findings that repeat across duplicate connections fold into one entry that cites both short IDs.
- Short IDs (`#a0000999`) appear only where they tell two connections apart (duplicates). Full UUIDs appear only in `details`, `ls --wide`, `--json` and the `rm` command that the attention block prints.
- Quantity-less quotas (MiniMax `limit: 0, unit: count`) render as a percent only. The renderer never invents a quantity line.
- `details` keeps the yesterday and 30-day activity breakdown and the top models. Per-day counts move to `--json` only. No new `--history` flag.

**Rejected:**

- A's 98-column fixed grid, `--compact`, and the `14h!` age glyph. The glyph is cryptic, and the grid breaks the plan requirement at 80 columns.
- A's snapshot "ledger" with absolute times. `--json` is the machine path, so `snapshot` reuses the `usage` renderer.
- B's tree glyphs (`├ └ │`) and B's `⚠`/`✗` glyphs, which can render double-width. Indentation alone carries the structure. Words carry the severity.
- C's severity-ordered provider groups and C's "+N more" collapsing.
- New flags beyond `--wide` and `snapshot --json`. A unique-prefix `--account` is deferred (section 10).

## 2. Rules (shared by every surface)

### 2.1 Card layout (80 columns)

```text
<Provider display name>                                             <n> accounts
  <label>[ #short] · <plan | plan not reported>     <freshness, right-aligned>
    <Main limit 16> <bar 8> <pct 4> used  <reset text>
                     <quantity line, only for amount limits>
      <Secondary 14> <bar 8> <pct 4> used  <reset text>
      <Note name 14> <text>
    ! <failure line>          (only when the last refresh failed)
    ! also connected as #short (same provider and label)
```

- The provider heading is the display name from the catalog. Never a slug.
- The bar column starts at column 21 on main and secondary lines, so the bars align.
- The bar has 8 cells: `█` used, `░` free. A nonzero value fills at least one cell. A value below 100% never fills all 8.
- Percent is **always % used**. `0%`; `<1%` for anything in (0, 1); `>99%` for anything in (99, 100); otherwise a rounded integer. Exact values appear in `details` only, and only when the rounding hides them (`exact 99.72%`).
- The `DashboardSettings.remaining` toggle ("Show remaining usage") is removed from `presentation::flags`. `details` shows `N% left` as secondary text.
- A blank line separates cards. No rules, boxes or tree glyphs appear in CLI text.

### 2.2 Main-limit selection: `AccountQuota::main`

1. Take the **live** meters. A meter is live when `resets_at` is in the future, or when it has no reset time.
2. Pick from role `Overall` first. If no live `Overall` meter exists, pick from role `Scoped`. Addon meters never become the main limit.
3. Within the role, the highest used percent wins, compared as a rounded integer. A tie goes to the longer window.
4. If no live `Overall` or `Scoped` meter exists, the longest `Overall` or `Scoped` meter becomes the main limit, shown as rolled over (Grok).

The builder table in `view/build.rs` assigns each role:

| Role | Metric keys |
|---|---|
| Overall | `session`, `weekly`, `monthly`, `monthlyTotal`, `tokenPlan`, `freeRequests`, `general/*` |
| Scoped | `fable`, `sonnet`, `opus`, `spark*`, `3p-*`, `gemini-*` |
| Addon | `extraUsage`, `webSearches`, `booster`, `keyLimit`, `budget`, `payAsYouGo` |
| Notes (not meters) | `credits` and `balance` (Balance), `rateLimitResets` (Resets), history (Tokens), `period_spend` (Spend) |

**The Claude Fable case:** account alexey2 has Fable at 4% and Weekly at 2%. The main limit is **Weekly 2%**, because Fable is a model-scoped window and the plan headline is the overall window. Fable shows on the next line. A scoped or addon window does not stay hidden when it binds: any live meter at the critical threshold or above enters **Needs attention**, whatever its role. The TUI applies a related rule of its own. Termrock re-ranks an account's rows by severity, so a critical Fable row would lead that account's list row. That is acceptable, because it is the same fact the attention block shows.

The rule produces these results on the real data: Muse → Weekly 99%. OpenCode → Monthly 11% (it beats Weekly 8%). Z.AI → Weekly (it ties Session at 0%, and the longer window wins; web searches is an addon). Antigravity → Claude weekly >99% (all of its meters are scoped). MiMo → Token Plan. OpenRouter → Free requests. Grok → Weekly, rolled over.

### 2.3 Windows that reset after the fetch ("rolled over")

A meter is rolled over when `fetched_at < resets_at <= now`. The number that was saved is no longer true. It renders like this:

```text
      Session        ╌╌╌╌╌╌╌╌    -       reset 11h 26m ago · 11:10 · was 9%
```

The track is `╌` (dim) and the percent column shows `-`. The reset text is past tense with the local clock. `was N%` keeps the old reading, dimmed. This replaces today's `reset due`, which made a 9%-used window read as "91% remaining" after it had reset. A rolled-over meter is never the main limit unless nothing is live (rule 4). It never triggers NEAR LIMIT, and it never colors red or yellow. `details` adds `was 9% at fetch · 5h window from Tue 24 Nov 06:10`. On the real data, 9 windows on 7 accounts are rolled over.

### 2.4 Reset and time text

- Future reset: `resets in <rel> · <local>`. Past reset: `reset <rel> ago · <local>`. No reset time: `reset not reported`.
- `<rel>` uses two units at most: `<1m`, `42m`, `8h 27m`, `2d 15h`. The same function produces ages (`fetched 13h 57m ago`).
- `<local>` uses the clock's UTC offset (`Clock::utc_offset_secs`): `HH:MM` when the date is today; `Ddd HH:MM` within 6 days, before or after; `D Mon HH:MM` later in the same year; `D Mon YYYY` in another year. `details` uses full dates (`Fri 11 Dec 2026 05:00`).
- The report title states the offset once: `Tue 24 Nov 2026 22:37 (+07)`.

### 2.5 Freshness, and the stale-noise rule

- Every account header ends, right-aligned, with exactly one of:
  - `fetched 3m ago` (dim): within the 600 s TTL.
  - `stale · fetched 13h 57m ago` (`stale` yellow).
  - `failed · fetched 1d 5h ago` (`failed` red).
  - `waiting for first refresh` (dim).
- **Stale noise:** stale status is never an attention entry. The report prints at most one summary line under the attention block:
  - `Stale  all 14 readings, fetched 13h 57m to 1d 7h ago · jackin usage refresh` when every reading is stale.
  - `Stale  3 of 14 readings, …` when some are.
  - Nothing when none are.
  - A single-account report says `the reading`.
- The per-card freshness stays, because the requirement says freshness is always visible. On a TTY only the word `stale` is colored. The age stays dim, so a fully stale board is not a wall of yellow.

### 2.6 Needs attention

Entries come in this order: `FAILING`, then `NEAR LIMIT`, then `DUPLICATE`. Within each kind they follow provider and account order. Each entry has two lines: a subject line, `Provider · label [#short …]`, then a detail line indented 14. The heading is `Needs attention (N)`, and the whole block is omitted when N = 0.

- **FAILING:** the last refresh failed. Detail: `<message> · retry due since <local>`, or `next retry in <rel>`.
- **NEAR LIMIT:** a live meter of any role at `used >= 1 - critical/100`. The `DashboardSettings` defaults give 90%. The 70% warning level only colors bars yellow; it never adds an attention entry. Detail: `<meter> <pct> used · resets in <rel> · <local>`. Identical findings on duplicate connections fold into one entry.
- **DUPLICATE:** two or more connections with the same provider and label. Detail: the exact `jackin account usage rm <full id>` command. The suggested ID is the failing connection, or else the one last in registry order. A command is never truncated or wrapped.

### 2.7 Ordering is stable

Providers follow `DashboardSettings.provider_order` when the user has set it, and catalog order otherwise:

> Claude, ChatGPT / Codex, Muse, Grok, Kimi, Z.AI, MiniMax, MiMo, Antigravity (Google AI), OpenCode, OpenRouter

Accounts follow registry order within a provider. Severity never moves a group or a card. Urgency is the attention block's job. `account usage ls` and `usage providers` use the same order.

### 2.8 Quantities and units

- Amount meters (limit > 0, unit not percent) add a dim quantity line at column 21:
  - `31.5B of 82B credits used · 50.5B left` (MiMo)
  - `10 of 4,000 searches used · 3,990 left`
  - `0 of 20 SGD used · 20 SGD left`
  - `0 of 1,000 requests used · 1,000 left`
- Money always uses the ISO code after the number (`771.60 USD`, `20 SGD`). The number has 2 decimals when it has cents.
- Approximations use `≈`: `19,290 credits · ≈ 771.60 USD`.
- Notes are one line each in the summary view:
  - `Credits`
  - `Limit resets`: `3 ready · next expires 11 Dec`, or `1 ready, 1 unavailable · …`
  - `Tokens`: `today n/a · yesterday 420K · 30 days 96.7B`
  - `Spend`: `today 0 USD · week 0 USD · month 0 USD`

### 2.9 Color and pipes

- Renderers produce `Vec<Line>` of `Span { text, tone }`. Only `view::render::ansi` emits escape codes.
- `ansi` is used only when stdout is a terminal, `NO_COLOR` is unset and `TERM` is not `dumb`. In every other case the output is `view::render::plain`, which is the same characters with no ANSI.
- `--json` is the only JSON path on every command. `jackin usage snapshot` prints text by default (a behavior change; scripts add `--json`).
- Tone map:

  | Tone | Used for |
  |---|---|
  | Strong (bold) | Provider names, account labels, the report title, the attention heading |
  | Crit (red) | Bar fill and percent at the critical level or above, `failed`, `FAILING`, `NEAR LIMIT` |
  | Warn (yellow) | Bar fill and percent at the warning level or above, `stale`, `DUPLICATE`, `! also connected` |
  | Ok (green) | Bar fill below the warning level |
  | Dim | Bar track, rolled-over text, quantity lines, `details` sublines, ages, account counts |

- Glyphs stay UTF-8 on pipes. The only glyphs are `█ ░ ╌ · ≈ …` (`·` is already in today's output).

### 2.10 Width

- Cards are laid out for 80 columns. A pipe or file always gets width 80, which is deterministic and diffable.
- On a TTY the width is `min(terminal columns, 80)`. Wider terminals do not stretch rows.
- Below 80, apply these steps in order:
  1. Drop the local clock from reset text (`resets in 2d 15h`).
  2. Shorten the freshness to `stale 13h 57m`, `failed 1d 5h` or `fresh 3m`.
  3. Truncate the plan, then the label, with `…`.
- The minimum supported width is 60.
- `·`-joined detail lines wrap at a part boundary, and the continuation keeps the same indent.
- Tables (`ls`, `providers`) size to their content and may exceed 80. `ls --wide` is wide by design.

### 2.11 Gemini

Gemini Code Assist and Gemini app chat limits stay removed. The `Gemini 5h` and `Gemini weekly` rows are Antigravity quota windows (existing data). Gemini app (web) appears only as the `Info` line in `usage providers`.

## 3. Normalized model

**Owner:** crate `jackin-usage`, new module `jackin_usage::view`. Files:

| File | Contents |
|---|---|
| `src/view.rs` | Module root and re-exports |
| `src/view/model.rs` | The types below |
| `src/view/build.rs` | `StoredSnapshot` + failure + settings → model |

Every surface renders only from `UsageView`. Provider quirks end in `build.rs`:

- Antigravity `remaining_fraction` becomes `UsedFraction(1 - f)`.
- MiniMax `remaining_percent` becomes `UsedFraction(1 - p/100)`. Its `limit: 0, unit: count` is ignored. The window comes from the `general/interval` or `general/weekly` key.
- MiMo `limit: 82000000000` becomes `Used::Amount { used: 31525748922, limit: PositiveAmount(82e9), unit: Credits }`, which renders as "82B credits, 38% used".
- Claude `unit: "sgd"` becomes `Money(SGD)`.
- `rolled_over` is computed from `now` at build time. The TUI rebuilds on every load, so it never goes stale.

```rust
pub struct UsageView {
    pub now: Timestamp,
    pub utc_offset_secs: i32,
    pub warn_at: UsedFraction,
    pub critical_at: UsedFraction,
    pub groups: Vec<ProviderGroup>,
    pub attention: Vec<Attention>,
}
pub struct ProviderGroup { pub provider: UsageProvider, pub name: &'static str, pub accounts: Vec<AccountQuota> }
pub struct AccountQuota {
    pub id: Uuid,
    pub label: String,
    pub plan: Option<String>,
    pub reading: Reading,
    pub duplicates: Vec<Uuid>,
    pub main: Option<Meter>,
    pub secondary: Vec<Meter>,
    pub notes: Vec<Note>,
    pub facts: Vec<Fact>,
}
pub enum Reading {
    Waiting,
    Fresh { at: Timestamp },
    Stale { at: Timestamp },
    Failed { at: Option<Timestamp>, message: String, retry_at: Option<Timestamp> },
}
pub struct Meter {
    pub key: String,
    pub name: String,
    pub role: Role,
    pub window_secs: Option<u32>,
    pub resets_at: Option<Timestamp>,
    pub used: Used,
    pub rolled_over: bool,
}
pub enum Role { Overall, Scoped, Addon }
pub enum Used { Fraction(UsedFraction), Amount { used: f64, limit: PositiveAmount, unit: Unit } }
pub struct UsedFraction(f64);
pub struct PositiveAmount(f64);
pub enum Unit { Credits, Tokens, Requests, Searches, Money(Currency) }
pub struct Currency([u8; 3]);
pub enum Note {
    Balance { amount: f64, unit: Unit, approx: Option<(f64, Currency)> },
    Resets(Vec<ResetCredit>),
    Tokens { today: Option<u64>, yesterday: Option<Activity>, last30: Option<Activity> },
    Spend { today: f64, week: f64, month: f64, byok: f64, currency: Currency },
}
pub struct ResetCredit { pub kind: String, pub ready: bool, pub expires_at: Timestamp }
pub struct Activity { pub tokens: u64, pub turns: u64, pub cached_in: u64, pub input: u64, pub output: u64, pub observed: (u8, u8), pub models: Vec<(String, u64)> }
pub enum Fact { Region(String), AutoRenew(bool), MimoClaw(bool), ApiKeyAuth, DeviceAuth, GoogleProject(String), KeyType(String), PeriodStart(Timestamp) }
pub enum Attention {
    Failing { account: Uuid },
    NearLimit { accounts: Vec<Uuid>, meter: String },
    Duplicate { accounts: Vec<Uuid>, remove: Uuid },
}
pub fn build(db: &AccountRegistry, filter: &ViewFilter, now: Timestamp, utc_offset_secs: i32) -> Result<UsageView, ConnectionError>;
```

These are invariants. Constructors enforce them, and D17 forbids explaining them in code comments:

- `UsedFraction::new` clamps to 0..=1.
- `PositiveAmount::new` returns `None` for values ≤ 0.
- `main` is chosen by the rule in 2.2.
- `secondary` is sorted by role (Overall, Scoped, Addon), then by window ascending, then by name.
- `build` applies `conceal_names` (labels become `Account N`), `provider_order` and the `warning`/`critical` thresholds from `DashboardSettings`.
- `ViewFilter` is `{ provider: Option<UsageProvider>, account: Option<Uuid> }`, taken from `UsageFilter`.

`--json` is not affected. `export::snapshot_account` and `export::details_account` keep their schemas. `UsageView` is not serialized.

## 4. Render function ownership

All renderers are pure: `(&UsageView | &AccountQuota, &Ctx) -> Vec<Line>`. Commands only build the view, choose `plain` or `ansi`, and write the result.

```rust
pub enum Tone { Plain, Strong, Dim, Ok, Warn, Crit }
pub struct Span { pub text: String, pub tone: Tone }
pub struct Line(pub Vec<Span>);
pub struct Ctx { pub width: u16 }
pub enum Depth { Summary, Full }
pub enum Report { Usage, Snapshot, Details }
```

| Function | File | Owner |
|---|---|---|
| `text::{percent, bar, amount, money, rel, clock, full_date, freshness, tone_for}` | `view/text.rs` | U0 |
| `card(&AccountQuota, &UsageView, &Ctx, Depth) -> Vec<Line>` | `view/card.rs` | U0. `Depth::Full` calls the three `detail::*` hooks |
| `report(&UsageView, &Ctx, Report) -> Vec<Line>`: title, banner, attention, stale line, groups, cards | `view/report.rs` | U0 |
| `render::{plain, ansi, markup}(&[Line]) -> String` (markup gives `[red]…[/]` for goldens) | `view/render.rs` | U0 |
| `detail::{meter_lines, note_lines, footer}`: the `Full`-depth extras | `view/detail.rs` | U0 lands stubs that return `Vec::new()`. U2 fills them |
| `ls::table(&UsageView, wide: bool) -> Vec<Line>` | `crates/jackin/src/commands/connections/ls.rs` | U4 |
| `providers::table(...)` and the Info line | `crates/jackin/src/commands/connections/providers.rs` | U3 |
| TUI mapping: `UsageView` → `BoardMetric`/`UsageEntry`/`UsageBoard` strings | `crates/jackin-tui/src/connections/board.rs`, `crates/usage/jackin-usage/src/presentation.rs` | U5 |

## 5. Surfaces (final mockups)

### 5.1 `jackin usage` (piped or plain; a TTY gets the same text with color)

Refresh behavior is unchanged. A refresh failure appears on the card and in the attention block, not on stderr. Exit codes are unchanged. `jackin usage --json` is unchanged.

```text
Usage · Tue 24 Nov 2026 22:37 (+07) · 14 accounts · 11 providers

Needs attention (4)
  FAILING     Antigravity (Google AI) · personal@example.com #a0000999
              HTTP 403 SERVICE_DISABLED · retry due since Mon 22:28
  NEAR LIMIT  Muse · work@example.com
              Weekly 99% used · resets in 5d 8h · Mon 07:00
  NEAR LIMIT  Antigravity (Google AI) · personal@example.com #a0000999 #a0000999
              Claude weekly >99% used · resets in 1d 7h · Thu 06:19
  DUPLICATE   Antigravity (Google AI) · personal@example.com #a0000999 #a0000999
              jackin account usage rm a0000100-0000-4000-8000-000000000100
Stale  all 14 readings, fetched 13h 57m to 1d 7h ago · jackin usage refresh

Claude                                                                2 accounts
  work@example.com · Ultra                      stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   9% used  resets in 2d 15h · Fri 13:59
      Fable weekly   ░░░░░░░░   0% used  resets in 2d 15h · Fri 14:00
      Limit resets   3 ready · next expires 11 Dec

  work2@example.com · Ultra                     stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   2% used  resets in 5d 3h · Mon 02:00
      Fable weekly   █░░░░░░░   4% used  resets in 5d 3h · Mon 02:00
      Extra usage    ░░░░░░░░   0% used  reset not reported
                     0 of 20 SGD used · 20 SGD left
      Limit resets   2 ready · next expires 11 Dec

ChatGPT / Codex                                                       2 accounts
  work@example.com · Pro 200                    stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   1% used  resets in 4d 6h · Sun 05:12
      Credits        19,290 credits · ≈ 771.60 USD
      Limit resets   1 ready · next expires 13 Dec
      Tokens         today n/a · yesterday 420K · 30 days 96.7B

  home@example.com · Pro 200                       stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   2% used  resets in 4d 12h · Sun 11:35
      Credits        44,515 credits · ≈ 1,780.60 USD
      Limit resets   2 ready · next expires 2 Dec
      Tokens         today n/a · yesterday 4.17M · 30 days 60.8B

Muse                                                                   1 account
  work@example.com · Muse Code Max Usage        stale · fetched 13h 57m ago
    Weekly           ███████░  99% used  resets in 5d 8h · Mon 07:00
      Session        ╌╌╌╌╌╌╌╌    -       reset 11h 26m ago · 11:10 · was 9%

Grok                                                                   1 account
  Grok Account · SuperGrok Heavy                     stale · fetched 13h 57m ago
    Weekly           ╌╌╌╌╌╌╌╌    -       reset 1h 49m ago · 20:47 · was 55%

Kimi                                                                   1 account
  Kimi Code · plan not reported                      stale · fetched 13h 57m ago
    Weekly           █░░░░░░░  <1% used  resets in 5d 22h · Mon 21:33
      Session        ╌╌╌╌╌╌╌╌    -       reset 10h 3m ago · 12:33 · was 0%

Z.AI                                                                   1 account
  Z.ai Account · GLM Coding Max                      stale · fetched 13h 57m ago
    Weekly           ░░░░░░░░   0% used  resets in 5d 0h · Sun 22:49
      Session        ░░░░░░░░   0% used  reset not reported
      Web searches   █░░░░░░░  <1% used  resets in 7d 1h · 2 Dec 00:08
                     10 of 4,000 searches used · 3,990 left
      Limit resets   1 ready, 1 unavailable · next expires 22 Dec

MiniMax                                                                1 account
  MiniMax Token Plan · plan not reported             stale · fetched 13h 57m ago
    General weekly   ███░░░░░  34% used  resets in 8h 27m · Wed 07:04
      General 5h     ╌╌╌╌╌╌╌╌    -       reset 8h 57m ago · 13:39 · was 0%

MiMo                                                                   1 account
  MiMo Token Plan · max                              stale · fetched 13h 57m ago
    Token Plan       ███░░░░░  38% used  resets in 17d 8h · 12 Dec 06:59
                     31.5B of 82B credits used · 50.5B left

Antigravity (Google AI)                                               2 accounts
  personal@example.com #a0000999 · Google AI Ultra       stale · fetched 1d 7h ago
    Claude weekly    ███████░ >99% used  resets in 1d 7h · Thu 06:19
      Claude 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 5h ago · Mon 17:15 · was 0%
      Gemini 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 5h ago · Mon 17:15 · was 0%
      Gemini weekly  █░░░░░░░  <1% used  resets in 3d 20h · Sat 19:05
    ! also connected as #a0000999 (same provider and label)

  personal@example.com #a0000999 · Google AI Ultra      failed · fetched 1d 5h ago
    Claude weekly    ███████░ >99% used  resets in 1d 7h · Thu 06:19
      Claude 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 0h ago · Mon 22:06 · was 0%
      Gemini 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 0h ago · Mon 22:06 · was 0%
      Gemini weekly  █░░░░░░░  <1% used  resets in 3d 20h · Sat 19:05
    ! refresh failed: HTTP 403 SERVICE_DISABLED · retry due since Mon 22:28
    ! also connected as #a0000999 (same provider and label)

OpenCode                                                               1 account
  work@example.com · Go Plus                    stale · fetched 13h 57m ago
    Monthly          █░░░░░░░  11% used  resets in 13d 15h · 8 Dec 14:15
      Session        ╌╌╌╌╌╌╌╌    -       reset 9h 8m ago · 13:28 · was 0%
      Weekly         █░░░░░░░   8% used  resets in 5d 8h · Mon 07:00

OpenRouter                                                             1 account
  OpenRouter Account · Pay-as-you-go                 stale · fetched 13h 57m ago
    Free requests    ░░░░░░░░   0% used  resets in 8h 23m · Wed 07:00
                     0 of 1,000 requests used · 1,000 left
      Spend          today 0 USD · week 0 USD · month 0 USD
```

TTY color, shown as markup on the first lines and three cards. The characters are identical to the plain output above:

```text
[bold]Usage[/] · Tue 24 Nov 2026 22:37 (+07) · 14 accounts · 11 providers

[bold]Needs attention (4)[/]
  [red]FAILING   [/]  Antigravity (Google AI) · personal@example.com #a0000999
              HTTP 403 SERVICE_DISABLED · retry due since Mon 22:28
  [red]NEAR LIMIT[/]  Muse · work@example.com
              Weekly 99% used · resets in 5d 8h · Mon 07:00
  [red]NEAR LIMIT[/]  Antigravity (Google AI) · personal@example.com #a0000999 #a0000999
              Claude weekly >99% used · resets in 1d 7h · Thu 06:19
  [yellow]DUPLICATE [/]  Antigravity (Google AI) · personal@example.com #a0000999 #a0000999
              jackin account usage rm a0000100-0000-4000-8000-000000000100
[yellow]Stale[/]  all 14 readings, fetched 13h 57m to 1d 7h ago · jackin usage refresh

[bold]Muse[/]                                                                   [dim]1 account[/]
  [bold]work@example.com[/] · Muse Code Max Usage        [yellow]stale[/] · [dim]fetched 13h 57m ago[/]
    Weekly           [red]███████[/][dim]░[/] [red] 99%[/] used  resets in 5d 8h · Mon 07:00
      Session        [dim]╌╌╌╌╌╌╌╌[/] [dim]   -[/]       [dim]reset 11h 26m ago · 11:10[/] [dim]· was 9%[/]

[bold]Grok[/]                                                                   [dim]1 account[/]
  [bold]Grok Account[/] · SuperGrok Heavy                     [yellow]stale[/] · [dim]fetched 13h 57m ago[/]
    Weekly           [dim]╌╌╌╌╌╌╌╌[/] [dim]   -[/]       [dim]reset 1h 49m ago · 20:47[/] [dim]· was 55%[/]

[bold]Antigravity (Google AI)[/]                                               [dim]2 accounts[/]
  [bold]personal@example.com[/] [dim]#a0000999[/] · Google AI Ultra       [yellow]stale[/] · [dim]fetched 1d 7h ago[/]
    Claude weekly    [red]███████[/][dim]░[/] [red]>99%[/] used  resets in 1d 7h · Thu 06:19
      Claude 5h      [dim]╌╌╌╌╌╌╌╌[/] [dim]   -[/]       [dim]reset 1d 5h ago · Mon 17:15[/] [dim]· was 0%[/]
      Gemini 5h      [dim]╌╌╌╌╌╌╌╌[/] [dim]   -[/]       [dim]reset 1d 5h ago · Mon 17:15[/] [dim]· was 0%[/]
      Gemini weekly  [green]█[/][dim]░░░░░░░[/]  <1% used  resets in 3d 20h · Sat 19:05
[yellow]    ! also connected as #a0000999[/][dim] (same provider and label)[/]

  [bold]personal@example.com[/] [dim]#a0000999[/] · Google AI Ultra      [red]failed[/] · [dim]fetched 1d 5h ago[/]
    Claude weekly    [red]███████[/][dim]░[/] [red]>99%[/] used  resets in 1d 7h · Thu 06:19
      Claude 5h      [dim]╌╌╌╌╌╌╌╌[/] [dim]   -[/]       [dim]reset 1d 0h ago · Mon 22:06[/] [dim]· was 0%[/]
      Gemini 5h      [dim]╌╌╌╌╌╌╌╌[/] [dim]   -[/]       [dim]reset 1d 0h ago · Mon 22:06[/] [dim]· was 0%[/]
      Gemini weekly  [green]█[/][dim]░░░░░░░[/]  <1% used  resets in 3d 20h · Sat 19:05
[red]    ! refresh failed: HTTP 403 SERVICE_DISABLED[/][dim] · retry due since Mon 22:28[/]
[yellow]    ! also connected as #a0000999[/][dim] (same provider and label)[/]
```

Below 80 columns (here 60; rule 2.10):

```text
Claude                                            2 accounts
  work@example.com · Ultra                stale 13h 57m
    Weekly           █░░░░░░░   9% used  resets in 2d 15h
      Fable weekly   ░░░░░░░░   0% used  resets in 2d 15h
      Limit resets   3 ready · next expires 11 Dec

Antigravity (Google AI)                           2 accounts
  personal@example.com #a0000999 · Google AI…     failed 1d 5h
    Claude weekly    ███████░ >99% used  resets in 1d 7h
      Claude 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 0h ago
```

### 5.2 `jackin usage snapshot`

Reads saved data only. It prints text by default; `--json` (or the existing top-level `jackin usage --json snapshot`) prints today's JSON. The renderer is the same as `usage` with `Report::Snapshot`, which changes only the title and adds a banner. Everything below the banner is byte-identical to 5.1.

```text
Usage snapshot · Tue 24 Nov 2026 22:37 (+07) · 14 accounts · 11 providers
Saved readings only; no provider or credential access.
… (identical to 5.1 from "Needs attention" onward)
```

`--provider` and `--account` filter the view before rendering. The attention block and the stale line count only the selected accounts.

### 5.3 `jackin usage details` (all accounts)

`Report::Details` uses `Depth::Full`. Each meter gets a dim subline: left, window and start, plus the exact value when rounding hides it. Amount meters with a limit of 100,000 or more get an exact line. Each reset gets its own line. Tokens get an activity breakdown and the top models. Each card ends with the full ID, the fetch time with its freshness deadline, and facts. Details has no attention block, because each card carries its own `!` lines. The stale line stays.

```text
Usage details · Tue 24 Nov 2026 22:37 (+07) · 14 accounts · 11 providers

Stale  all 14 readings, fetched 13h 57m to 1d 7h ago · jackin usage refresh

Claude                                                                2 accounts
  work@example.com · Ultra                      stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   9% used  resets in 2d 15h · Fri 13:59
                     91% left · 7d window from Fri 20 Nov 13:59
      Fable weekly   ░░░░░░░░   0% used  resets in 2d 15h · Fri 14:00
                     100% left · 7d window from Fri 20 Nov 14:00
      Limit resets   3 ready · next expires 11 Dec
                     1  weekly ready       expires Fri 11 Dec 2026 05:00
                     2  fable  ready       expires Tue 9 Mar 2027 14:59
                     3  fable  ready       expires Tue 9 Mar 2027 14:59
    id a0000101-0000-4000-8000-000000000101
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    redeem a reset: jackin usage redeem <id> <n>

  work2@example.com · Ultra                     stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   2% used  resets in 5d 3h · Mon 02:00
                     98% left · 7d window from Mon 23 Nov 02:00
      Fable weekly   █░░░░░░░   4% used  resets in 5d 3h · Mon 02:00
                     96% left · 7d window from Mon 23 Nov 02:00
      Extra usage    ░░░░░░░░   0% used  reset not reported
                     0 of 20 SGD used · 20 SGD left
                     100% left
      Limit resets   2 ready · next expires 11 Dec
                     1  weekly ready       expires Fri 11 Dec 2026 05:00
                     2  fable  ready       expires Tue 9 Mar 2027 14:59
    id a0000102-0000-4000-8000-000000000102
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    redeem a reset: jackin usage redeem <id> <n>

ChatGPT / Codex                                                       2 accounts
  work@example.com · Pro 200                    stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   1% used  resets in 4d 6h · Sun 05:12
                     99% left · 7d window from Sun 22 Nov 05:12
      Credits        19,290 credits · ≈ 771.60 USD
                     exact 19,290 credits · 771.60 USD
      Limit resets   1 ready · next expires 13 Dec
                     1  full   ready       expires Sun 13 Dec 2026 05:19
      Tokens         today n/a · yesterday 420K · 30 days 96.7B
                     yesterday: 420,307 tokens · 2 turns
                       354K cached in · 59,352 in · 6,779 out
                     30 days: 96,689,333,883 tokens · 4,358 turns · 30/30 days
                       95.7B cached in · 829M in · 140M out
                     models, 30 days (turns) · gpt-6.1-terra 2,484
                     gpt-5.6-luna 754 · gpt-6.1-sol 558 · +5 more
                     per-day counts: jackin usage details --json
    id a0000103-0000-4000-8000-000000000103
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    redeem a reset: jackin usage redeem <id> <n>

  home@example.com · Pro 200                       stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   2% used  resets in 4d 12h · Sun 11:35
                     98% left · 7d window from Sun 22 Nov 11:35
      Credits        44,515 credits · ≈ 1,780.60 USD
                     exact 44,515 credits · 1,780.60 USD
      Limit resets   2 ready · next expires 2 Dec
                     1  full   ready       expires Wed 2 Dec 2026 05:49
                     2  full   ready       expires Sun 13 Dec 2026 05:43
      Tokens         today n/a · yesterday 4.17M · 30 days 60.8B
                     yesterday: 4,169,838 tokens · 0 turns
                       3.96M cached in · 204K in · 9,847 out
                     30 days: 60,751,131,615 tokens · 1,557 turns · 30/30 days
                       59.7B cached in · 869M in · 151M out
                     models, 30 days (turns) · gpt-6.1-sol 809 · gpt-5.6-sol 231
                     gpt-6-luna 154 · +6 more
                     per-day counts: jackin usage details --json
    id a0000104-0000-4000-8000-000000000104
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    redeem a reset: jackin usage redeem <id> <n>

Muse                                                                   1 account
  work@example.com · Muse Code Max Usage        stale · fetched 13h 57m ago
    Weekly           ███████░  99% used  resets in 5d 8h · Mon 07:00
                     1% left · 7d window from Mon 23 Nov 07:00
      Session        ╌╌╌╌╌╌╌╌    -       reset 11h 26m ago · 11:10 · was 9%
                     was 9% at fetch · 5h window from Tue 24 Nov 06:10
    id a0000105-0000-4000-8000-000000000105
    fetched Tue 24 Nov 08:39 · fresh until 08:49

Grok                                                                   1 account
  Grok Account · SuperGrok Heavy                     stale · fetched 13h 57m ago
    Weekly           ╌╌╌╌╌╌╌╌    -       reset 1h 49m ago · 20:47 · was 55%
                     was 55% at fetch · 7d window from Tue 17 Nov 20:47
    id a0000106-0000-4000-8000-000000000106
    fetched Tue 24 Nov 08:39 · fresh until 08:49

Kimi                                                                   1 account
  Kimi Code · plan not reported                      stale · fetched 13h 57m ago
    Weekly           █░░░░░░░  <1% used  resets in 5d 22h · Mon 21:33
                     >99% left · 7d window from Mon 23 Nov 21:33 · exact 0.04%
      Session        ╌╌╌╌╌╌╌╌    -       reset 10h 3m ago · 12:33 · was 0%
                     was 0% at fetch · 5h window from Tue 24 Nov 07:33
    id a0000107-0000-4000-8000-000000000107
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    region Global · auth API key

Z.AI                                                                   1 account
  Z.ai Account · GLM Coding Max                      stale · fetched 13h 57m ago
    Weekly           ░░░░░░░░   0% used  resets in 5d 0h · Sun 22:49
                     100% left · 7d window from Sun 22 Nov 22:49
      Session        ░░░░░░░░   0% used  reset not reported
                     100% left · 5h window
      Web searches   █░░░░░░░  <1% used  resets in 7d 1h · 2 Dec 00:08
                     10 of 4,000 searches used · 3,990 left
                     >99% left · 30d window from Mon 2 Nov 00:08 · exact 0.25%
      Limit resets   1 ready, 1 unavailable · next expires 22 Dec
                     1  weekly ready       expires Tue 22 Dec 2026 22:59
                     2  weekly unavailable expires Tue 22 Dec 2026 22:59
    id a0000108-0000-4000-8000-000000000108
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    redeem a reset: jackin usage redeem <id> <n>

MiniMax                                                                1 account
  MiniMax Token Plan · plan not reported             stale · fetched 13h 57m ago
    General weekly   ███░░░░░  34% used  resets in 8h 27m · Wed 07:04
                     66% left · 7d window from Wed 18 Nov 07:04
      General 5h     ╌╌╌╌╌╌╌╌    -       reset 8h 57m ago · 13:39 · was 0%
                     was 0% at fetch · 5h window from Tue 24 Nov 08:39
    id a0000109-0000-4000-8000-000000000109
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    region Global · auth API key

MiMo                                                                   1 account
  MiMo Token Plan · max                              stale · fetched 13h 57m ago
    Token Plan       ███░░░░░  38% used  resets in 17d 8h · 12 Dec 06:59
                     31.5B of 82B credits used · 50.5B left
                     62% left
                     exact 31,525,748,922 of 82,000,000,000 credits used
    id a0000110-0000-4000-8000-000000000110
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    region Singapore · auto-renew on · MiMo Claw off · auth API key
    period started Wed 11 Nov 21:27

Antigravity (Google AI)                                               2 accounts
  personal@example.com #a0000999 · Google AI Ultra       stale · fetched 1d 7h ago
    Claude weekly    ███████░ >99% used  resets in 1d 7h · Thu 06:19
                     <1% left · 7d window from Thu 19 Nov 06:19 · exact 99.72%
      Claude 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 5h ago · Mon 17:15 · was 0%
                     was 0% at fetch · 5h window from Mon 23 Nov 12:15
      Gemini 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 5h ago · Mon 17:15 · was 0%
                     was 0% at fetch · 5h window from Mon 23 Nov 12:15
      Gemini weekly  █░░░░░░░  <1% used  resets in 3d 20h · Sat 19:05
                     >99% left · 7d window from Sat 21 Nov 19:05 · exact 0.28%
    ! also connected as #a0000999 (same provider and label)
    id a0000111-0000-4000-8000-000000000111
    fetched Mon 23 Nov 14:53 · fresh until Mon 15:03

  personal@example.com #a0000999 · Google AI Ultra      failed · fetched 1d 5h ago
    Claude weekly    ███████░ >99% used  resets in 1d 7h · Thu 06:19
                     <1% left · 7d window from Thu 19 Nov 06:19 · exact 99.72%
      Claude 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 0h ago · Mon 22:06 · was 0%
                     was 0% at fetch · 5h window from Mon 23 Nov 17:06
      Gemini 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 0h ago · Mon 22:06 · was 0%
                     was 0% at fetch · 5h window from Mon 23 Nov 17:06
      Gemini weekly  █░░░░░░░  <1% used  resets in 3d 20h · Sat 19:05
                     >99% left · 7d window from Sat 21 Nov 19:05 · exact 0.28%
    ! refresh failed: HTTP 403 SERVICE_DISABLED · retry due since Mon 22:28
    ! also connected as #a0000999 (same provider and label)
    id a0000100-0000-4000-8000-000000000100
    fetched Mon 23 Nov 17:06 · fresh until Mon 17:16
    Google project example-project

OpenCode                                                               1 account
  work@example.com · Go Plus                    stale · fetched 13h 57m ago
    Monthly          █░░░░░░░  11% used  resets in 13d 15h · 8 Dec 14:15
                     89% left · 30d window from Sun 8 Nov 14:15
      Session        ╌╌╌╌╌╌╌╌    -       reset 9h 8m ago · 13:28 · was 0%
                     was 0% at fetch · 5h window from Tue 24 Nov 08:28
      Weekly         █░░░░░░░   8% used  resets in 5d 8h · Mon 07:00
                     92% left · 7d window from Mon 23 Nov 07:00
    id a0000112-0000-4000-8000-000000000112
    fetched Tue 24 Nov 08:39 · fresh until 08:49

OpenRouter                                                             1 account
  OpenRouter Account · Pay-as-you-go                 stale · fetched 13h 57m ago
    Free requests    ░░░░░░░░   0% used  resets in 8h 23m · Wed 07:00
                     0 of 1,000 requests used · 1,000 left
                     100% left · 24h window from Tue 24 Nov 07:00
      Spend          today 0 USD · week 0 USD · month 0 USD
                     BYOK spend 0 USD in every period
    id a0000113-0000-4000-8000-000000000113
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    standard key; workspace balance needs a management key
```

### 5.4 `jackin usage details --account a0000103-0000-4000-8000-000000000103` (one account)

```text
Usage details · Tue 24 Nov 2026 22:37 (+07) · 1 account · 1 provider

Stale  the reading, fetched 13h 57m ago · jackin usage refresh

ChatGPT / Codex                                                        1 account
  work@example.com · Pro 200                    stale · fetched 13h 57m ago
    Weekly           █░░░░░░░   1% used  resets in 4d 6h · Sun 05:12
                     99% left · 7d window from Sun 22 Nov 05:12
      Credits        19,290 credits · ≈ 771.60 USD
                     exact 19,290 credits · 771.60 USD
      Limit resets   1 ready · next expires 13 Dec
                     1  full   ready       expires Sun 13 Dec 2026 05:19
      Tokens         today n/a · yesterday 420K · 30 days 96.7B
                     yesterday: 420,307 tokens · 2 turns
                       354K cached in · 59,352 in · 6,779 out
                     30 days: 96,689,333,883 tokens · 4,358 turns · 30/30 days
                       95.7B cached in · 829M in · 140M out
                     models, 30 days (turns) · gpt-6.1-terra 2,484
                     gpt-5.6-luna 754 · gpt-6.1-sol 558 · +5 more
                     per-day counts: jackin usage details --json
    id a0000103-0000-4000-8000-000000000103
    fetched Tue 24 Nov 08:39 · fresh until 08:49
    redeem a reset: jackin usage redeem <id> <n>
```

The failing duplicate (`--provider antigravity`) shows the failure and duplicate lines and the Google project fact:

```text
Usage details · Tue 24 Nov 2026 22:37 (+07) · 2 accounts · 1 provider

Stale  all 2 readings, fetched 1d 5h to 1d 7h ago · jackin usage refresh

Antigravity (Google AI)                                               2 accounts
  personal@example.com #a0000999 · Google AI Ultra       stale · fetched 1d 7h ago
    Claude weekly    ███████░ >99% used  resets in 1d 7h · Thu 06:19
                     <1% left · 7d window from Thu 19 Nov 06:19 · exact 99.72%
      Claude 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 5h ago · Mon 17:15 · was 0%
                     was 0% at fetch · 5h window from Mon 23 Nov 12:15
      Gemini 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 5h ago · Mon 17:15 · was 0%
                     was 0% at fetch · 5h window from Mon 23 Nov 12:15
      Gemini weekly  █░░░░░░░  <1% used  resets in 3d 20h · Sat 19:05
                     >99% left · 7d window from Sat 21 Nov 19:05 · exact 0.28%
    ! also connected as #a0000999 (same provider and label)
    id a0000111-0000-4000-8000-000000000111
    fetched Mon 23 Nov 14:53 · fresh until Mon 15:03

  personal@example.com #a0000999 · Google AI Ultra      failed · fetched 1d 5h ago
    Claude weekly    ███████░ >99% used  resets in 1d 7h · Thu 06:19
                     <1% left · 7d window from Thu 19 Nov 06:19 · exact 99.72%
      Claude 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 0h ago · Mon 22:06 · was 0%
                     was 0% at fetch · 5h window from Mon 23 Nov 17:06
      Gemini 5h      ╌╌╌╌╌╌╌╌    -       reset 1d 0h ago · Mon 22:06 · was 0%
                     was 0% at fetch · 5h window from Mon 23 Nov 17:06
      Gemini weekly  █░░░░░░░  <1% used  resets in 3d 20h · Sat 19:05
                     >99% left · 7d window from Sat 21 Nov 19:05 · exact 0.28%
    ! refresh failed: HTTP 403 SERVICE_DISABLED · retry due since Mon 22:28
    ! also connected as #a0000999 (same provider and label)
    id a0000100-0000-4000-8000-000000000100
    fetched Mon 23 Nov 17:06 · fresh until Mon 17:16
    Google project example-project
```

### 5.5 `jackin usage providers`

```text
Providers · 11 tracked · 11 connected · 14 accounts

PROVIDER                 ID           ACCOUNTS  NOTE
Claude                   claude              2
ChatGPT / Codex          codex               2
Muse                     muse                1
Grok                     grok                1
Kimi                     kimi                1
Z.AI                     zai                 1
MiniMax                  minimax             1
MiMo                     mimo                1
Antigravity (Google AI)  antigravity         2  same account connected twice
OpenCode                 opencode            1
OpenRouter               openrouter          1

Info  Gemini app (web): chat limits are not tracked; there is no usage source.
      Google AI plan quotas appear under Antigravity (Google AI).

Connect: jackin account usage connect <id>
Filter:  jackin usage --provider <id>
```

- A provider with no connections prints `not connected` in ACCOUNTS.
- A provider the catalog marks unavailable prints its reason in NOTE.
- `jackin usage providers <id>` prints the same header, that one row, and then the account rows of that provider in the `ls` format.
- `--json` keeps `{schema, providers}` unchanged. The Info line goes in a new `info` array: `[{"id":"gemini-app","name":"Gemini app (web)","tracked":false,"reason":"…"}]`.

### 5.6 `jackin account usage ls`

The data comes from saved snapshots and failures. There is no network access. The columns are PROVIDER, ACCOUNT, PLAN, STATUS and FLAGS. STATUS is `fresh <age>`, `stale <age>`, `failing <age>` or `waiting`. FLAGS is `duplicate`.

```text
PROVIDER                 ACCOUNT                 PLAN                 STATUS         FLAGS
Claude                   work@example.com   Ultra                stale 13h 57m
Claude                   work2@example.com  Ultra                stale 13h 57m
ChatGPT / Codex          work@example.com   Pro 200              stale 13h 57m
ChatGPT / Codex          home@example.com      Pro 200              stale 13h 57m
Muse                     work@example.com   Muse Code Max Usage  stale 13h 57m
Grok                     Grok Account            SuperGrok Heavy      stale 13h 57m
Kimi                     Kimi Code               -                    stale 13h 57m
Z.AI                     Z.ai Account            GLM Coding Max       stale 13h 57m
MiniMax                  MiniMax Token Plan      -                    stale 13h 57m
MiMo                     MiMo Token Plan         max                  stale 13h 57m
Antigravity (Google AI)  personal@example.com      Google AI Ultra      stale 1d 7h    duplicate
Antigravity (Google AI)  personal@example.com      Google AI Ultra      failing 1d 5h  duplicate
OpenCode                 work@example.com   Go Plus              stale 13h 57m
OpenRouter               OpenRouter Account      Pay-as-you-go        stale 13h 57m

14 accounts · 11 providers · 1 failing · 1 duplicate · all stale
! Antigravity (Google AI): personal@example.com is connected twice.
  IDs: jackin account usage ls --wide · remove one: jackin account usage rm <id>
```

`--wide` adds FETCHED (local time), ID and LAST ERROR:

```text
PROVIDER                 ACCOUNT                 PLAN                 STATUS         FLAGS      FETCHED           ID                                    LAST ERROR
Claude                   work@example.com   Ultra                stale 13h 57m             Tue 24 Nov 08:39  a0000101-0000-4000-8000-000000000101  -
Claude                   work2@example.com  Ultra                stale 13h 57m             Tue 24 Nov 08:39  a0000102-0000-4000-8000-000000000102  -
ChatGPT / Codex          work@example.com   Pro 200              stale 13h 57m             Tue 24 Nov 08:39  a0000103-0000-4000-8000-000000000103  -
ChatGPT / Codex          home@example.com      Pro 200              stale 13h 57m             Tue 24 Nov 08:39  a0000104-0000-4000-8000-000000000104  -
Muse                     work@example.com   Muse Code Max Usage  stale 13h 57m             Tue 24 Nov 08:39  a0000105-0000-4000-8000-000000000105  -
Grok                     Grok Account            SuperGrok Heavy      stale 13h 57m             Tue 24 Nov 08:39  a0000106-0000-4000-8000-000000000106  -
Kimi                     Kimi Code               -                    stale 13h 57m             Tue 24 Nov 08:39  a0000107-0000-4000-8000-000000000107  -
Z.AI                     Z.ai Account            GLM Coding Max       stale 13h 57m             Tue 24 Nov 08:39  a0000108-0000-4000-8000-000000000108  -
MiniMax                  MiniMax Token Plan      -                    stale 13h 57m             Tue 24 Nov 08:39  a0000109-0000-4000-8000-000000000109  -
MiMo                     MiMo Token Plan         max                  stale 13h 57m             Tue 24 Nov 08:39  a0000110-0000-4000-8000-000000000110  -
Antigravity (Google AI)  personal@example.com      Google AI Ultra      stale 1d 7h    duplicate  Mon 23 Nov 14:53  a0000111-0000-4000-8000-000000000111  -
Antigravity (Google AI)  personal@example.com      Google AI Ultra      failing 1d 5h  duplicate  Mon 23 Nov 17:06  a0000100-0000-4000-8000-000000000100  HTTP 403 SERVICE_DISABLED
OpenCode                 work@example.com   Go Plus              stale 13h 57m             Tue 24 Nov 08:39  a0000112-0000-4000-8000-000000000112  -
OpenRouter               OpenRouter Account      Pay-as-you-go        stale 13h 57m             Tue 24 Nov 08:39  a0000113-0000-4000-8000-000000000113  -

14 accounts · 11 providers · 1 failing · 1 duplicate · all stale
! Antigravity (Google AI): personal@example.com is connected twice.
  Remove the failing one: jackin account usage rm a0000100-0000-4000-8000-000000000100
```

`--json` keeps today's `ConnectedAccount` fields and adds these:

| Field | Value |
|---|---|
| `provider_name` | The display name |
| `plan` | The plan string |
| `fetched_at` | The fetch time |
| `status` | `fresh`, `stale`, `failing` or `waiting` |
| `last_error` | The last refresh error |
| `duplicate_of` | An array of IDs |

The TSV `id\tprovider\tlabel` output is removed.

### 5.7 Console TUI (termrock renders; jackin changes only the data it hands over)

Termrock's layout, chrome, keys and title meta are fixed (git dependency `87ea570`). The frames below are today's termrock frames at 80×24, filled with the new strings. Approximate cells, such as the bar glyphs and the title meta, are termrock's. The goldens from section 7 are the authority.

**Usage page, All view.** Rows are ordered by provider, then account, then main meter first, so termrock's per-account entry leads with the main limit. Termrock re-ranks only by severity.

```text
  jackin❯    File   Go   Help              …  Usage › All Usage  ▲ gateway failing

 ╭─ All Usage ────────────────────────────────────────────────── 14 accounts ─╮
 │   Priority    Providers    Refresh    Attention…    Metrics                │
 │  Account / reading                                                Value    │
 │  ▎› Claude · work@example.com · Weekly             9% used · 2d 15h ┃ │
 │     work2@example.com · Weekly                      2% used · 5d 3h ┃ │
 │     ChatGPT / Codex · work@example.com · Weekly     1% used · 4d 6h ┃ │
 │     home@example.com · Weekly                         2% used · 4d 12h ┃ │
 │     Muse · work@example.com · Weekly               99% used · 5d 8h ┃ │
 │     Grok Account · Weekly                     reset 1h 49m ago · was 55% ┃ │
 │     Kimi Code · Weekly                                 <1% used · 5d 22h ┃ │
 │     Z.ai Account · Weekly                                0% used · 5d 0h ┃ │
 │     MiniMax Token Plan · General weekly                34% used · 8h 27m ┆ │
 │     MiMo Token Plan · Token Plan                       38% used · 17d 8h ┆ │
 │   work@example.com                                         Claude     │
 │   ─────────────────────────────────────────────────────────────────────    │
 │   Weekly                        ━━━━━━━━━━━━    9% used · 2d 15h           │
 │   Fable weekly                  ━━━━━━━━━━━━    0% used · 2d 15h           │
 │   Limit resets                                  3 ready · next 11 Dec      │
 ╰────────────────────────────────────────────────────────────────────────────╯

 p Priority  a All Usage  ↑↓ Move  Enter Open  r Refresh  Esc Back  …
```

**Accounts page, Usage accounts.** Entries are in provider/catalog order. A duplicate label gets ` #<first 8 of id>` from jackin, so termrock's own last-8-characters discriminator never triggers, and the CLI and TUI show the same short ID.

```text
  jackin❯    File   Go   Help                       …  Accounts  ▲ gateway failing

 ╭─ Accounts ──────────────────────────────────────── 14 usage · 0 execution ─╮
 │     Overview                                                             ┃ │
 │     Usage accounts                                           14 accounts ┃ │
 │       work@example.com                                       Claude ┃ │
 │       work2@example.com                                      Claude ┃ │
 │       work@example.com                              ChatGPT / Codex ┃ │
 │       home@example.com                                 ChatGPT / Codex ┆ │
 │       work@example.com                                         Muse ┆ │
 │       Grok Account                                                  Grok ┆ │
 │       Kimi Code                                                     Kimi ┆ │
 │       Z.ai Account                                                  Z.AI ┆ │
 │       MiniMax Token Plan                                         MiniMax ┆ │
 │                                                                            │
 │                                                                            │
 ╰────────────────────────────────────────────────────────────────────────────╯
```

#### Data contract

The rows below are for U5. `ctx` is `Ctx { width: 80 }`, and every string comes from the `view::text` atoms.

| termrock field | jackin value | Example (real data) |
|---|---|---|
| `UsageBoard.rows` | One `BoardMetric` per meter. Order: provider (2.7), then account, then main, then `secondary`. Notes follow as unbounded rows | |
| `BoardMetric.group` | Provider display name (All view). The Priority view keeps today's `Section` names | `Claude` |
| `BoardMetric.title` | `caption(label, meter name)` | `work@example.com · Weekly` |
| `BoardMetric.label` | Meter name | `Fable weekly` |
| `BoardMetric.value` | Live: `<pct> used · <rel>`. Rolled over: `reset <rel> ago · was <pct>`. Note: the summary text. At most 32 chars (termrock truncates meta at 32) | `9% used · 2d 15h`, `reset 1h 49m ago · was 55%`, `19,290 credits` |
| `BoardMetric.exact_value` | `details` subline text | `<1% left · 7d window from Thu 19 Nov 06:19 · exact 99.72%` |
| `BoardMetric.bar` / `bounded` | `Some((used% rounded, None))` for live meters; `None` and `false` for rolled-over meters and notes | |
| `BoardMetric.severity` | `Danger` at the critical level or above, `Warning` at the warning level or above, `Muted` when rolled over, else `Normal` | |
| `BoardMetric.reset` | Full reset text (2.4) | `resets in 2d 15h · Fri 13:59` |
| `BoardMetric.inspector` | `Plan · <plan>`, the freshness text (2.5), and `<window> window` | `Plan · Ultra`, `stale · fetched 13h 57m ago` |
| `BoardMetric.details` | `detail::meter_lines` rendered plain | |
| `UsageBoard.summary` | `<n> accounts · all stale` / `<k> of <n> stale` / `<n> accounts` | `14 accounts · all stale` |
| `UsageBoard.notices` | One string per attention entry: `<TAG> · <subject> · <detail>` | `FAILING · Antigravity (Google AI) · personal@example.com #a0000999 · HTTP 403 SERVICE_DISABLED` |
| `UsageEntry.label` | Label, plus ` #short` when duplicated within the provider | `personal@example.com #a0000999` |
| `UsageEntry.summary` | `[<plan> · <freshness>, <main meter line text>, <! lines…>]` | `Google AI Ultra · failed · fetched 1d 5h ago`, `Claude weekly >99% used · resets in 1d 7h · Thu 06:19`, `! refresh failed: HTTP 403 SERVICE_DISABLED`, `! also connected as #a0000999` |
| `UsageEntry.details` | `card(Depth::Full)` rendered plain | |

At 80 columns termrock already truncates labels and drops the bar when narrow. Jackin's part is to keep `value` at 32 chars or less, and never to put the reset clock in `value`.

## 6. JSON compatibility summary

| Command | Change |
|---|---|
| `jackin usage --json`, `jackin usage details --json` | Unchanged |
| `jackin usage snapshot` | Text by default. `--json` keeps today's schema |
| `jackin account usage ls --json` | Adds the fields in 5.6 |
| `jackin usage providers --json` | Adds `info` |

Call these out in the PR body.

## 7. Snapshot (golden) test approach

The repo has no `insta` and no `.snap` files. The only snapshot-style test compares against `include_str!("golden/report.json")` (`crates/usage/jackin-usage/tests/usage/schema.rs:188`). Decision: **plain-text golden files with a small helper in `jackin-testkit`**, not `insta`. Reasons:

- No new dependency, so no churn in `deny.toml` or in the latest-stable dependency policy (DESIGN.md).
- It matches the existing golden style.
- Reviewers read `.txt` diffs directly in the PR.
- Golden files carry no comments, so D17 is untouched.

**Helper (U0):** `crates/jackin-testkit/src/golden.rs`.

```rust
pub fn assert_golden(relative: &str, actual: &str, ids: &[Uuid]) -> TestResult;
```

- It resolves the path against `CARGO_MANIFEST_DIR`.
- It redacts each seeded UUID to a stable fake, in order, and its 8-character prefix as well. IDs are `Uuid::new_v4` in `add_with_snapshot`, so redaction is required. The fakes are `a0000001-0000-4000-8000-000000000001`, then `a0000002-…`, so short IDs stay distinct.
- With `JACKIN_BLESS=1` it writes the file. Otherwise it compares, and on a mismatch it fails with the first differing line number, the expected line and the actual line.

**Fixture (U0):**

- `crates/jackin-testkit/tests/fixtures/usage_view.json`: the 14 real snapshots with their real numbers, `fetched_at`, the failure (`HTTP 403 SERVICE_DISABLED`, `retry_at`) and the Google project.
- The labels are anonymized before the fixture enters the repo: `work@example.com` → `work@example.com`, `work2@example.com` → `work2@example.com`, `home@example.com` → `home@example.com`, `personal@example.com` → `personal@example.com`.
- `seed_usage_view(path) -> Vec<ProfileAccount>` sits next to `seed_usage_profile` in `usage_profile.rs`. It calls `add_with_snapshot` with each row's `fetched_at` and `record_failure`.
- The fake clock is fixed at `1795534620` with `utc_offset_secs = 25200`.

**Test levels.** Each unit owns its test files and goldens:

1. **Model (U0):** `crates/usage/jackin-usage/tests/view/`, asserting values with no goldens:
   - Main-limit selection on the real data, including Fable versus Weekly, the OpenCode monthly case, the all-scoped Antigravity case and the all-rolled-over Grok case.
   - Rolled-over detection.
   - The three normalizations (Antigravity, MiniMax, MiMo).
   - Duplicate detection and the attention list with folding.
2. **Renderer (U0, U2):** `crates/usage/jackin-usage/tests/view/render.rs`, with goldens in `tests/golden/view/`:
   - `usage.txt` and `usage.markup.txt`
   - `usage-60.txt`
   - `details.txt` (U2)
   - `details-one.txt` (U2)
3. **CLI (U1–U4):** `crates/jackin/tests/cli/` through `Harness`, with stdout as a non-TTY, so plain output. Goldens go in `crates/jackin/tests/cli/golden/<surface>.txt`. Each CLI test also asserts:
   - zero `\x1b` bytes;
   - the `--json` variant still parses with today's keys;
   - for `ls`, no UUID in the default output.
4. **TUI (U5):** `crates/jackin-tui/tests/console/usage_view.rs` with the existing `Lab`, at 80×24. Goldens: `lab.console.screen()` dumps of the All view, the Accounts list, and the duplicate account detail. A second test dumps the mapped `UsageBoard` rows (`group | title | value | reset | severity`) as text. That catches data regressions even when termrock changes its layout.

Per D18, each test plan needs approval from `tailrocks-test-reviewer` before it is written. Each test is kept only after it is seen red on broken code and green on fixed code. Bless files only after that proof. Delete the old format assertions this work makes obsolete, and replace them in the same unit. These are in `tests/cli/usage.rs` (line-per-window format), `tests/cli/usage_snapshot.rs` (JSON default) and `tests/cli/help.rs` where it pins the `ls` text.

## 8. Work units

U0 must merge first. U1–U5 then run in parallel with disjoint files. U6 is a short cleanup after all of them.

| Unit | Scope | Files it owns (creates or edits) |
|---|---|---|
| **U0 shared model** (first) | Model, builder, text atoms, summary card, report, render, detail stubs, golden helper, fixture, CLI flags, routing, color detection | `crates/usage/jackin-usage/src/view.rs`, `view/{model,build,text,card,report,render,detail}.rs`, `src/lib.rs` (`pub mod view`); `crates/usage/jackin-usage/tests/view/{main,model,render}.rs` + `tests/golden/view/usage*.txt`; `crates/jackin-testkit/src/{golden.rs,lib.rs,usage_profile.rs}`, `crates/jackin-testkit/tests/fixtures/usage_view.json`, fake-clock offset setter in `crates/jackin-testkit/src/host/clock.rs` if missing; `crates/jackin/src/cli.rs` (`snapshot --json`, `ls --wide`); `crates/jackin/src/output.rs` + `crates/jackin/src/lib.rs` (`Io` carries `color: bool`, decided from `terminal`, `NO_COLOR` and `TERM`); `crates/jackin/src/commands/connections.rs` (routing only: move the existing `overview` body to `connections/overview.rs` and the `Ls` body to `connections/ls.rs` without behavior changes, and route `Snapshot` to `overview::snapshot`); `crates/jackin/tests/cli/main.rs` (registers empty `usage_view`, `usage_providers`, `usage_ls` modules) |
| **U1 usage + snapshot** | Replace the overview text with `report(Report::Usage)`; snapshot text default; drop the stderr failure note | `crates/jackin/src/commands/connections/overview.rs`; `crates/jackin/tests/cli/{usage.rs,usage_snapshot.rs,usage_view.rs}`; `crates/jackin/tests/cli/golden/{usage,snapshot}.txt` |
| **U2 details** | Fill `detail::{meter_lines,note_lines,footer}`; replace the inline details text | `crates/usage/jackin-usage/src/view/detail.rs`; `crates/jackin/src/commands/connections/details.rs`; `crates/usage/jackin-usage/tests/golden/view/details*.txt`; `crates/jackin/tests/cli/usage_details.rs` + `golden/details*.txt` |
| **U3 providers** | Table, Info line, `info` in JSON | `crates/jackin/src/commands/connections/providers.rs`; `crates/usage/jackin-usage/src/catalog.rs` (an `info_only()` iterator holding the Gemini app entry); `crates/jackin/tests/cli/usage_providers.rs` + `golden/providers*.txt` |
| **U4 ls** | Table, `--wide`, JSON additions, duplicate footnote | `crates/jackin/src/commands/connections/ls.rs`; `crates/jackin/tests/cli/usage_ls.rs` + `golden/ls*.txt` |
| **U5 TUI** | Map `UsageView` into the termrock types (5.7); entry order and labels; remove the `remaining` flag | `crates/jackin-tui/src/connections/board.rs`; `crates/jackin-tui/src/connections.rs` (entry ordering only); `crates/usage/jackin-usage/src/presentation.rs` (`detail()`/`entries()` build from `view`; `flags!` loses `remaining`); `crates/jackin-tui/tests/console/usage_view.rs` + goldens; existing `crates/jackin-tui/tests/console/{usage,usage_windows,mixed_usage,overlap_usage}.rs` where they pin `remaining` wording |
| **U6 cleanup** (last) | Delete the helpers that no longer have callers | `presentation::{metric_value, exact_metric_value, reset_caption, metric_details, metric_inspector}` and any `DashboardRow` string fields left without readers; `connections.rs` leftovers |

Overlap notes:

- U5 and U6 both touch `presentation.rs`. U6 runs after U5 merges.
- U2 is the only unit that edits `view/detail.rs` after U0.
- If a unit needs a new text atom, it adds it in its own file. It does not edit `view/text.rs`. U6 may hoist duplicated atoms afterwards.

## 9. Acceptance checklist (for every unit)

- The output equals the golden at width 80 on a non-TTY.
- The output has no ANSI on a non-TTY.
- `--json` is the only JSON path.
- No UUIDs appear outside `details`, `ls --wide`, `--json` and the `rm` command.
- Freshness is visible on every account.
- Gemini app shows only as the providers `Info` line.
- `cargo xtask` comment check (D17) passes.
- The surface's mockup in section 5 matches its golden, except for the anonymized labels and redacted IDs.

## 10. Deferred, out of scope

- `--account` accepting a unique 8-character prefix or a label.
- `jackin usage --attention` (only the attention block, for prompts).
- Refusing a second connection with the same provider and label at connect time.
- A real identity key for duplicate detection. Today the rule is label equality within a provider.
- Changes to the termrock layout itself, such as group header rows and inline expansion. These need a termrock PR.
