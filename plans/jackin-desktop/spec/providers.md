# Provider data core (Rust)

## Purpose

Rust-side provider data: auto-detection of enabled providers, credential
resolution (incl. macOS Keychain for Claude), provider-core correctness
fixes, the Amp Free daily parser, the run-out producer, and Grok server plan/credits. Everything here
feeds the display surfaces through existing DTOs; Swift never derives any of
it (N1).
Anchors: F3, F5, F6, F7, F11, F12, W5 · Evidence:
research/agent-usage-provider-apis/01–11

## Requirements

### Requirement: Auto-detected enabled providers
The host runtime SHALL treat a provider as enabled exactly when its
credentials/usage data are resolvable on this host (per-provider resolution
order already implemented in `crates/jackin-usage`, ch. 01 Q2), with no
user toggle; the enabled set SHALL be re-evaluated on every refresh so a
new login is picked up without app restart.
Covers: F3, W5 · Evidence: research/agent-usage-provider-apis/01-jackin-usage-current-coverage.md

#### Scenario: New credential appears between refreshes
- **GIVEN** the app is running and Kimi had no resolvable credential
- **WHEN** the operator logs into the Kimi CLI and the next refresh runs
- **THEN** Kimi joins the enabled set, its status bar item appears, and its popover tab shows data
- **AND** no restart was required

#### Scenario: No credentials at all
- **GIVEN** a host with zero resolvable provider credentials
- **WHEN** the app starts
- **THEN** the enabled set is empty, the popover shows the empty state (S10), and the status item remains present

### Requirement: Claude credential from macOS Keychain
Claude credential resolution SHALL, on macOS, derive the generic-password
service from the effective `CLAUDE_CONFIG_DIR`: exact
`Claude Code-credentials` for the default `~/.claude`, otherwise
`Claude Code-credentials-<sha256(normalized-absolute-config-path)[..8]>`.
Custom configs SHALL never read default-home credential/account metadata, and
their service/account identity SHALL scope local cache, shared snapshots,
locks, and cooldowns. The payload is
the same `claudeAiOauth` JSON as the credentials file and SHALL use the one
existing parser (A2). Keychain resolution SHALL happen BEFORE any file/env
credential read, shared adoption, account identity, lock, or cooldown.
Interactive acquisition SHALL complete before provider-probe timeout and
shared coordination; reads SHALL serialize, and an
explicit operator denial SHALL be terminal for that service for the process
lifetime without affecting other providers. A headless
`errSecInteractionNotAllowed` result is absence, not denial, so existing
file/env fallback remains available. jackin❯ Desktop SHALL serialize every
blocking bridge access off `@MainActor` and coalesce overlapping refresh
requests so a consent sheet cannot freeze the menu-bar UI or create a prompt
storm.
Covers: F6, W5 · Evidence: research/agent-usage-provider-apis/09-claude-followups.md (Q1)

#### Scenario: Default macOS install
- **GIVEN** Claude Code logged in on macOS (Keychain-only, no credentials file)
- **WHEN** the app resolves Claude credentials
- **THEN** the Keychain item is read once for that refresh wave and Claude becomes enabled

#### Scenario: Custom Claude config directory
- **GIVEN** `CLAUDE_CONFIG_DIR` points to a non-default absolute path
- **WHEN** the app resolves Claude credentials
- **THEN** it queries the path-derived suffixed service used by Claude Code, never the default account

#### Scenario: Config scope changes while running
- **GIVEN** valid default, custom-A, and custom-B credentials/cache entries
- **WHEN** effective `CLAUDE_CONFIG_DIR` changes between refresh waves
- **THEN** each wave uses only its normalized service/account scope and no
  default/custom cache, lock, cooldown, or metadata crosses into another

#### Scenario: Consent denied
- **GIVEN** the operator denies the Keychain prompt
- **WHEN** resolution completes
- **THEN** Claude is not enabled, cached quota is not restored, file/env
  fallback is not read, no retry-prompt storm occurs, and all other providers
  are unaffected

#### Scenario: File still present (Linux/container parity)
- **GIVEN** Linux/container execution, a missing item, or headless macOS where
  Keychain interaction is unavailable, and a credentials file exists
- **WHEN** resolution runs
- **THEN** the file path resolves exactly as today (no regression)

### Requirement: Run-out producer (Variant A)
The Rust core SHALL compute, per quota bucket where `remaining_percent`,
`resets_at`, and window duration are known and `used > 0`,
`runs_out_in = remaining × elapsed / used` under the linear-from-window-start
model anchored on `resets_at` (window_start = resets_at − window_seconds, A1),
and SHALL emit it appended to the existing pace label as the composite
`"<pace> · Runs out in <compact duration>"` only when the projected run-out
precedes the reset; when it does not, the existing "Lasts until reset"
semantics remain (TUI synthesis stays valid). Zero-used or window-start
edge cases SHALL emit no run-out segment.
Covers: F5 · Evidence: research/agent-usage-provider-apis/07-runout-projection-semantics.md, 10 (Q1/Q4)

#### Scenario: Behind pace, runs out before reset
- **GIVEN** a Weekly bucket at 48% left, 5% in deficit, reset in 3d 16h
- **WHEN** the bucket view is built
- **THEN** `pace_label` reads "5% in deficit · Runs out in <duration>" with duration < 3d 16h
- **AND** the capsule dialog and Swift splitter render the two segments in their existing columns unchanged

#### Scenario: Ahead of pace
- **GIVEN** a bucket in reserve (delta > 0)
- **WHEN** the view is built
- **THEN** no "Runs out in" segment is emitted and the TUI still shows "Lasts until reset"

#### Scenario: Nothing used yet
- **GIVEN** a bucket at 100% left
- **WHEN** the view is built
- **THEN** the pace label carries no run-out segment (no division by zero)

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
Proto3 `{}` cent objects SHALL decode as zero; signed credit accounting
SHALL use checked magnitude normalization. On-demand SHALL render only when
not explicitly disabled and a positive provider cap exists; used without a
positive cap is not a permitted quota-bound surface.
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
  monthly limit, zero/default used, and billing-period bounds exist
- **WHEN** the Grok view is built
- **THEN** one headline bucket renders with pace inputs; no obsolete
  top-level billing lane or duplicate headline is used

#### Scenario: On-demand has no positive cap
- **GIVEN** on-demand is disabled, the cap is missing/zero, or only used
  amount exists
- **WHEN** the Grok view is built
- **THEN** no on-demand row renders because no positive provider quota bound
  exists

#### Scenario: Free account over browser OAuth
- **GIVEN** a Free-tier login whose `auth.json` has `auth_mode: "oidc"`
- **WHEN** the view is built
- **THEN** the label is NOT "SuperGrok" (server truth or empty)

### Requirement: Provider-core correctness fixes
The Codex RPC account decoder SHALL accept upstream wire tags `"apiKey"`
(camelCase) and `"amazonBedrock"` (and keep `"chatgpt"`), and an account
decode failure SHALL degrade to no account label instead of failing the
usage result; the MiniMax URL fan-out SHALL include the officially
documented `https://www.minimax.io/v1/token_plan/remains`; the z.ai quota
deserializer SHALL map the observed `data.level` field as a plan-label
source (alias alongside the existing names).
Covers: F11 · Evidence: research/agent-usage-provider-apis/08-codex-followups.md (Q2), 06 (MiniMax/z.ai), 01 (round-2 addenda)

#### Scenario: Codex API-key account
- **GIVEN** `account/read` returns `{"type":"apiKey"}`
- **WHEN** the RPC usage result is decoded
- **THEN** rate limits are returned with no decode error
- **AND** the account label is the existing API-key origin label (or absent) — never a failure

#### Scenario: z.ai pro plan
- **GIVEN** a quota response with `data.level: "pro"` and no `planName`
- **WHEN** the view is built
- **THEN** the plan label renders from `level`

#### Scenario: MiniMax documented host
- **GIVEN** the `api.*` hosts fail and `www.minimax.io` succeeds
- **WHEN** the fan-out runs
- **THEN** usage is returned from the documented host

### Requirement: Current Amp Free daily usage
The Amp API and CLI paths SHALL use one shared parser for the current
`userDisplayBalanceInfo.displayText` contract: account identity, `Amp Free:
N% remaining today (resets daily)`, individual credit balance, and repeated
`Workspace <name>` credit balances. The Amp Free bucket SHALL carry a
semantic Daily slot, the parsed remaining percentage, and the exact
`Resets daily` cadence with no fabricated reset timestamp. The parser MUST
NOT retain the retired hourly-dollar reader, speculative structured-key
fallbacks, or replenishment-derived reset math. Plan label `Amp Free` SHALL
exist only when the daily line exists; paid-only balances SHALL not infer a
plan or a daily percentage.
Covers: F12 · Evidence:
research/agent-usage-provider-apis/04-amp-usage-api.md,
11-amp-daily-followup.md

#### Scenario: Current Amp Free account
- **GIVEN** `displayText` contains `Amp Free: 61% remaining today (resets daily)`
- **WHEN** the Amp view is built
- **THEN** its Amp Free bucket has 61% remaining, semantic Daily slot,
  `Resets daily`, and no exact `resets_at`

#### Scenario: Current workspace balances
- **GIVEN** the same text includes individual credits and two
  `Workspace <name>: $N remaining` lines
- **WHEN** the view is built
- **THEN** all three provider-supplied quota bounds render in source order
  without becoming status-item percentages

#### Scenario: Paid-only text has no Amp Free line
- **GIVEN** a successful response contains credit balances but no Amp Free
  daily line
- **WHEN** the view and glance projection are built
- **THEN** detail surfaces retain the balances, plan label is absent, and
  the Amp glance percentage is unavailable rather than inferred

## Notes

- z.ai auth header stays Bearer (A3); a live 401 falsifies A3 → the fix is a
  raw-key retry fallback, flagged to the operator (plans inline this STOP).
- All new labels/strings are produced in Rust (N1); limits-only rule (N3)
  binds every new bucket (prepaid balance is a quota bound, never a price).
