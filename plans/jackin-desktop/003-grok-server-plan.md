# Plan 003: Decode Grok's current billing config, resolved server tier, quota bounds, and pace inputs

> **Executor instructions**: Follow this plan step by step. Run the
> preconditions first. Run every verification command and confirm the
> expected result before moving on. If anything in "STOP conditions"
> occurs, stop and report — do not improvise. When done, update this
> plan's status row in `plans/jackin-desktop/README.md`.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/jackin-desktop/005-status-bar-multi-item.md`
- **Covers**: spec/providers.md "Grok plan label and prepaid balance from server" (F7) — hub row 003
- **Guardrails**: N3 (inlined below)
- **Research basis**: research/agent-usage-provider-apis/05-grok-usage-api.md, research/agent-usage-provider-apis/10-phrase-provenance-and-misc.md (Q4), research/jackin-desktop-verification-tooling/01-commands.md
- **Planned at**: commit `3e6376d`, 2026-07-24

## Why this matters

jackin❯ currently labels every browser-OAuth Grok login "SuperGrok" via a
client-side heuristic (`auth_mode == "oidc"` → "SuperGrok"). The official
auth model marks `auth_mode` as token provenance only — a Free-tier browser
login also carries `oidc`, so Free accounts are mislabeled. The server
already ships one resolved truth: ACP `x.ai/billing.subscription_tier` is
populated upstream from display tier then machine tier. Its current response
has one nested `config`: preferred `creditUsagePercent/currentPeriod`, current
fallback `monthlyLimit/used/billingPeriodStart/End`, and confirmed
prepaid/on-demand quota bounds. jackin❯ instead models an obsolete top-level
billing lane. After this plan, one current decoder emits exactly one headline,
feeds period bounds into pace, exposes permitted quota bounds, ignores
history/product usage, and never guesses a plan.

## Preconditions — run before anything else

- Not on `main`: `rtk git branch --show-current` → an operator-confirmed feature
  branch (see Git workflow). On `main` → STOP and ask the operator.
- Resolve upstream conditionally:
  `if UPSTREAM="$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null)"; then REMOTE_HEAD="${UPSTREAM#*/}"; gh pr list --head "$REMOTE_HEAD"; else gh pr list --head "$(git branch --show-current)"; fi`.
  A missing upstream is accepted only for the approved new branch; after
  first push, the final proof below requires an upstream. Local/remote names
  may differ.
- Whole tree is clean:
  `test -z "$(git status --porcelain=v1)"`. This plan requires the preceding
  dependency commit to be complete; unrelated staged, unstaged, or untracked
  changes are a STOP because they make the exact-scope proof ambiguous.
- Planning artifacts are tracked:
  `git ls-files --error-unmatch plans/jackin-desktop/003-grok-server-plan.md plans/jackin-desktop/README.md roadmap/jackin-desktop/README.md roadmap/README.md`.
- Plan 005 is DONE and
  `rg -n 'usage_bucket_presentation_limit_only_balance' crates/jackin-usage/src/usage/tests.rs`
  finds its passing generic balance-only display contract. Run
  `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked`.
- Toolchain present: `cargo nextest --version` → prints a version, exit 0.
- Baseline green: `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked`
  → all tests pass.
- Dependency-aware cleanliness: require
  `rtk git status --short --` over every Scope path to be empty. Plans 001/002/005 legitimately
  changed shared tests/docs after research commit `3e6376d`; never demand
  those files equal that old commit. Re-locate the exact production anchors
  `GrokBillingResponse`, `grok_plan_label`, and
  `GrokBillingResponse::buckets`; if already current, verify and skip the
  satisfied edit, otherwise any third shape is a STOP for re-planning.

Any failed precondition is a STOP.

## Spec contract

Inlined **verbatim** from `plans/jackin-desktop/spec/providers.md` — do not
read `spec/`:

### Requirement: Grok plan label and prepaid balance from server
The Grok probe SHALL take the plan label from the server
(`x.ai/billing.subscription_tier`, already resolved display-first by the
official client from remote settings) and SHALL retire the
`auth_mode == "oidc"` → "SuperGrok" heuristic; it SHALL additionally expose
current `config.prepaidBalance` (Extra Usage Credits) and
`config.onDemandCap`/`onDemandUsed` as provider-supplied quota-bound buckets.
The primary headline SHALL prefer
`config.creditUsagePercent` + `config.currentPeriod`, then fall back to
`config.monthlyLimit` + `config.used` +
`config.billingPeriodStart/End`, without emitting duplicate headlines.
When the server tier is absent (web-scrape fallback path), the plan label
SHALL be empty rather than guessed.
Covers: F7 · Evidence: research/agent-usage-provider-apis/05-grok-usage-api.md

#### Scenario: SuperGrok account over ACP
- **GIVEN** the ACP billing response carries subscription tier data
- **WHEN** the Grok view is built
- **THEN** the plan label equals the server-resolved string and the detail
  buckets show prepaid and on-demand quota bounds

#### Scenario: Current config fallback
- **GIVEN** preferred percentage/period fields are absent but nested
  monthly limit, used, and billing-period bounds exist
- **WHEN** the Grok view is built
- **THEN** one headline bucket renders with pace inputs; no obsolete
  top-level billing lane or duplicate headline is used

#### Scenario: Free account over browser OAuth
- **GIVEN** a Free-tier login whose `auth.json` has `auth_mode: "oidc"`
- **WHEN** the view is built
- **THEN** the label is NOT "SuperGrok" (server truth or empty)

Done means these scenarios hold; the test plan below exercises them.

The spec file's Notes section additionally binds this plan (verbatim):

> All new labels/strings are produced in Rust (N1); limits-only rule (N3)
> binds every new bucket (prepaid balance is a quota bound, never a price).

## Must NOT

Guardrail inlined verbatim from the must-not registry
(`plans/jackin-desktop/spec/README.md`). It overrides anything a step seems
to imply:

- **N3**: No surface MUST ever show token unit prices, cost-of-session
  estimates, spend-over-time charts, trend sparklines, token/spend
  histories, aggregate-spend donuts, or cost-legend rankings —
  provider-supplied quota bounds (money caps, credit balances) are the only
  money allowed — reason: repo hard rule (CLAUDE.md usage-surfaces).

Concretely: `prepaidBalance` is remaining quota, so use
`limit_label`/`limit_money`, never spend. `onDemandCap` is a cap and
`onDemandUsed` is consumption within it, so the existing on-demand bucket may
carry both structured slots. Never show token unit prices. Do not decode or
surface `config.history` or `config.productUsage`.

## Inputs to provide

None — fully self-contained. All tests are fixture-based; no credentials or
live Grok account are required. (An operator with a logged-in `grok` CLI
may optionally smoke-test live, but absence never blocks.)

## Starting state

All in `crates/jackin-usage/` unless noted. Excerpts re-read from the live
files at commit `3e6376d`.

### The heuristic to retire

`crates/jackin-usage/src/usage/grok.rs:793-802`:

```rust
pub(crate) fn grok_plan_label(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    first_string_key(&value, "auth_mode").map(|mode| {
        if mode.eq_ignore_ascii_case("oidc") {
            "SuperGrok".to_owned()
        } else {
            mode
        }
    })
}
```

Its single call site, `grok.rs:102` (inside `grok_snapshot_from_rpc_result`,
building `UsageViewInput`):

```rust
        plan_label: grok_plan_label(auth),
```

It is also re-exported in `crates/jackin-usage/src/usage.rs:89-97` (block
carries `#[expect(unused_imports, ...)]`):

```rust
pub(crate) use self::grok::{
    GrokBillingCycle, GrokBillingResponse, GrokBillingSnapshot, GrokBillingUsage, GrokCent,
    GrokWebBillingSnapshot, fetch_grok_billing, fetch_grok_rpc_billing, fetch_grok_web_billing,
    grok_account_label, grok_account_label_or_presence, grok_bearer_token,
    grok_bearer_token_from_entry, grok_binary_path, grok_cycle_label_from_minutes,
    grok_cycle_label_from_reset, grok_plan_label, grok_rpc_request, grok_rpc_request_payload,
    grok_snapshot, grok_snapshot_from_rpc_result, grpc_web_data_frames,
    parse_grok_web_billing_response, scan_protobuf,
};
```

Research verdict on why the heuristic is wrong (ch. 05, Q2, quoted):
"Per the official model, `auth_mode` records login *method*, not plan
('Token provenance (debugging/auth.json only -- no code branches on this)';
`Oidc` covers browser OAuth and enterprise IdP alike), so a Free-tier
browser login also carries `oidc`."

### JSON keys the RPC decoder currently reads

`grok.rs:129-163` — the structs decoding the ACP `x.ai/billing` result.
Today they read an obsolete top-level (`GetGrokBuildBillingConfig`) shape:
`billingCycle{billingPeriodStart,billingPeriodEnd}`, `monthlyLimit{val}`,
`onDemandCap{val}`, `on_demand_enabled` (snake_case), and
`usage{includedUsed,onDemandUsed,totalUsed}` (each a `{val}` cents object).
The current credits-config keys (`config.*`, `subscription_tier`) are not
decoded at all:

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct GrokBillingResponse {
    #[serde(rename = "billingCycle")]
    pub(crate) billing_cycle: Option<GrokBillingCycle>,
    #[serde(rename = "monthlyLimit")]
    pub(crate) monthly_limit: Option<GrokCent>,
    #[serde(rename = "onDemandCap")]
    pub(crate) on_demand_cap: Option<GrokCent>,
    #[serde(rename = "on_demand_enabled")]
    pub(crate) on_demand_enabled: Option<bool>,
    pub(crate) usage: Option<GrokBillingUsage>,
}
```

(`GrokBillingCycle` holds the two period strings; `GrokBillingUsage` the
three `GrokCent` fields; `GrokCent { val: Option<i64> }` — grok.rs:142-163.)

### Server response shape to decode (research, verbatim facts)

Research ch. 05, Q1 (current official billing.rs plus its credits/fallback
tests): "**Current `x.ai/billing` response:** top-level `config`,
`on_demand_enabled`, and `subscription_tier`. One `config` object carries
`config.creditUsagePercent` (0.0–100.0 used), `config.currentPeriod {type:
USAGE_PERIOD_TYPE_WEEKLY|…MONTHLY, start, end}` (RFC 3339 — reset time =
`currentPeriod.end`), fallback `config.monthlyLimit`/`used`/
`billingPeriodStart`/`billingPeriodEnd`, and confirmed
`config.onDemandCap`/`onDemandUsed`/`prepaidBalance`. There is no current
top-level `billingCycle`/`monthlyLimit`/`usage` lane."

Research ch. 05, Q2: official code assigns
`BillingConfigResponse.subscription_tier =
RemoteSettings.subscription_tier_display.or(subscription_tier)` before the
ACP response is serialized. Decode only top-level `subscription_tier`;
`subscription_tier_display` is not a billing-response key.

Research ch. 10, Q4 (Grok window model): "`config.currentPeriod {type:
USAGE_PERIOD_TYPE_WEEKLY, start, end}` plus `history` of past periods =
consecutive fixed slots; window start is available on the RPC path (period
`start`) … on the web-scrape path only `reset_at` is available".

### Bucket building — where pace is currently `None`

Web fallback path, `grok.rs:193-220` (`GrokWebBillingSnapshot::buckets`) —
pace argument is `None` at grok.rs:215; the snapshot struct
(grok.rs:171-175) carries only `used_percent: f64` and
`reset_at_epoch: Option<i64>`:

```rust
        let mut view = timed_bucket(
            label,
            None,
            None,
            { /* remaining = 100 - used_percent.round() */ },
            self.reset_at_epoch,
            now,
            None,                       // ← grok.rs:215
            UsageSnapshotStatus::Fresh,
        );
        view.status_slot = Some(StatusSlot::Weekly);
```

Research ch. 05, Q4 on why the web path CANNOT have pace: "The jackin❯
protobuf scrape recovers only `used_percent` + `reset_at_epoch`
(grok.rs:553-600), so window duration (needed for any pace/deficit
computation) is unavailable there; the proxy/ACP path does provide
`currentPeriod.start`+`end`, from which the window is derivable."

RPC path, `grok.rs:223-263` (`GrokBillingResponse::buckets`, obsolete
`monthlyLimit` bucket) — pace argument is `None` at grok.rs:259; the
window IS derivable here via `grok.rs:306-311`:

```rust
    pub(crate) fn billing_period_minutes(&self) -> Option<i64> {
        let cycle = self.billing_cycle.as_ref()?;
        let start = parse_iso_epoch(cycle.billing_period_start.as_deref()?)?;
        let end = parse_iso_epoch(cycle.billing_period_end.as_deref()?)?;
        (end > start).then_some((end - start) / 60)
    }
```

Detail buckets "Included usage" / "On-demand usage" (grok.rs:265-298) are
emitted only when their value is `> 0` and stay `status_slot`-untagged.

Cycle-label helpers to reuse, `grok.rs:314-334`:
`grok_cycle_label_from_minutes(minutes)` → "Weekly" (6–8 days) /
"Monthly" (28–31 days) / "Credits"; `grok_cycle_label_from_reset`.

### The pace function (do NOT reimplement — call it)

`crates/jackin-usage/src/usage/format.rs:164-191` (in scope via
`use super::*`; imported into `usage.rs` at usage.rs:163):

```rust
pub(super) fn quota_pace_label(
    remaining_percent: Option<u8>,
    reset_at: Option<i64>,
    window_seconds: Option<i64>,
    now: i64,
) -> Option<String> {
```

Semantics (from its body): returns `None` when any input is missing or
`reset_in > window_seconds`; else "On pace" (|delta| ≤ 2), "N% in reserve"
(ahead), "N% in deficit" (behind), where
`delta = remaining_percent − reset_in/window_seconds×100`. Exemplar call
sites with a window: `claude.rs:506`, `codex.rs:697`, `kimi.rs:239`.

### Bucket constructors and the Money quota-bound slot

`crates/jackin-usage/src/usage/view.rs:566-589` `bucket(label, used_label,
limit_label, remaining_percent, reset_label, pace_label, status)`;
view.rs:611-632 `timed_bucket(..., reset_at, now, pace_label, status)`;
view.rs:596-602 `with_status_slot(view, slot)`.

`crates/jackin-protocol/src/control.rs:593-628` `QuotaBucketView` — the
monetary slots (doc comments verbatim): `used_money: Option<Money>`
("Structured spent amount behind `used_label`…") and
`limit_money: Option<Money>` ("Structured cap behind `limit_label`, when
monetary. `None` = uncapped."). `Money::new(amount_minor: i64, currency,
exponent: u8)` — control.rs:524. `Money` is already imported in `usage.rs`
(usage.rs:23) and therefore in scope in `grok.rs` via `use super::*`.

Money-from-cents exemplar: `claude.rs:710-711`
`Money::new((used * 100.0).round() as i64, "USD", 2)`; grok.rs's own
cents formatting is `format_cents(i64)` (format.rs:449-451, `$25` style —
existing Grok test proves `{ "val": 2500 }` → `"$25"`).

### Conventions to match

- Tests: all Grok tests live inline in
  `crates/jackin-usage/src/usage/tests.rs` (header `use super::*;`). No new
  test files — the crate rule is one `tests.rs` per module, no child
  modules. Exemplars: `grok_billing_response_maps_monthly_credits`
  (tests.rs:2464, serde_json fixture → `.buckets(now)` asserts),
  `grok_snapshot_uses_probe_success_without_local_credential_marker`
  (tests.rs:2562, `grok_snapshot_from_rpc_result` with `Ok(...)`),
  `grok_web_billing_response_maps_weekly_usage` (tests.rs:2594),
  tempfile-auth pattern at tests.rs:2529.
- Comments: non-obvious WHY only.
- Existing cast pattern for percent→u8:
  `#[expect(clippy::cast_sign_loss, reason = ...)]` around
  `100u8.saturating_sub(used.round() as u8)` (grok.rs:205-211, 249-256).
- Workspace lints are deny-heavy (`-D warnings` in CI): removing a function
  requires removing its `usage.rs` re-export in the same change.

## Commands you will need

Proven by research/jackin-desktop-verification-tooling/01-commands.md:

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests (CI-exact) | `rtk cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` | all pass, exit 0 |
| Focused tests | `rtk cargo nextest run -p jackin-usage --locked -E 'test(grok)'` | all matched pass |
| Lint | `rtk cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 |
| Format | `rtk cargo fmt --all -- --check` | exit 0 |
| Fast CI | `rtk cargo xtask ci --fast` | exit 0 |
| Docs | `(cd docs && rtk bunx tsc --noEmit && rtk bun test && rtk bun run build)` | all pass |
| Docs/repo audits | `rtk cargo xtask docs brand && env -u CI rtk cargo xtask docs specs && rtk cargo xtask docs repo-links && rtk cargo xtask roadmap audit && rtk cargo xtask research check` | all pass |

## Scope

**In scope** (the only files to create or modify):

- `crates/jackin-usage/src/usage/grok.rs` — all production changes.
- `crates/jackin-usage/src/usage/tests.rs` — new/updated Grok tests.
- `crates/jackin-usage/src/usage.rs` — remove `grok_plan_label` and every
  obsolete Grok type re-export deleted with the old top-level decoder; keep
  the block synchronized with the current model.
- `crates/jackin-usage/README.md`
- `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx`
- `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx`
- `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`
- `docs/content/docs/roadmap/index.mdx`
- `plans/jackin-desktop/README.md`
- `roadmap/jackin-desktop/README.md`
- `roadmap/README.md`

**Out of scope** (do NOT touch, even though related):

- Run-out math (`runs_out_in`, the composite
  `"<pace> · Runs out in <duration>"` label) — plan 004's territory. This
  plan only makes `window_seconds` reach `quota_pace_label`; 004 owns the
  composite emission, which may land at these same call sites.
- The web-scrape protobuf path (`parse_grok_web_billing_response`,
  `scan_protobuf`, `grpc_web_data_frames`, grok.rs:553-714) — untouched.
  The web path stays pace-less and plan-label-less by design.
- `config.history` and `config.productUsage` — do not decode; they are
  N3-banned history/trend surfaces. Current `config.onDemandCap`/
  `onDemandUsed` are in scope as quota-bound detail.
- Swift / `native/`, `crates/jackin-usage-ffi` (plan 005's generic
  Rust-produced limit-only presentation already makes this bucket visible;
  run its tests, change nothing).
- Other providers' modules; `crates/jackin-protocol`; capsule TUI (its
  dialog test at `crates/jackin-capsule/src/tui/components/dialog/tests.rs:1245`
  uses "SuperGrok" as hand-built sample text — display fixture, not the
  heuristic; leave it alone).

## Git workflow

- Branch: operator-chosen feature branch. If none exists yet, propose
  `feature/grok-server-plan-label` and wait for operator confirmation —
  never commit on `main`.
- Make exactly one signed Conventional Commit for this plan:
  `git commit -s -m "feat(usage): decode current Grok quota config" -m "Co-authored-by: Codex <codex@openai.com>"`
  Fix verification failures before committing; do not split follow-up
  commits because the final exact-scope proof uses `HEAD^..HEAD`.
- Push immediately. New same-name branch:
  `rtk git push -u origin HEAD`. Existing upstream:
  `rtk git push`; if local and remote head names differ, use
  `rtk git push origin HEAD:<remote-head>`. Verify destination first; no
  force pushes.

## Steps

### Step 1: Replace obsolete top-level decoder with current response

In `grok.rs`, replace `GrokBillingResponse`'s top-level
`billing_cycle/monthly_limit/on_demand_cap/usage` fields and obsolete
`GrokBillingCycle`/`GrokBillingUsage` types with:

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct GrokBillingResponse {
    pub(crate) config: Option<GrokBillingConfig>,
    pub(crate) on_demand_enabled: Option<bool>,
    pub(crate) subscription_tier: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrokBillingConfig {
    pub(crate) credit_usage_percent: Option<f64>,
    pub(crate) current_period: Option<GrokCurrentPeriod>,
    pub(crate) monthly_limit: Option<GrokCent>,
    pub(crate) used: Option<GrokCent>,
    pub(crate) on_demand_cap: Option<GrokCent>,
    pub(crate) on_demand_used: Option<GrokCent>,
    pub(crate) prepaid_balance: Option<GrokCent>,
    pub(crate) billing_period_start: Option<String>,
    pub(crate) billing_period_end: Option<String>,
}
```

`GrokCurrentPeriod` has
`#[serde(rename = "type")] period_type: Option<String>`,
`start: Option<String>`, and `end: Option<String>` so a partially omitted
object remains decodable and can fall back. Do not add fields/aliases for top-level `billingCycle`,
`monthlyLimit`, `usage`, or `subscription_tier_display`. Do not decode
`history`, `productUsage`, or `isUnifiedBillingUser`.

Add helpers returning parsed start/end, positive window seconds, and a
trimmed nonempty `subscription_tier`. Remove every obsolete re-export from
`usage.rs`. Web fallback remains unchanged.

Define the cents wire exactly as the official current model:

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct GrokCent {
    #[serde(default)]
    pub(crate) val: i64,
}
```

Proto3 JSON `{}` therefore means zero. Add one pure
`checked_cent_magnitude(i64) -> Option<i64>` helper using `checked_abs`;
`i64::MIN` is invalid and returns `None`. Use this helper for every monetary
quota field before comparisons, labels, arithmetic, or `Money::new`.

### Step 2: Build exactly one current headline with pace

`GrokBillingConfig::headline_bucket(now)` chooses:

1. preferred path when finite `credit_usage_percent` exists:
   clamp/round used percent, derive remaining, use `current_period` bounds
   and type;
2. otherwise fallback only when normalized `monthly_limit.val > 0`; decode
   absent or `{}` `used` as zero, derive remaining from those cents, and use
   `billing_period_start/end`.

Both paths feed exact positive `window_seconds`, `reset_at`, and remaining to
`quota_pace_label`; both emit at most one status headline. Period type maps
weekly/monthly, then duration helper, then `"Credits"`. Keep the current
Weekly status-slot convention. Web fallback stays pace-less because its
protobuf gives reset but no window length.

The preferred branch is eligible only when percent is finite and
`currentPeriod.start/end` parse to a positive period. Otherwise attempt the
fallback. The fallback is eligible only when its positive normalized limit
and `billingPeriodStart/End` also parse to a positive period. Missing,
malformed, equal, or reversed bounds cannot satisfy the pace-bearing
headline contract and emit no headline when neither complete branch exists.

`GrokBillingResponse::buckets` calls the helper once. Delete old headline
and detail construction from top-level types; never preserve a compatibility
lane or emit both preferred/fallback headline.

### Step 3: Emit current quota-bound detail

From the same `config`, emit:

- `Extra usage credits` when normalized `prepaid_balance.val > 0`:
  `limit_label`/`limit_money`, no `used_*`, no status slot;
- `On-demand usage` only when `on_demand_enabled != Some(false)` and the
  normalized cap is positive: used/cap labels and structured money, no
  status slot. Missing or `{}` used means zero. A missing/zero/invalid cap
  emits no row even when used exists: without a positive provider cap it is
  unbounded spend, not an N3-permitted quota bound.

Reuse `format_cents` and `Money::new(cents, "USD", 2)`. Do not emit
`Included usage`: current `config.used` participates in headline fallback
and is not a separate provider field. Do not decode `history` or
`productUsage`.

### Step 4: Use server-resolved tier; retire auth heuristic

`GrokBillingSnapshot::plan_label()` returns the RPC response's trimmed
`subscription_tier`; Web returns `None`. Wire snapshot plan label from the
billing result, delete `grok_plan_label`, remove its re-export, and remove
all deleted old-type re-exports. Production `grok.rs` must contain no
`eq_ignore_ascii_case("oidc")`, `"SuperGrok"` literal, or
`subscription_tier_display`.

Replace both existing obsolete-wire tests by name:
`grok_billing_response_maps_monthly_credits` becomes a current nested-config
fixture, and
`grok_snapshot_uses_probe_success_without_local_credential_marker` keeps its
probe-success purpose but uses the same current nested shape. No old
top-level fixture remains silently deserializable to an empty response.

Add all tests below, then:

```sh
rtk cargo nextest run -p jackin-usage --locked -E 'test(grok)'
rtk rg -n 'eq_ignore_ascii_case\\(\"oidc\"\\)|grok_plan_label|\"SuperGrok\"|subscription_tier_display|GrokBillingCycle|GrokBillingUsage' crates/jackin-usage/src/usage/grok.rs crates/jackin-usage/src/usage.rs
```

Expected: tests pass; `rg` exits 1.

### Step 5: Full gates, hub row, commit

1. Update docs:
   - operator guide: Grok plan comes from server truth; Extra usage credits
     and on-demand cap/used are quota bounds, not trends or prices;
   - usage README + ADR-011: server-tier precedence, generic
     remaining-balance bucket, and RPC-only pace inputs;
   - docs roadmap item/index: record this phase while keeping the overall
     Desktop item Partially implemented;
   - local roadmap/index: keep IN EXECUTION and append one plan-003 log;
   - hub row 003 → DONE only after gates.
2. `rtk cargo fmt --all -- --check` → exit 0.
3. `rtk cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
   → exit 0.
4. `rtk cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` → all
   pass.
5. `rtk cargo xtask ci --fast`, then the docs build and
   `rtk cargo xtask docs brand`,
   `env -u CI rtk cargo xtask docs specs`,
   `rtk cargo xtask docs repo-links`,
   `rtk cargo xtask roadmap audit`, and
   `rtk cargo xtask research check` → all exit 0.
   After setting hub row 003 DONE and writing roadmap status/log, rerun the
   docs build and all five docs/roadmap/research audits.
6. Stage exactly:

   ```sh
   git add -- \
     crates/jackin-usage/src/usage/grok.rs \
     crates/jackin-usage/src/usage/tests.rs \
     crates/jackin-usage/src/usage.rs \
     crates/jackin-usage/README.md \
     'docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx' \
     docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx \
     'docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx' \
     docs/content/docs/roadmap/index.mdx \
     plans/jackin-desktop/README.md \
     roadmap/jackin-desktop/README.md roadmap/README.md
   git diff --cached --name-only
   git diff --cached -- plans/jackin-desktop/README.md \
     roadmap/jackin-desktop/README.md roadmap/README.md
   ```

   Expected: exactly 11 paths; protocol diff contains row 003 plus narrow
   roadmap status/log only.
7. Commit and push per Git workflow.

**Verify**:

```sh
git log -1 --format=%B |
  grep -q '^Signed-off-by: .\+ <.\+>$'
git log -1 --format=%B |
  grep -qx 'Co-authored-by: Codex <codex@openai.com>'
test "$(git rev-parse HEAD)" = "$(git rev-parse '@{upstream}')"
diff -u \
  <(git diff --name-only HEAD^ HEAD | sort) \
  <(printf '%s\n' \
    crates/jackin-usage/README.md \
    crates/jackin-usage/src/usage.rs \
    crates/jackin-usage/src/usage/grok.rs \
    crates/jackin-usage/src/usage/tests.rs \
    'docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx' \
    docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx \
    'docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx' \
    docs/content/docs/roadmap/index.mdx \
    plans/jackin-desktop/README.md \
    roadmap/README.md \
    roadmap/jackin-desktop/README.md | sort)
test -z "$(git status --porcelain=v1)"
```

Expected: both trailer commands independently match, local/upstream SHAs are
equal, the sole plan commit contains exactly the 11 staged paths, and the
whole tree remains clean.

## Test plan

All tests live inline in `usage/tests.rs`. Use exact current wire keys.
At `now = 1_784_808_000`, weekly start/end
`1_784_505_600`/`1_785_110_400` leave 50% time.

1. `grok_current_config_maps_tier_weekly_and_quota_bounds`: fixture has
   30% used, weekly period, `prepaidBalance=2500`,
   `onDemandCap=5000`, `onDemandUsed=300`, and
   `subscription_tier="SuperGrok Heavy"`. Assert plan passes verbatim;
   one Weekly headline has 70% left and `"20% in reserve"`; prepaid row is
   `$25` only in limit slots; on-demand row is `$3` used of `$50` cap; both
   detail rows lack status slots.
2. `grok_nested_fallback_maps_one_headline_with_pace`: fixture omits
   percentage/currentPeriod and uses config `monthlyLimit=10000`,
   `used={}`, period start/end. Assert zero used, 100% remaining, correct
   reset/window pace, exactly one status headline, and no obsolete top-level
   fixture. A table subcase uses negative limit/used accounting values and
   proves checked magnitudes drive the same bounded percentage without
   overflow.
3. `grok_preferred_shape_suppresses_fallback_headline`: include both current
   preferred and fallback fields with different values; assert preferred
   remaining/period wins and exactly one status-slot bucket exists.
4. `grok_incomplete_periods_do_not_emit_paceless_headlines`: table-test
   preferred `currentPeriod` missing one bound plus malformed/reversed
   periods with a valid fallback
   (fallback wins), then both branches with missing/equal bounds (no
   headline).
5. `grok_signed_prepaid_balance_uses_safe_magnitude`: a negative
   `prepaidBalance.val=-2500` emits a `$25` remaining-limit row; `i64::MIN`
   emits none and cannot overflow.
6. `grok_on_demand_requires_enabled_positive_cap`: table cases prove
   enabled true and missing both emit for a positive cap; false suppresses;
   zero/missing cap suppresses even with used; and absent/`{}` used renders
   zero within a positive cap. Negative cap/used accounting values normalize
   safely to the same bounded row; `i64::MIN` never overflows or emits.
7. `grok_plan_label_not_guessed_from_oidc_auth_mode`: OIDC test auth plus Web
   snapshot yields no plan label.
8. `grok_plan_label_uses_only_trimmed_resolved_tier`: absent/blank tier is
   `None`; one top-level `"subscription_tier":"X Premium+"` passes unchanged.
   No `subscription_tier_display` fixture exists.
9. `grok_web_path_emits_no_pace`: web percent/reset produces no pace because
   window duration is unavailable.

`rtk cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` must
pass all nine plus existing Grok tests after both obsolete fixtures are
replaced.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` exits 0;
      all nine named current-wire tests exist and pass
- [ ] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exits 0
- [ ] `cargo fmt --all -- --check`, `cargo xtask ci --fast`, and every docs
      gate exit 0
- [ ] Heuristic retired: production-only `rg` from Step 4 has no matches;
      server-truth test fixtures may contain `"SuperGrok"`
- [ ] Current bounds exist:
      `rg -n "Extra usage credits|On-demand usage" crates/jackin-usage/src/usage/grok.rs`
      shows both generic quota rows.
- [ ] No stale/banned decode:
      `rg -n "subscription_tier_display|billingCycle|productUsage|history" crates/jackin-usage/src/usage/grok.rs`
      → no matches.
- [ ] Whole tree began/ends clean; `git diff --name-only HEAD^ HEAD` equals
      the exact 11-path Step-5 allowlist
- [ ] Every commit is signed (`-s`), contains
      `Co-authored-by: Codex <codex@openai.com>`, and is pushed
- [ ] `plans/jackin-desktop/README.md` row 003 status updated

## STOP conditions

Stop and report back (do not improvise) if:

- Any precondition fails, or a "Starting state" excerpt does not match the
  live code (drift).
- The current `grok` CLI's `x.ai/billing` response demonstrably lacks the
  researched top-level `config`/`subscription_tier` contract —
  e.g. a live probe or operator capture contradicts the researched shape.
  Report; do NOT guess alternative field names or casings.
- Current official source changes any named wire type away from the verified
  `{val}` cents/current-period model; re-research and regenerate rather than
  inventing aliases.
- Any step drifts toward N3 territory: rendering the prepaid balance as
  spend (`used_money`), a token price, a cost estimate, or decoding
  `history`/`productUsage`. That temptation is itself a STOP.
- A verification remains impossible after inspecting its implementation,
  testing bounded alternatives, and proving a tool/project limit.
- The work requires touching an out-of-scope file or violating Must NOT.

## Maintenance notes

- **Plan 004 boundary**: this plan delivers `window_seconds` into
  `quota_pace_label` on the RPC path. Plan 004 (run-out producer) computes
  `runs_out_in` and appends the composite
  `"<pace> · Runs out in <duration>"` — likely at these same call sites.
  Do not pre-build any of that here.
- Reviewer scrutiny: (1) N3 framing of the "Extra usage credits" bucket —
  balance must sit in `limit_label`/`limit_money`, never `used_*`;
  (2) serde field names against the researched wire shape (no invented
  aliases); (3) exactly one `StatusSlot::Weekly` bucket per response shape.
- Current `config.onDemandCap`/`onDemandUsed` are confirmed and in scope;
  never defer them or resurrect obsolete top-level fields.
- The capsule TUI dialog test/snapshot
  (`crates/jackin-capsule/src/tui/components/dialog/tests.rs:1245` and its
  `.snap`) uses "SuperGrok" as hand-authored sample plan-label text — that
  is display-fixture data unrelated to the retired heuristic; it must not
  be edited by this plan.
- The web-scrape protobuf heuristics (grok.rs:553-600) remain the fallback
  path's only data source; if xAI's web proto ever exposes period bounds,
  pace on the web path becomes possible — new plan, not this one.
