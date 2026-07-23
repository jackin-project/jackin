# Agent usage provider APIs

Vetted: 2026-07-24 · Chapters 01–11 all vetted (round 1: 01–07; `--deep`
round 2: 08–10 + corrections into 01/02/03/04/07; operator-directed Amp
daily follow-up: 11). Informs:
[roadmap/jackin-desktop](../../roadmap/jackin-desktop/README.md).

Question set: (a) per provider, which API exposes the availability data a CodexBar-style
menu bar shows, and which displayed elements have no direct API source; (b) semantics of
the run-out projection phrases and what jackin❯ already computes; (c) what
`crates/jackin-usage/` already fetches vs. gaps.

## Headline conclusions

1. **Every provider in scope has a workable availability source, and jackin❯ already
   calls the right endpoint for six of seven** —
   [01](01-jackin-usage-current-coverage.md). Per provider:
   - **Codex**: `GET {base}/wham/usage` (+ `/wham/rate-limit-reset-credits`, consume
     POST) with ChatGPT OAuth Bearer; windows carry `used_percent`,
     `limit_window_seconds`, `reset_after_seconds`, `reset_at`; Spark rides
     `additional_rate_limits[]`; credits + reset-credits are separate concepts. All
     first-party-sourced from openai/codex — [02](02-codex-usage-api.md),
     [08](08-codex-followups.md).
   - **Claude**: de-facto `GET api.anthropic.com/api/oauth/usage` (OAuth Bearer +
     `anthropic-beta: oauth-2025-04-20` + `claude-code/<ver>` UA); per-window
     `utilization` + `resets_at` only; `limits[]` carries per-model weekly
     (`weekly_scoped`, display name "Fable"); no official API, endpoint 429s hard;
     fields multi-source corroborated — [03](03-claude-usage-api.md),
     [09](09-claude-followups.md).
   - **Amp**: single RPC `POST ampcode.com/api/internal?userDisplayBalanceInfo`
     returning ONE server-rendered prose string (`displayText`); current Amp
     Free output is a daily remaining percentage plus individual/workspace
     balances, all client-parsed — [04](04-amp-usage-api.md),
     [11](11-amp-daily-followup.md).
   - **Grok**: official CLI now open-source (xai-org/grok-build): proxy
     `GET cli-chat-proxy.grok.com/v1/billing?format=credits` / ACP `x.ai/billing`;
   consumer model = one shared weekly pool, fixed weekly reset; the billing
   response's `subscription_tier` is already resolved display-first by the
   server/official client — [05](05-grok-usage-api.md).
   - **z.ai / MiniMax / Kimi**: bearer/raw-key GETs; MiniMax is the only officially
     documented quota API of the three (`www.minimax.io/v1/token_plan/remains`) —
     [06](06-zai-minimax-kimi-usage-apis.md).

2. **The pace/projection phrase family is CodexBar's UI vocabulary, not API data.**
   "Runs out in", "Lasts until reset" confirmed verbatim on codexbar.app; "Projected
   empty" in CodexBar release notes; OpenUsage uses "~N% left at reset" instead. NO
   provider API supplies burn rate, projection, or history for these — all
   client-computed — [10](10-phrase-provenance-and-misc.md),
   [07](07-runout-projection-semantics.md).

3. **jackin❯ already ships the pace math; the run-out family has a consumer but no
   producer.** `quota_pace_label` (deficit/reserve/on-pace) is live; the capsule TUI
   synthesizes "Lasts until reset" from the pace token; the Swift shell and capsule
   dialog both split/route "Runs out in …" — but nothing in Rust emits it.
   Projection formulas derivable from existing inputs (Variant A linear-from-window-
   start needs zero new data; its lasts-until-reset boolean is algebraically the sign
   of the existing pace delta). Sampled-burn variants blocked: snapshot store is
   latest-only — [07](07-runout-projection-semantics.md). Window anchoring is
   fixed-slot (not rolling) for Claude weekly, Grok weekly, Codex (client-observable);
   anchor on `resets_at` — [09](09-claude-followups.md) Q3,
   [10](10-phrase-provenance-and-misc.md) Q4.

4. **macOS credential blocker for Claude (Desktop-critical):** default macOS Claude
   Code stores OAuth credentials Keychain-ONLY (service `Claude Code-credentials`) and
   actively unlinks `~/.claude/.credentials.json`; jackin❯ has no Keychain reader, so
   its Claude probe finds nothing on a default macOS host. Codex has no such hole
   (default store = `~/.codex/auth.json` file) — [09](09-claude-followups.md) Q1,
   [08](08-codex-followups.md) Q3. `claude setup-token` tokens are rejected by the
   usage endpoint (`user:inference` scope only) — only `/login` credentials work.

5. **Concrete jackin❯ gaps found** (question c):
   - Run-out/projection producer absent (above).
   - Amp current parser: jackin❯ still expects the retired
     `$remaining/$limit + hourly replenishment` line; current Amp Free is
     `N% remaining today (resets daily)`. Workspace balances now have a public
     `amp usage` capture in the same `displayText` — [11](11-amp-daily-followup.md).
   - Amp "Amp Free" hardcoded plan label stale: Megawatt/Gigawatt subscriptions
     shipped 2026-07-18; no public capture of the new `displayText` yet —
     [04](04-amp-usage-api.md), [10](10-phrase-provenance-and-misc.md) Q3.
   - Amp reset model resolved for Amp Free: the current server format changed
     2026-07-11 to a percentage with `"resets daily"` and no exact timestamp;
     jackin❯'s replenishment-derived reset is obsolete. Paid subscription
     monthly text remains unobserved — [11](11-amp-daily-followup.md).
   - Grok: plan label heuristic wrong-by-design (`auth_mode=="oidc"` → "SuperGrok"
     also matches Free browser logins; `x.ai/billing.subscription_tier` is
     already resolved display-first); current `config.prepaidBalance` and
     positive enabled `onDemandCap`/`onDemandUsed` are not extracted; cent
     objects need proto-zero and signed-magnitude handling; pace passes
     `None` though the RPC path has period bounds — [05](05-grok-usage-api.md),
     [01](01-jackin-usage-current-coverage.md) round-2 addenda.
   - Codex `account/read` decoder tag mismatch: expects `"apikey"`, upstream sends
     `"apiKey"` + `"amazonBedrock"`; decode error fails the whole RPC usage result —
     [08](08-codex-followups.md) Q2.
   - z.ai: observed plan field `data.level` not in jackin❯'s alias list; auth header
     form (Bearer vs raw) unsettled — [06](06-zai-minimax-kimi-usage-apis.md),
     [10](10-phrase-provenance-and-misc.md) Q2.
   - MiniMax: officially documented host `www.minimax.io` absent from jackin❯'s
     URL fan-out — [01](01-jackin-usage-current-coverage.md) round-2 addenda.
   - Kimi: possible UA/client-whitelist gating (third-party tracker spoofs
     `KimiCLI/1.6`; jackin❯ sends `jackin-capsule/usage`) —
     [06](06-zai-minimax-kimi-usage-apis.md).

6. **Plan labels are mostly client-side.** Server-provided: Grok
   (`x.ai/billing.subscription_tier`, resolved display-first upstream), z.ai
   (`level`, single observation), Codex
   (`plan_type` enum — distinguishes `pro`/`prolite` but carries no "5x"/"20x"
   labels; first-party renders "Pro"/"Pro Lite"; jackin❯'s "Pro 20x"/"Pro 5x" is a
   marketing-level inference). Client-inferred: Claude (stored credential metadata),
   Amp (nothing), Kimi (web surface only) — [08](08-codex-followups.md) Q1,
   [03](03-claude-usage-api.md), [09](09-claude-followups.md).

## Candidate directions (evidence in chapters; choice is the operator's)

- **Run-out producer**: Variant A (linear-from-window-start; zero new data; jumpy
  early-window, wrong for rolling windows — but windows verified fixed-slot) vs
  Variant C (sampled burn from consecutive snapshots; smoother, needs history the
  store doesn't retain — one in-memory delta available today) vs Variant B (nominal
  pace; misleading for idle accounts). Producer belongs Rust-side (Swift splitter
  already exists) — [07](07-runout-projection-semantics.md).
- **Claude on macOS**: add a Keychain reader (macOS consent UI; "Always
  Allow" makes future reads silent while "Allow" may prompt again;
  `security find-generic-password -s "Claude Code-credentials"` / Security.framework)
  vs file-only + documented limitation vs waiting on anthropics#22144
  (credential-export feature request) — [09](09-claude-followups.md).
- **Grok plan/credits**: adopt the billing response's already-resolved
  `subscription_tier`; surface signed-normalized `prepaidBalance` and current
  on-demand used strictly behind a positive enabled cap as quota bounds —
  [05](05-grok-usage-api.md).

## Ruled out

- Any official/public REST usage API for Claude subscription windows (closed "not
  planned"), Codex ChatGPT-plan windows (private backend per openai/codex#29618), or
  Grok consumer windows (docs.x.ai is developer-API only) — [02](02-codex-usage-api.md),
  [03](03-claude-usage-api.md), [05](05-grok-usage-api.md).
- Codex per-turn `x-codex-*` headers / websocket events as a passive polling source
  (fire only on real turns) — [02](02-codex-usage-api.md).
- `account/usage/read` (historical token stats) — exactly the trend payload the
  limits-only rule forbids — [08](08-codex-followups.md).
- Anthropic Admin/Analytics APIs (org-scoped, not subscription windows) —
  [03](03-claude-usage-api.md).
- Amp structured-key fallbacks (`ampFreeRemaining` etc.) and the retired
  hourly-dollar compatibility reader — current CLI/API exposes only
  `displayText`, whose Amp Free line is daily percentage — [04](04-amp-usage-api.md),
  [11](11-amp-daily-followup.md).
- OpenUsage's spend donuts/histories as any kind of reference — forbidden surface
  (limits-only rule), noted in its screenshot — [10](10-phrase-provenance-and-misc.md).

## Open unknowns → disposition

**Operator-gated (agreed method: operator-authenticated agent-browser sessions +
live-key curls; queue for a follow-up session):**

- Claude: routines window live key (`seven_day_routines` vs `seven_day_cowork`) on a
  routines-active account; whether `oauth/profile` returns a plan label; weekly
  ~72h-refresh anomaly (monperrus gist) vs documented fixed weekly reset.
- Codex: live Spark `limit_name` strings; reset-credits list cap;
  `ChatGPT-Account-Id` header requirement; help.openai.com tier articles (Intercom
  SPA, browser-only).
- Amp: post-subscription `displayText` under Megawatt/Gigawatt/linked-subscription
  accounts (no public paid-plan capture exists six days post-launch);
  `getUserInfo` contents. Amp Free daily format and workspace-balance line are
  now resolved by the public live capture in [11](11-amp-daily-followup.md).
- Grok: live protobuf shape of the grok.com `GetGrokCreditsConfig` web route;
  `/v1/settings` contract; per-tier weekly-vs-monthly assignment.
- z.ai: Bearer vs raw header acceptance (two-form probe); `level` value enumeration.
- MiniMax: plan-title fields presence; `api.*` vs `www.` host equivalence.
- Kimi: exact `/coding/v1/usages` schema, plan field, UA gating; OAuth-token
  acceptance on the API-key surface.

**Scoped out:** OpenCode probe surface (exists in jackin❯, outside the seven
providers of this topic — [01](01-jackin-usage-current-coverage.md)).

**Assumptions safe to plan on:** fixed-slot windows anchored on `resets_at`;
one-parser-covers-both for Claude Keychain/file credential JSON; Swift stays
display-only with Rust producing any new label strings.
