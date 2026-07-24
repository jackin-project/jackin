# 05 — Grok usage API

Questions: (1) Which endpoint(s) expose subscription usage/limits for SuperGrok-style plans — billing-cycle/weekly window, percent left, reset time? (2) Is the plan label ("SuperGrok") an API field or client-side inference? (3) What auth is used and where is it stored on disk (location/type only)? Does the primary evidence confirm jackin's "billing cycle only, no session window" note (grok.rs:198)? (4) Which display elements (deficit %, "Runs out in" projection) have no direct API source and need client-side computation?
Informs: jackin-desktop
Method: web + codebase cross-reference; current official billing source
rechecked after plan cold review at xai-org/grok-build main
Vetted: 2026-07-24

Primary-source situation changed in July 2026: xAI open-sourced the official `grok` CLI/agent runtime as [xai-org/grok-build](https://github.com/xai-org/grok-build) ("synced periodically from the SpaceXAI monorepo" — the repo's own branding; xAI org on GitHub). Its billing extension, auth model, and user guide are now first-party evidence for everything below. Clean-room constraint respected: no CodexBar or OpenUsage source was read; one non-banned third-party client (Raycast `agent-usage` extension) is cited as corroboration only. No embedded instructions were found in any fetched content.

## Findings

### Q1 — Endpoints exposing SuperGrok subscription usage/limits

- **Consumer usage model (2026): one shared weekly usage pool per subscription, replacing per-product daily limits.** "Instead of separate daily limits for each product (like Chat, Imagine, Voice, or Build), you get one shared weekly usage pool"; the in-product view (Settings → Usage on grok.com web/mobile) shows "a progress bar showing your current usage percentage", "a percentage breakdown by product (API, Build, Chat, Imagine, Voice)", "your weekly reset date and time", and an "Extra Usage Credits balance". — https://docs.x.ai/grok/faq (confidence: HIGH)
- **Official CLI billing endpoint: `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`**, Bearer-authenticated, forwarding to backend RPC `GetGrokCreditsConfig`. Proxy base default `"https://cli-chat-proxy.grok.com/v1"` — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-env/src/lib.rs#L23; request construction and "forwards to the backend `GetGrokCreditsConfig`" comment — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/extensions/billing.rs#L207-L237; doc comment also names it "primarily from `GET /rest/grok/credits`" (backend-side path) — same file #L111. (confidence: HIGH)
- **Current `x.ai/billing` response:** top-level `config`,
  `on_demand_enabled`, and `subscription_tier`. One `config` object carries
  preferred `creditUsagePercent` (0.0–100.0 used) and
  `currentPeriod {type: USAGE_PERIOD_TYPE_WEEKLY|…MONTHLY, start, end}`;
  its current fallback fields are `monthlyLimit`, `used`,
  `billingPeriodStart/End`. The same object carries confirmed
  `onDemandCap`, `onDemandUsed`, `prepaidBalance`,
  `isUnifiedBillingUser`, plus excluded `productUsage`/`history`.
  There is no top-level `billingCycle`/`monthlyLimit`/`usage` object in the
  official current model. —
  https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/extensions/billing.rs#L60-L124
  and its credits/fallback tests. (confidence: HIGH)
- **Cent decoding is signed and zero-safe.** The official `Cent` wire type is
  `val: i64` with `#[serde(default)]`, so proto3 JSON `{}` is a valid zero
  amount. Billing accounting values can be negative; the official display
  normalizes their magnitude before rendering. A consumer must use a
  checked absolute-value conversion (rejecting `i64::MIN`) rather than
  treating a negative credit balance as absent. —
  https://github.com/xai-org/grok-build/blob/69f0ba880aa98f55e3ac1dcc570e2f332f825fe2/crates/codegen/xai-grok-shell/src/extensions/billing.rs
  and
  https://github.com/xai-org/grok-build/blob/69f0ba880aa98f55e3ac1dcc570e2f332f825fe2/crates/codegen/xai-grok-pager/src/views/credit_bar.rs
  (confidence: HIGH)
- **On-demand is quota-bound only when a positive cap exists.**
  `on_demand_enabled=false` hides on-demand controls in the official client;
  true or missing permits a row only when the normalized cap is positive.
  Used-without-cap and zero-cap responses are not bounded quotas and must not
  be surfaced. Missing or `{}` used decodes as zero inside a valid cap. —
  https://github.com/xai-org/grok-build/blob/69f0ba880aa98f55e3ac1dcc570e2f332f825fe2/crates/codegen/xai-grok-shell/src/extensions/billing.rs
  (confidence: HIGH)
- **Percent left is not a field; used-percent is.** Official CLI displays `usage_pct.floor()` as "Weekly limit: N%" plus "Next reset: <time>"; percent-left is the complement, computed client-side. — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/views/credit_bar.rs#L106-L152 (confidence: HIGH)
- **ACP surface for third parties:** the CLI exposes the same data over Agent Client Protocol stdio as extension methods `x.ai/billing` and `x.ai/auto-topup-rule` (auto top-up rule fetched from `{proxy}/auto-topup-rule`). — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/extensions/billing.rs#L148-L161, #L291-L346. jackin consumes exactly this: spawns `grok agent stdio`, calls `initialize` then `x.ai/billing` — /Users/donbeave/Projects/jackin-project/jackin/crates/jackin-usage/src/usage/grok.rs:355-437 (method string at :420). (confidence: HIGH)
- **grok.com web (Settings → Usage) endpoint: `POST https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig`** (gRPC-web+proto, Bearer). Evidenced by jackin's working fallback probe — grok.rs:449-506 (URL at :468, Referer `https://grok.com/?_s=usage` at :471) — and independently by the Raycast `agent-usage` extension using the identical URL + Bearer-from-`~/.grok/auth.json` — https://github.com/raycast/extensions/blob/main/extensions/agent-usage/src/grok/fetcher.ts#L14. RPC name `GetGrokCreditsConfig` confirmed first-party (billing.rs above); the grok.com gRPC-web route itself is undocumented by xAI. (confidence: MED — works today, no official contract)
- **No public/consumer usage-quota REST doc exists on docs.x.ai.** Developer Rate Limits page covers API-key tiers only ("Your tier is based on cumulative spend on the xAI API since January 1, 2026") and offers no programmatic quota endpoint, only the console page. — https://docs.x.ai/developers/rate-limits. Management API billing endpoints (`/v1/billing/teams/{team_id}/…` on `management-api.x.ai`) are team API billing — invoices, spending limits, prepaid balance — not SuperGrok subscription windows. — https://docs.x.ai/developers/rest-api-reference/management/billing (confidence: HIGH)
- **Server can redirect usage display entirely:** remote settings `usage_billing_redirect_url` makes `/usage` show a link instead of fetching billing (feature-flag `grok_build_usage_redirect_url`). — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-config-types/src/lib.rs#L983-L989 (confidence: HIGH)

### Q2 — Plan label ("SuperGrok"): API field or client-side inference?

- **API field, server-set.** `RemoteSettings.subscription_tier_display` — "User-friendly display name for the current subscription tier (e.g. \"SuperGrok\", \"X Premium+\", \"Free\", \"API Key\"). Set by CCP from the JWT tier claim (OAuth) or credential kind (API key)." Fetched from cli-chat-proxy `GET /v1/settings` (struct doc: "Remote settings fetched from cli-chat-proxy `GET /v1/settings`"). — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-config-types/src/lib.rs#L438, #L972-L978; machine-readable sibling `subscription_tier` ("free", "premium", "supergrok", "supergrok_heavy") #L952-L956. (confidence: HIGH)
- The ACP `x.ai/billing` response re-exports one already-resolved field:
  `BillingConfigResponse.subscription_tier`. Before serialization, official
  code sets it to
  `RemoteSettings.subscription_tier_display.or(subscription_tier)`.
  Consumers must read this top-level field only; a separate
  `subscription_tier_display` key is not present in the billing response. —
  https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/extensions/billing.rs#L119-L124,
  #L271-L278 (confidence: HIGH)
- **Fallback chain when `/settings` is silent:** (1) `subscription_tier_display`, (2) API-key credential → `"api_key"`, (3) numeric `tier` claim decoded from the JWT access token: 0=free, 1=supergrok, 2=x_basic, 3=x_premium, 4=x_premium_plus, 5=supergrok_heavy, 6=supergrok_lite. — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/agent/mvp_agent/mod.rs#L117-L161. Live-tier check endpoint `GET {proxy}/user?include=subscription` returns tier strings `SuperGrokPro`, `GrokPro`, `SuperGrokLite`, `XPremiumPlus`, `XPremium`, `XBasic`. — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/agent/subscription_check.rs#L22-L29, #L81 (confidence: HIGH)
- **jackin's current label is a client-side inference that diverges from all of the above:** `grok_plan_label` maps `auth.json` `auth_mode == "oidc"` → `"SuperGrok"` — /Users/donbeave/Projects/jackin-project/jackin/crates/jackin-usage/src/usage/grok.rs:793-802. Per the official model, `auth_mode` records login *method*, not plan ("Token provenance (debugging/auth.json only -- no code branches on this)"; `Oidc` covers browser OAuth and enterprise IdP alike), so a Free-tier browser login also carries `oidc`. — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/auth/model.rs#L26-L40 (confidence: HIGH)

### Q3 — Auth: type, on-disk location; "billing cycle only, no session window"

- **Credential store: `~/.grok/auth.json`** (owner-only `0600`; MCP OAuth tokens separately in `~/.grok/mcp_credentials.json`). Default flow is browser OAuth at `auth.x.ai`; device-code and external-provider flows write the same file; hot-reloaded on change. — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md (confidence: HIGH)
- **File format: JSON map scope-string → credential object** (`AuthStore = BTreeMap<String, GrokAuth>`), entries carrying `key` (Bearer JWT), `refresh_token`, `expires_at`, `auth_mode` (`web_login`/`oidc`/`external`/`api_key`), `user_id`, `email`, team/org fields. Scope keys: OIDC issuer-derived (auth.x.ai), legacy `"https://accounts.x.ai/sign-in"`, API-key `"xai::api_key"`. — https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/auth/model.rs#L11-L14, #L47-L95, #L259. jackin's parser matches this: scans `https://auth.x.ai::`-prefixed and legacy `/sign-in` scopes for `key` + `expires_at` — grok.rs:508-531. Types/locations only; no values reproduced. (confidence: HIGH)
- **Precedence:** per-model key > session token from `auth.json` > `XAI_API_KEY` env fallback. — 02-authentication.md ("Auth Precedence"). jackin additionally probes `XAI_API_KEY`/`GROK_DEPLOYMENT_KEY` env presence (grok.rs:29-30) and a container handoff copy of auth.json (`GROK_HANDOFF_AUTH_PATH`, usage.rs:186). (confidence: HIGH)
- **Bearer JWT suffices for billing — no session cookie required.** Official CLI sends `Authorization: Bearer` (+ `X-XAI-Token-Auth`, `x-userid`) to the proxy billing endpoint — billing.rs#L213-L226; jackin (grok.rs:465-477) and Raycast (fetcher.ts#L91-L97) both hit the grok.com gRPC-web route with Bearer only. (confidence: HIGH for proxy; MED for the grok.com web route, unofficial)
- **grok.rs:198 claim — confirmed for "no session window", refined for "billing cycle":** no session-scoped (rolling hours) quota window exists anywhere in the official billing surface; the quota window is the usage period — now typically **weekly** (`USAGE_PERIOD_TYPE_WEEKLY`, unified consumer pool) with a monthly variant, plus the deprecated monthly billing cycle. Official label choice is driven by the period-type enum ("Weekly limit"/"Monthly limit", fallback "Usage") — credit_bar.rs#L38-L47 — whereas jackin infers Weekly/Monthly from window duration or reset distance because the scraped web protobuf lacks the enum (grok.rs:314-334). jackin's own comments already treat the cycle as the Weekly-slot filler (grok.rs:198-199, :242). (confidence: HIGH)

### Q4 — Display elements with NO direct API source (client-side computation)

- **Deficit/reserve pace ("N% in deficit" / "N% in reserve" / "On pace"):** no Grok API field. jackin computes it by comparing remaining-percent against the fraction of the window still left — /Users/donbeave/Projects/jackin-project/jackin/crates/jackin-usage/src/usage/format.rs:164-191. The official CLI ships no pace concept at all (credit bar shows usage %, next reset, credits, PAYG only — credit_bar.rs#L106-L152). Grok buckets in jackin currently pass no pace label (`None` at grok.rs:215 web branch, :259 RPC branch). (confidence: HIGH)
- **"Runs out in" projection:** no Grok API field supplies burn rate, projected depletion time, or equivalent; `GetGrokCreditsConfig` carries only point-in-time used-percent + period bounds (+ per-period history). Any runout estimate must be extrapolated client-side. Desktop renders it only if the Rust side ever emits it in `pace_label` — native/Sources/JackinDesktop/UsageWindow/ProviderCardView.swift:209-229. (confidence: HIGH)
- **Percent left:** derived (100 − used); official CLI computes the complement of floored usage (credit_bar.rs#L208-L214), jackin does `100 - used.round()` (grok.rs:210, :254). (confidence: HIGH)
- **Window length for pace math on the web-scrape path:** jackin's protobuf scrape recovers only `used_percent` + `reset_at_epoch` (grok.rs:553-600), so window duration (needed for any pace/deficit computation) is unavailable there; the proxy/ACP path does provide `currentPeriod.start`+`end`, from which the window is derivable. (confidence: HIGH)
- **Available in API but unused for display:** per-period `history` and per-product `productUsage` exist in the credits config (billing.rs#L107-L108, #L567-L569 — "productUsage is still unused by the CLI billing surface"); jackin's limits-only product rule excludes historical trend surfaces regardless (crates/jackin-usage/CLAUDE.md hard rule). (confidence: HIGH)

## Dead ends and contradictions

- **docs.x.ai has no consumer-subscription usage API.** Rate-limits and consumption pages are developer-API-tier only; Management API (`management-api.x.ai`) is team API billing. Checked and ruled out as a SuperGrok usage source. — https://docs.x.ai/developers/rate-limits, https://docs.x.ai/developers/rest-api-reference/management/billing
- **Third-party blog limit tables (jingrey.com etc.) claiming fixed daily message counts (e.g. "300–500 texts/24h") are stale/unsourced** — they describe the pre-2026 per-product daily-limit era; the official FAQ now documents the unified weekly pool. Dropped as evidence.
- **`auth_mode` as plan signal contradicted:** official auth model marks it token-provenance-only ("no code branches on this"), so jackin's `oidc → SuperGrok` mapping (grok.rs:793-802) can mislabel Free/X-Premium browser logins. Recorded under Q2; no recommendation made per scope.
- **Branding drift, not a contradiction:** the repo prose says "SpaceXAI" while org/domain remain xai-org / x.ai / grok.com; quoted verbatim where relevant.
- **GitHub-wide search for `GrokBuildBilling`** (excluding the clean-room-banned repos) shows ~147 third-party hits using the same grok.com gRPC-web route — corroborates it is the de-facto web endpoint, but none are primary sources; not cited as evidence beyond the one Raycast corroboration.

## Open unknowns

- **Live response shape of `grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig` (protobuf field numbers).** jackin's scraper keys on heuristics (fixed32 percent at a path ending in field 1; varint epoch, preferred path `[1,5,1]` — grok.rs:564-595). Whether these match the current proto and whether the response now carries the `USAGE_PERIOD_TYPE_*` enum and `productUsage` per-product split needs an **operator-authenticated browser session inspecting network traffic** on grok.com `Settings → Usage` (`/?_s=usage`).
- **Full server contract of `GET https://cli-chat-proxy.grok.com/v1/settings`** — only the client-side `RemoteSettings` struct is public (config-types lib.rs); undocumented fields and stability guarantees unknown. Same for `/v1/billing?format=credits` availability to non-official clients (no published API contract; header set includes `X-XAI-Token-Auth` and `x-grok-client-version`, gating behavior unverified).
- **Whether consumer chat surfaces retain any separately queryable per-product/per-model rate-limit endpoint** post-unification (for non-Build products like Imagine/Voice), or whether everything folds into the credits config percent + `productUsage`. No primary source found; needs authenticated grok.com network inspection.
- **Weekly vs monthly period assignment per plan** (which tiers get `USAGE_PERIOD_TYPE_WEEKLY` vs `…MONTHLY`, and whether `is_unified_billing_user=false` legacy monthly+on-demand accounts still exist in the wild) — the FAQ describes the weekly pool generally; per-tier mapping unverified.
- **JWT `tier` claim contract** (claim name `tier`, numeric values 0–6) is evidenced only by the client decoder (mvp_agent/mod.rs#L117-L141); no token-format spec published. Values could change server-side without notice.
