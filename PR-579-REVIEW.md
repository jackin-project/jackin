# PR #579 Review — Capsule usage & quota overlay

Branch: `feature/capsule-usage-overlay-roadmap`
Roadmap item: [`docs/content/docs/reference/roadmap/capsule-usage-quota-overlay.mdx`](docs/content/docs/reference/roadmap/capsule-usage-quota-overlay.mdx)
Reviewer host run id: `18bbf3524d6cadd8` (`jackin/scratch/jk-5vvkqps1-holla-thearchitect`)

## Purpose

This file collects every defect observed on the operator host while exercising
this branch, cross-checked against the roadmap's claimed state. The roadmap
currently asserts most of the usage overlay is "landed and live-verified"; the
live run contradicts several of those claims. This document is the working
record we turn into a fix plan. Findings are not yet fixed.

## Legend

- **Severity**: Blocker (feature broken for the operator) / Major / Minor / Cosmetic.
- **Confidence**: Confirmed (code evidence cited) / Likely (strong inference) / Needs research.

---

## Meta-finding M1 — roadmap status markers are ahead of reality

**Severity:** Major · **Confidence:** Confirmed

`capsule-usage-quota-overlay.mdx` marks the remediation plan steps 1, 2, 5, 6,
7, 8 as "Landed and unit/snapshot-verified" (lines 75–85) and claims an
end-to-end live in-container HTTP 200 for Claude (line 87). The live run on this
branch shows the Claude overlay tab, Codex, MiniMax, and Kimi all failing in the
running app. The unit/snapshot tests pass because they exercise the adapter
fetch and the renderer in isolation; they do **not** exercise the live overlay
data path (cache key selection, refresh-clobber, credential timing). The status
must drop back to "Partially implemented — overlay data path unverified" and the
per-step "landed" claims that fail live verification must be reopened.

Related wording bug: line 32 says "The adapter must read `~/.claude/.credentials.json`
first **and the Keychain item on macOS hosts**." The capsule adapter is
container-only (Linux, PID 1) and *cannot* read the macOS Keychain. The Keychain
read is correctly done host-side by the runtime, which forwards the blob into the
container. The roadmap sentence conflates the host runtime with the in-container
adapter and should be corrected so future work doesn't try to add a `security`
call to `usage.rs`.

---

## F1 — Claude overlay tab shows "needs login" while the status bar shows live quota

**Severity:** Blocker · **Confidence:** Confirmed

### Observed

Same instant, same running daemon:

- Bottom status chrome: `Claude · Claude OAuth Weekly: 44% used / 100% · 56% left · Resets in 1d 23h` (authenticated, live).
- Usage overlay (`u`) Claude tab: `Anthropic / Claude — needs Claude login`, account `Needs login`, buckets `Session / Weekly / Daily Routines → provider API pending`, `Source: none · no confidence · needs login`, footer `Detail Claude credentials not available to Capsule`.

The overlay buckets `Session / Weekly / Daily Routines` and the account string
`needs Claude login` are exactly the `claude_snapshot` quota-`None` fallback
(`usage.rs:492-521`) and the no-credentials-file branch (`usage.rs:471-473`).
So the overlay is rendering a freshly recomputed `NeedsLogin` snapshot while the
status bar renders a previously cached good snapshot.

### Root cause (three contributing causes, all confirmed)

Both surfaces call `Multiplexer::focused_usage_snapshot` → `UsageCache::focused_snapshot` against the same cache, but diverge:

1. **Overlay recomputes live and the recompute fails.** Overlay open and the
   `r` refresh call `focused_usage_snapshot(true)` (`daemon/input_dispatch.rs:148,298`),
   `force_refresh = true` bypasses the 5-min cache and re-runs `claude_snapshot`
   (`usage.rs:129-135`). If at that instant the forwarded `.credentials.json` is
   unreadable, `oauth = None` → `NeedsLogin` fallback.
2. **Different cache key.** Status bar key is `"{agent}:{focused_provider}"`
   (provider derived from the session, `multiplexer_utils.rs:227-231`, key built
   at `usage.rs:122`). The overlay's `SwitchUsageProvider` hard-codes
   `"{agent}:Claude"` (`input_dispatch.rs:158`). `resolve_surface` treats
   `Claude` / `Claude Code` / `Anthropic` / agent `claude` all as Claude
   (`usage.rs:413-416`), so the same account can land under several distinct
   keys; the overlay can read a key the status bar never warmed.
3. **A failed refresh clobbers the cached-good entry.**
   `preserve_cached_quota_on_stale_refresh` (`usage.rs:1276-1313`) only restores
   cached buckets when `new == Stale && cached == Fresh`. It does **not** cover
   `NeedsLogin` / `Error`, so a transient credential miss overwrites a Fresh
   cache entry with `NeedsLogin` and all later reads on that key stay broken.

### Credential timing risk (likely amplifier)

The daemon's `usage_ticker` / `usage_account_ticker` are `interval(...)` whose
first tick fires immediately (`daemon.rs:590-591`). If the first usage refresh
runs before `setup_claude` copies `/jackin/claude/credentials.json` →
`/home/agent/.claude/.credentials.json` (`runtime_setup.rs:301-306`), the first
Claude snapshot is cached `NeedsLogin`. `~/.claude.json` cannot rescue it (no
access token there).

### Fix direction (for the plan, not yet applied)

- Make overlay/status read one normalized cache key (single provider-canonical key).
- Extend the preserve-on-stale guard to also preserve last-good quota on `NeedsLogin`/`Error`.
- Do not `force_refresh` on overlay *open*; only on explicit `r`. On explicit refresh, keep last-good on failure.
- Ensure first usage refresh runs after credential materialization (or retry on miss).

### Verify

Open overlay immediately after launch and again after 1 min; Claude tab must
match the status bar (live `Max`, `Session`/`Weekly`/`Sonnet` meters), never
`needs login`, including right after an `r` refresh.

---

## F2 — Codex status shows "account unavailable login" despite a working Codex CLI

**Severity:** Blocker · **Confidence:** Likely (parallels F1; needs code confirmation)

### Observed

Status bar: `Codex · account unavailable login`. Overlay Codex tab: `OpenAI /
Codex — needs Codex login`, account `Needs login`, buckets `Session` / `Weekly`
(`app-server/OAuth quota pending`), `Codex Spark 5-hour` / `Codex Spark Weekly`
(`provider API pending`), all `unsupported`; `Source: none · no confidence ·
needs login`. The Codex pane itself is fully logged in and working (shows
weekly-limit warnings, `/usage` resets available, `plan_type` clearly active).
The roadmap (lines 18, 65) claims Codex returns HTTP 200 from
`chatgpt.com/backend-api/wham/usage` and the tab renders. So the adapter found
**no** Codex credentials at read time — same no-creds class as F1.

### Suspected root cause

Same class as F1: the Codex adapter reads `~/.codex/auth.json`
(`tokens.access_token` + `tokens.account_id`) inside the container; the value is
either not forwarded, not present at refresh time, or the snapshot was cached as
unavailable and not refreshed. To confirm: trace `codex_snapshot`, the
`~/.codex/auth.json` forwarding into the container, and whether the cache holds a
stale unavailable entry.

### Verify

Codex tab and status bar both show the live `Pro`-class plan and
`Session`/`Weekly`/`Codex Spark` meters matching the running Codex CLI's `/usage`.

---

## F3 — MiniMax: "MiniMax API token unsupported"

**Severity:** Major · **Confidence:** Confirmed (mechanism) / Needs research (correct source)

### Observed

Status bar: `MiniMax · MiniMax API token unsupported` (later `… usage cached`).
Overlay MiniMax tab: account `MiniMax API token`, `Coding plan → MiniMax
API-token endpoint unavailable`, `MiniMax billing history is not imported by
Capsule`. Note the footer still reads `Source: provider API · authoritative ·
fresh` while the bucket is unavailable — provenance contradiction, see F6.

### Root cause

`minimax_snapshot` (`usage.rs:910-984`): status is `Unsupported` when a token
*was* found (`has_token == true`, hence account label `MiniMax API token`) but
`fetch_minimax_usage` failed (`usage.rs:913-919`). The token comes from
`MINIMAX_CODING_API_KEY` / provider keys / `MINIMAX_API_KEY` (`usage.rs:399-405`).
`fetch_minimax_usage` tries a list of `…/remains` endpoints
(`usage.rs:3057-3106`). So: a MiniMax key is reaching the adapter, but none of
the tried endpoints return usable usage for that key type.

### Needs research

- Which MiniMax credential type is forwarded (coding-plan API key vs platform API key vs web token), and which `remains` host/endpoint that key is valid against (`api.minimax.io` vs `api.minimaxi.com`, `token_plan/remains` vs `coding_plan/remains`).
- Whether the coding-plan usage endpoint needs different auth headers than currently sent.
- Confirm region/host: `MINIMAX_API_HOST` / `MINIMAX_REMAINS_URL` overrides exist (`usage.rs:3083-3106`) — determine the correct default for the operator's account.

### Verify

MiniMax tab shows a live `Coding plan` bucket / points balance with a 200 from
the resolved endpoint.

---

## F4 — Kimi: "Kimi auth token unsupported"

**Severity:** Major · **Confidence:** Confirmed (mechanism) / Needs research (correct source)

### Observed

Status bar: `Kimi · Kimi auth token unsupported`. **Decisive evidence:** the Kimi
Code CLI runs fine in its pane (K2.7 Code, v0.19.2, authenticated session active)
— so a **valid Kimi credential exists in the container** and the CLI uses it
successfully. The usage adapter still reports `unsupported`. This rules out a
missing/invalid credential: the credential is good, the adapter's billing call is
wrong.

### Root cause

`kimi_snapshot` (`usage.rs:827-908`): status is `Unsupported` when a token or
local config exists (`has_token || has_local`, account label `Kimi auth token`)
but `fetch_kimi_usage` failed (`usage.rs:831-837`). Token sources:
`KIMI_AUTH_TOKEN` / `kimi_auth_token` / `load_kimi_local_token` (reads
`~/.kimi-code/credentials/kimi-code.json`) / provider keys / `KIMI_CODE_API_KEY`
(`usage.rs:391-395`). But `fetch_kimi_usage` POSTs to the **web** billing
endpoint `www.kimi.com/apiv2/…/BillingService/GetUsages` using a browser
**cookie** `kimi-auth={token}` (`usage.rs:2774-2783`). Mismatch: the forwarded
credential is a Kimi-Code API/OAuth token, but the billing call expects a
`kimi.com` web session cookie. So a present token never authenticates the
billing endpoint.

### Needs research

- The correct usage/billing endpoint for the **Kimi For Coding** API token (roadmap token-cost-telemetry line 236: "Kimi For Coding billing endpoint from `KIMI_AUTH_TOKEN` / local Kimi Code OAuth token"), not the `kimi.com` web cookie endpoint.
- Whether the local `kimi-code.json` token can call a coding-plan billing API directly with a Bearer header.
- If only the web cookie endpoint exists, document Kimi as honest `unsupported` per the read-only policy instead of implying a token failure.

### Verify

Kimi tab shows live `Weekly` / `5-hour rate limit` buckets with a 200 from the
coding-plan endpoint, or an honest documented `unsupported` if no API-token
source exists.

---

## F5 — Provider detail render diverges from the binding roadmap preview (all tabs)

**Severity:** Major · **Confidence:** Confirmed

> The scaffold rows, the duplicate `Status` / `Status Page` rows, the `Cost`
> scaffold row, and the account-action stack below appear on **every** provider
> tab captured (Claude, Codex, MiniMax, Kimi) — this is the shared
> provider-detail renderer, not a Claude-only issue.

### Observed

Once Claude authenticates, the overlay shows live `Session` / `Weekly` / `Sonnet`
meters (good), but the rest of the tab does **not** match the binding Claude
Detail Preview (`capsule-usage-quota-overlay.mdx:763-805`). The preview is
declared the canonical look-and-feel contract (line 807): "A rendered overlay
that does not read like these previews is not done."

Divergences, live vs preview:

1. **Scaffold rows still present** — Step 5 (line 79) claims these were removed; they are not:
   - `Cost  Today unavailable · 30d unavailable · 30d tokens unavailable · latest unavai…` — the scaffold "Cost row"; the preview has no such row.
   - `Status: ProviderApi · Authoritative · cached by capsule daemon` **and** a duplicate `Status Page  ProviderApi · Authoritative · cached by capsule daemon  >` — status rendered twice.
2. **Footer format wrong.** Preview footer is two lines: `Source: local estimate · fresh · fetched 8s ago` / `Status: ok`. Live emits `Source: provider API · authoritative · fresh · Updated just now` plus a stack of `Status` / `Status Page` / `Subscription Utilization` / `Buy Credits` / `Add Account` / `Usage Dashboard` / `Refresh:` rows the preview does not show.
3. **Missing sections** the preview requires: the history **sparkline** (`▁▁▁▂…`) and **`Top model: claude-sonnet-4-6`** are absent.
4. **Missing buckets:** preview shows `Daily Routines` and `Extra usage` (with `Monthly cap: …`) after `Sonnet`; live stops at `Sonnet`. (Ties to roadmap confirmed-defect #4, incomplete window mapping.)
5. **Mixed-provenance footer.** Account availability is `provider API · authoritative · fresh` while the cost line reads `Estimated from local Claude logs; no local usage files found`. Two different provenances are flattened under one `Source:` label. token-cost-telemetry line 109: "Any UI that mixes these numbers must label them separately." Account availability and Workspace spend must be labeled distinctly.
6. **Account cost/tokens all `unavailable`.** May be legitimate (fresh workspace, no local logs), but the provenance string then contradicts the "fresh provider API" source label; needs the two-axis labeling from #5.

### Root cause (to confirm in code)

The provider-detail builder (`tui/components/dialog.rs`, ~`1307-1320`, fed by the
`FocusedUsageView` from `usage.rs`) still emits the account-action / status
scaffold rows and omits sparkline + `Top model`. Note an **internal roadmap
contradiction** to resolve: Step 5 (line 66/79) says delete scaffold rows, while
line 94 says the account-action rows (`Cost`, `Subscription Utilization`,
`Buy Credits`, `Add Account`, `Usage Dashboard`, `Status Page`) are "retained per
the Required account-tab fields list." The previews omit them. Operator must
decide: match the preview (drop them) or keep them in a clearly separated,
non-duplicated section.

### Fix direction

- Render exactly the preview's sections in order: header (provider left / plan right), `Account availability` meters (incl. `Daily Routines`, `Extra usage`), two-column `Account cost and tokens`, sparkline, `Top model`, single provenance line, two-line `Source` / `Status` footer.
- Remove the duplicate `Status` / `Status Page` rows and the scaffold `Cost` row.
- Label Account availability (provider-authoritative) and spend (local estimate) provenance separately.
- Map `Daily Routines` / `Extra usage` (and `Opus` when present); ignore unknown promo windows.

### Verify

Side-by-side diff of the rendered Claude tab against the preview across the three
width bands: same sections, same order, each label once, sparkline + `Top model`
present, no scaffold/duplicate rows.

---

## F6 — Footer provenance is dishonest (claims "provider API · authoritative · fresh" when the fetch failed)

**Severity:** Major · **Confidence:** Confirmed

### Observed

- MiniMax tab: bucket says `Coding plan → MiniMax API-token endpoint unavailable`, yet footer says `Source: provider API · authoritative · fresh · Updated just now` and `Status: ProviderApi · Authoritative · cached by capsule daemon`.
- The same `Source: provider API · authoritative · fresh` footer appears on Claude even though the cost line is a `local estimate`.

### Why it's wrong

`minimax_snapshot` with no successful fetch sets `confidence = PresenceOnly`,
`status = Unsupported`, `source = ProviderApi` (`usage.rs:959-970`). The footer
renders `authoritative · fresh` regardless — so the displayed provenance does not
match the snapshot's real `(source, confidence, status)`. Per token-cost-telemetry
lines 109/307, every rendered number must carry its true `UsageConfidence`
(provenance) and `UsageSnapshotStatus` (lifecycle); a `fresh authoritative` label
on an unavailable/`PresenceOnly` bucket is exactly the "single percentage without
provenance is a bug" failure. Either the footer mapping is decoupled from the
snapshot fields, or a stale-preserved entry's labels leak onto a failed refresh.

### Fix direction

Footer `Source` / `Status` must render the snapshot's actual
`source · confidence · status`, and Account-availability vs Workspace-spend
provenance must be labeled on their own lines (F5 #5). An unavailable bucket must
never read `authoritative · fresh`.

### Verify

For each provider, the footer provenance matches the real fetch outcome:
authoritative+fresh only on a real 200; unavailable/unsupported buckets show
`presence only` / `none` and `unsupported` / `needs login` honestly.

---

## F7 — Amp: "needs Amp login" / "Amp auth not available to Capsule"

**Severity:** Major · **Confidence:** Needs research (bug vs honest not-configured)

### Observed

Amp tab: account `needs Amp login`, bucket `Amp Free → amp usage/web source
pending`, `Source: none · no confidence · needs login`, footer `Detail Amp auth
not available to Capsule`.

### Open question

The host inventory (roadmap line 19) shows `~/.local/share/amp/secrets.json`
present on the host. Either (a) Amp creds are not being forwarded into this
workspace's container (same forwarding class as F2/Claude-overlay), or (b) this
workspace simply has no Amp pane and the honest `needs login` is correct. Confirm
whether Amp is configured for this workspace before treating as a bug. If creds
exist on host but don't reach the container, it's the same forwarding defect.

### Verify

If Amp is configured: tab shows live `Amp Free` / credits from `amp --no-color
usage`. If not: honest `needs login` is acceptable.

---

## F8 — Grok Build: "account unavailable login" despite working CLI

**Severity:** Major · **Confidence:** Likely (cluster)

### Observed

Grok Build pane runs and responds (authenticated, "Grok Build · always-approve",
turn completed). Status bar: `Grok Build · account unavailable login`. Same
pattern as Codex/Claude/Amp: the CLI authenticates, the usage adapter does not.
Roadmap claims "Grok API-key and deployment-key presence detection" landed
(line 5). Confirm what `grok` usage adapter reads vs what the working CLI uses.

---

## Cross-cutting cluster: "needs login / no forwarded creds" (F1·F2·F7·F8)

Claude (overlay, F1), Codex (F2), Amp (F7), and Grok Build (F8) all render
`needs login` / `account unavailable` while the underlying CLI works
(Claude, Codex, Grok all observed running and authenticated) or the host has
creds (Amp). This strongly suggests a **single shared root cause**: the
in-container credential read (or the host→container forwarding/timing) fails for
file/fetch-based providers, and the daemon caches the failure.

Pattern: providers on the **env-var / provider-key** path work (GLM/Z.AI
`GLM 99% left`, `usage.rs:383-388`); providers that must **read a forwarded
credential and call a usage endpoint** fail (Claude OAuth, Codex `auth.json`, Amp
secrets, Grok `auth.json`). Kimi/MiniMax are a separate sub-case: credential
present (CLI works) but the adapter calls the wrong endpoint (F3/F4).

**Recommend root-causing the forwarding/read path once** — likely fixes F1, F2,
F7, F8 together. Trace: what each adapter reads in-container, whether
`runtime_setup` forwards each provider's creds into the expected container path,
and the first-refresh-vs-credential-materialization ordering (F1 timing note).

---

## F9 — No token refresh (Claude, Codex)

**Severity:** Major · **Confidence:** Confirmed · Detail in CodexBar section.

jackin never refreshes OAuth tokens. CodexBar refreshes Claude
(`platform.claude.com/v1/oauth/token`, client `9d1c250a-…`) and Codex
(`auth.openai.com/oauth/token`, client `app_EMoamEEZ…`). Once a forwarded token
expires, jackin's fetch 401s and the surface dies until the host re-forwards.
**Design tension:** roadmap line 104 forbids writing host credentials. Resolution
must refresh only the *container* copy, never the host — explicit decision needed
(Open questions).

## F10 — Claude field/parity gaps

**Severity:** Minor · **Confidence:** Confirmed · Detail in CodexBar section.

`extra_usage` amounts not divided by 100 (100× too large, currency mislabeled);
missing bucket aliases (`seven_day_oauth_apps`, routines aliases, `iguana_necktie`)
→ Daily Routines silently dropped on alias; plan label ignores `rateLimitTier`
fallback; static `claude-code/2.1.0` UA vs CodexBar's detected version.

---

## Working (for contrast — do not regress)

- **GLM / Z.AI** — status `GLM 99% left` (provider-key snapshot path works, `usage.rs:383-388`).
- **Claude status chrome** — `Claude 56% left` / live Weekly (the *cache* is good; only the overlay path diverges — see F1).

---

## Cross-cutting observations

- **Two render paths, one truth needed.** F1 and F2 are both "status chrome has data, detail surface doesn't" — the overlay/detail builder must read the same authenticated daemon cache the chrome reads, with one canonical provider key, and must never let a failed refresh erase last-good data.
- **`unsupported` vs `needs login` honesty.** F3/F4 render `unsupported` when a token is present but the *endpoint/credential-type* is wrong. That reads as "we don't support this" when the truth is "we're calling the wrong endpoint." The label should distinguish "no usage API exists for this credential" from "fetch failed."
- **Stale memory note.** The local memory "Claude usage reads wrong cred source" is pre-fix; `usage.rs:454-456` already reads `.credentials.json` first. The real Claude defect is the overlay path divergence (F1), not the file order.

---

## CodexBar reference recipes & confirmed drift (per provider)

Researched against CodexBar source (`github.com/steipete/CodexBar`, cloned), diffed
against jackin `crates/jackin-capsule/src/usage.rs` and
`crates/jackin-runtime/src/instance/auth.rs`. **Headline: jackin can extract every
provider's data using the same APIs CodexBar uses — CodexBar is the proof. Each
failure is a specific drift, listed below.** These recipes are also the seed for
the source-of-truth doc (see Documentation Plan).

### Claude / Anthropic

- **Credential order (CodexBar):** env `CODEXBAR_CLAUDE_OAUTH_TOKEN` → in-mem cache → CodexBar keychain cache → `~/.claude/.credentials.json` (`claudeAiOauth.accessToken`) → macOS Keychain `Claude Code-credentials`. Blob = JSON `claudeAiOauth { accessToken, refreshToken, expiresAt(ms), scopes, rateLimitTier, subscriptionType }` (camelCase).
- **Endpoint:** `GET https://api.anthropic.com/api/oauth/usage` · `Authorization: Bearer` · `Accept/Content-Type: application/json` · `anthropic-beta: oauth-2025-04-20` · `User-Agent: claude-code/<detected version>` (falls back 2.1.0) · no body, no `anthropic-version`.
- **Buckets:** `five_hour`→Session, `seven_day`→Weekly, `seven_day_sonnet`/`seven_day_opus`→model, routines (7 aliases) →Daily Routines, `seven_day_oauth_apps`, `iguana_necktie`, `extra_usage`{monthly_limit, used_credits(**cents/100**), utilization, currency=USD}. Plan = `subscriptionType` then `rateLimitTier`.
- **Refresh:** `POST https://platform.claude.com/v1/oauth/token` grant=refresh_token, client_id `9d1c250a-e61b-44d9-88ed-5944d1962f5e`, when expired.
- **Drift (jackin):** request matches. Gaps: (1) static UA `claude-code/2.1.0` vs dynamic version; (2) **no refresh** — once forwarded token expires → 401 → stale forever (capsule can't tell expired from missing); (3) missing bucket aliases (`seven_day_oauth_apps`, routines aliases, `iguana_necktie`) → silently drops Daily Routines if API uses an alias; (4) **`extra_usage` not divided by 100** → amounts 100× too large + currency mislabeled `credits`; (5) plan only from `subscriptionType`, drops `rateLimitTier` (struct discards it); (6) overlay path divergence = F1.

### Codex / OpenAI

- **Credential (CodexBar):** `$CODEX_HOME/auth.json` → `OPENAI_API_KEY` top-level, else `tokens.access_token` + `tokens.account_id` (+ `id_token`, `last_refresh`). File-only, no keychain.
- **Endpoint:** `GET https://chatgpt.com/backend-api/wham/usage` · `Authorization: Bearer` · `User-Agent` · `Accept: application/json` · `ChatGPT-Account-Id: <account_id>` (only if present). Alternate authority: spawn `codex app-server`, JSON-RPC `account/read` + `account/rateLimits/read`.
- **Buckets:** `rate_limit.primary_window`→Session, `secondary_window`→Weekly (`used_percent`/`reset_at`/`limit_window_seconds`); `additional_rate_limits[]` spark→`Codex Spark 5-hour`/`Weekly`; `credits`; `plan_type`→plan.
- **Refresh:** `POST https://auth.openai.com/oauth/token` client_id `app_EMoamEEZ73f0CkXaXp7hrann`, when `last_refresh` > 8 days.
- **Drift (jackin):** URL+headers **byte-identical** (incl. `ChatGPT-Account-Id`). Root cause of "needs login" is NOT the request — it's: (1) **account label requires `tokens.email`** which real auth.json lacks (email is inside the `id_token` JWT, never decoded); CodexBar derives the account from app-server RPC instead; (2) the app-server RPC path is gated by a **launch cooldown** (`ManagedCliLaunchGate`) — one failed `codex app-server` spawn poisons the account label, and the HTTP `/wham/usage` path returns quota-but-no-email, so status stays `NeedsLogin`; (3) **no token refresh** + (4) in-container `auth.json` copy is skipped after first launch (`setup_codex` only copies when `!marker.exists()`), so a re-login on host doesn't reach an existing container → stale token → 401, silently.

### Kimi  ← THE clear root cause

- **CodexBar = two separate auth/endpoint pairs:**
  - **Code-API token** (`KIMI_CODE_API_KEY`, or the working CLI's token): `GET https://api.kimi.com/coding/v1/usages` · `Authorization: Bearer` · `Accept: application/json`. Response = `KimiCodeAPIUsageResponse` (top-level `usage` + `limits[]`).
  - **Web cookie** (`kimi-auth` JWT): `POST https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages` · `Cookie: kimi-auth=<jwt>` + JWT-derived `x-msh-device-id`/`x-msh-session-id`/`x-traffic-id` + browser UA · body `{"scope":["FEATURE_CODING"]}`.
- **Drift (jackin):** jackin merges ALL token sources (incl. `KIMI_CODE_API_KEY` and `~/.kimi-code/credentials/kimi-code.json` `access_token`) into one var and **always POSTs to the web-cookie endpoint** as `Cookie: kimi-auth={token}` — and omits the JWT session headers. The forwarded credential is a **Code-API token** (the CLI proves it's valid), which the web endpoint rejects → `Unsupported`. **jackin has no `GET api.kimi.com/coding/v1/usages` Bearer path and no Code-API response shape at all.** Fix: branch by credential type; add the Code-API GET path.

### MiniMax

- **CodexBar:** primary path is **web/cookie** against `platform.minimax.io/user-center/payment/coding-plan` (+ `www.*` JSON remains); API-token path (`Authorization: Bearer`, `MM-API-Source`) against `api.minimax.io`/`api.minimaxi.com` `/v1/token_plan/remains` + `/v1/api/openplatform/coding_plan/remains` is one strategy that **falls back to web** on 404/reject. Global→China host retry preserved. Buckets from `model_remains[]` (counts, not tokens).
- **Drift (jackin):** URL/host/path/Bearer scheme **identical** — no host bug. But: (1) **jackin removed the web/cookie fallback** — which is the path CodexBar actually relies on for coding-plan quota (coding-plan keys are served to the web session, not the bare `api.minimax.io` bearer endpoint); (2) jackin **aborts the whole URL loop on the first transport error** (`?` on `.send()`, `usage.rs:3068`) so a `.com`-only China key never gets tried; (3) **swallows the real error** (`.ok()`) → always shows generic "endpoint unavailable" instead of the true 401/`status_code 1004`/msg.

### Grok (xAI)

- **CodexBar:** `~/.grok/auth.json` (scope-keyed map; bearer = `key` field). Primary: spawn `grok agent stdio`, JSON-RPC `x.ai/billing` (must keep method slash-unescaped). Fallback: `POST https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig` (gRPC-web, `Authorization: Bearer` or browser cookie). Buckets = credits (cents), `monthlyLimit`/`usage.totalUsed`/`billingCycle`.
- **Drift (jackin):** RPC params + field map + slash-unescape trick **match exactly**. Failure: (1) `~/.grok/auth.json` only forwarded under `AuthForwardMode::Sync`; (2) jackin **gates the RPC on credential-file presence** (`has_credentials`) and short-circuits to `NeedsLogin` without even running `grok agent stdio` — CodexBar always runs the probe; (3) **no web gRPC-web fallback** (disabled). So a non-Sync role or transient RPC failure → "needs Grok login" while the CLI works.

### Amp

- **CodexBar:** primary token path = env `AMP_API_KEY` → `POST https://ampcode.com/api/internal?userDisplayBalanceInfo` body `{"method":"userDisplayBalanceInfo","params":{}}` `Authorization: Bearer`; plus `amp usage` CLI; plus web cookie (`ampcode.com/settings`). **CodexBar never reads `secrets.json`.**
- **Drift (jackin):** jackin implements **only the `amp usage` CLI**, detects "auth" from `AMP_API_KEY` **or** `~/.local/share/amp/secrets.json`, and forwards `secrets.json` only under Sync. No `AMP_API_KEY`→REST path (CodexBar's primary). Also no amp-binary staging in the container (unlike grok), so `amp usage` may not even be on PATH. → "needs Amp login" when secrets.json not forwarded and no env key.

### GLM / Z.AI — WORKS (reference pattern)

- Env `Z_AI_API_KEY`/`ZAI_API_KEY` (no file, no cookie) → `GET https://api.z.ai/api/monitor/usage/quota/limit` `Authorization: Bearer`. jackin mirrors CodexBar byte-for-byte.
- **Why it works = the fix template:** (1) credential is an **env var carried into the container regardless of mount mode** (not a Sync-gated host file); (2) **direct HTTPS REST call jackin makes itself** (no dependency on an in-container CLI being present/logged-in); (3) one deterministic GET. Grok and Amp should get equivalent env-key REST paths.

## Cross-cutting root-cause patterns (the real fix list)

1. **Credential delivery into the container is the dominant failure.** File-based creds (Claude `.credentials.json`, Codex `auth.json`, Grok `auth.json`, Amp `secrets.json`) only land under `AuthForwardMode::Sync` and (Codex) only on first launch via a `.done` marker. Env-key providers (Z.AI) always work. **Pattern fix:** prefer env-key REST paths where the provider offers them; make forwarding unconditional/refreshed for the rest; verify forwarding in the launch summary.
2. **Don't gate the usage probe on local credential presence** (Grok). Run the probe; report the real error. Pairs with F6 (honest provenance).
3. **Branch by credential TYPE, not a merged token** (Kimi: Code-API Bearer vs web cookie; MiniMax: API token vs web session).
4. **Add the fallback strategy CodexBar relies on, or document honest `unsupported`** (MiniMax web/cookie for coding-plan; Kimi web with JWT headers).
5. **No token refresh anywhere** (Claude, Codex). Once a forwarded token expires the capsule is dead until host re-forwards. CodexBar refreshes. **Design tension with the read-only-host guarantee (roadmap line 104):** refresh would have to write only the *container* credential copy, never the host — needs an explicit decision (see Open questions).
6. **Stop swallowing fetch errors** (`.ok()` everywhere) — surface the true status string; it both fixes operator confusion and feeds F6.
7. **Field/label parity gaps** (Claude extra_usage cents/100, bucket aliases, rateLimitTier; dynamic UA) — parity-or-better per roadmap line 100.

## Documentation plan (source-of-truth pages)

Goal: one canonical contributor reference for **where and how jackin extracts token usage and quota for every provider** — kept in lockstep with `usage.rs`. Proposed under `docs/content/docs/reference/` (Internals audience — contains endpoints, fields, on-disk paths; must NOT go on operator/role surfaces per docs rules).

1. **`reference/usage-telemetry/index.mdx` — "Token usage & quota: data sources" (the source-of-truth page).** Per provider: credential source(s) + container delivery, exact endpoint(s) + method + headers, response→bucket field map, plan-label rule, refresh policy, and the honest-degradation/unsupported states. One table-per-provider mirroring the recipes above. This is the page that gets updated whenever an adapter changes (add to the PR docs gate / TODO stale-docs checklist).
2. **`reference/usage-telemetry/credential-forwarding.mdx`** — how host creds reach the container (Keychain→file, `auth.json`/`secrets.json` forwarding, `AuthForwardMode`, per-config-dir Keychain service derivation, the read-only-host guarantee and the no-refresh-writes-host rule). Cross-links to the runtime `auth.rs`.
3. **`reference/usage-telemetry/cost-and-provenance.mdx`** — how cost is computed (price table, `cost_source` Exact/PriceTable/Unpriced), the Account-availability vs Workspace-spend two-axis labeling, confidence/status semantics, units (cents/tokens/counts/points per provider).
4. **Roadmap updates (M1):** reconcile `capsule-usage-quota-overlay.mdx` status markers (reopen falsely-landed steps), fix line-32 Keychain wording, resolve the Step-5-vs-line-94 scaffold contradiction, and point the roadmap at the new reference pages as canonical (roadmap keeps status + remaining work only, per docs "roadmap doesn't duplicate shipped docs" rule).
5. **Sidebar/overview discipline:** add the new group to `reference/.../meta.json` and the roadmap overview per the docs `check:roadmap-sidebar` rule.

## Findings index

| ID | Severity | Confidence | Summary |
|----|----------|-----------|---------|
| M1 | Major | Confirmed | Roadmap status markers ahead of reality; line-32 Keychain wording wrong |
| F1 | Blocker | Confirmed | Claude overlay `needs login` while chrome shows live quota (cache-key + clobber + recompute) |
| F2 | Blocker | Confirmed | Codex `account unavailable`: account-label needs `tokens.email` (absent; in id_token JWT) + app-server RPC cooldown + no refresh + stale in-container auth.json |
| F3 | Major | Confirmed | MiniMax: no web/cookie fallback (CodexBar's coding-plan path); aborts URL loop on first transport err; swallows real error |
| F4 | Major | Confirmed | Kimi: Code-API token sent to web-cookie endpoint; jackin lacks `GET api.kimi.com/coding/v1/usages` Bearer path + Code-API response shape |
| F5 | Major | Confirmed | Provider detail render diverges from binding preview on ALL tabs (scaffold + dup rows, missing sparkline/Top model/buckets) |
| F6 | Major | Confirmed | Footer provenance dishonest (`authoritative · fresh` on failed fetch); errors swallowed via `.ok()` |
| F7 | Major | Confirmed | Amp: only `amp usage` CLI (binary maybe not on PATH), secrets.json Sync-gated; missing CodexBar's `AMP_API_KEY`→REST primary path |
| F8 | Major | Confirmed | Grok: `~/.grok/auth.json` Sync-gated + RPC gated on cred presence (skipped) + no gRPC-web fallback |
| F9 | Major | Confirmed | No token refresh (Claude/Codex) → expired forwarded token = dead until host re-forward; CodexBar refreshes |
| F10 | Minor | Confirmed | Claude field gaps: extra_usage cents/100 (100× too large), missing bucket aliases, rateLimitTier plan fallback, static UA |

## Proposed fix order (draft — confirm before implementing)

1. **Credential delivery + cluster (F1 + F2 + F7 + F8).** Fix host→container forwarding so file-creds (Claude/Codex/Grok/Amp) reliably land and refresh (not Sync-gated/first-launch-only); don't gate the probe on local cred presence (F8); add env-key REST paths where providers offer them (Amp `AMP_API_KEY`→REST, Grok gRPC-web), using Z.AI as the template. Plus F1 overlay specifics: one canonical cache key, preserve-last-good on `NeedsLogin`/`Error`, no force-refresh on overlay open.
2. **Codex account label (F2).** Derive account from `id_token` JWT (or app-server RPC) instead of requiring `tokens.email`; decouple status from the cooldown-gated RPC; refresh the in-container `auth.json` copy.
3. **Kimi (F4) + MiniMax (F3) credential-type branching.** Kimi: add `GET https://api.kimi.com/coding/v1/usages` Bearer path + Code-API response shape; reserve web-cookie POST for real JWTs (with session headers). MiniMax: add web/cookie fallback or honest `unsupported`; don't abort URL loop on transport error; surface the real error.
4. **F5 + F6 render contract.** Rewrite shared provider-detail renderer to match the binding preview (sections/order, sparkline, `Top model`, full bucket set), drop scaffold + duplicate `Status`/`Cost` rows, render the real `(source, confidence, status)`, label availability-vs-spend provenance separately, stop swallowing errors (`.ok()`).
5. **F9 refresh.** Decide the container-only refresh policy (never touch host) for Claude + Codex, then implement.
6. **F10 parity.** Claude extra_usage cents/100, bucket aliases, rateLimitTier fallback, dynamic UA.
7. **M1 + docs.** Reconcile roadmap status markers, fix line-32 Keychain wording, resolve Step-5-vs-line-94 contradiction, and create the source-of-truth doc set (Documentation Plan).

## CodexBar visual acceptance reference (operator host screenshots)

These are the operator's live CodexBar renders on the host — the binding visual
target for WS8, alongside the roadmap previews. Transcribe each tab into the
source-of-truth doc.

### Codex tab (CleanShot 2026-06-24)

Tab strip: `Overview · Codex · Claude · z.ai · MiniMax · Kimi · Amp · Grok` (per-tab
underline = health color: z.ai/Grok green, Claude/MiniMax/Kimi/Amp orange/red).

Section order, top to bottom:
1. **Header:** `Codex` (left) · `alexey@chainargos.com` (right). Second line: `Updated 2m ago` (left) · `Pro 20x` (right).
2. **Session** meter — `97% left` / `6% in reserve` (left) · `Resets in 4h 33m` / `Lasts until reset` (right).
3. **Weekly** meter — `19% left` / `11% in reserve` · `Resets in 13h 6m` / `Lasts until reset`.
4. **Codex Spark 5-hour** — full bar · `100% left` · `Resets in 4h 58m`.
5. **Codex Spark Weekly** — full bar · `100% left` · `Resets in 6d 23h`.
6. **Limit Reset Credits** — `2 manual resets available` (right) · `Next expires in 17d 17h`. (Codex-specific; not in roadmap preview — add it.)
7. **Cost grid (2-col):** `Today $186.32` | `30d cost $3,544.59` · `30d tokens 5B` | `Latest tokens 243M`.
8. **Sparkline** histogram.
9. `Top model: gpt-5.5`.
10. Provenance: `Estimated from local Codex logs for the selected acco…`.
11. **Credits** meter — `0 left` (left) · `1K tokens` (right).
12. Action rows (clean, single line, chevron where expandable): `⊕ Buy Credits…`, `Cost ›`, `Subscription Utilization ›`, `+ Add Account…`, `▫ Usage Dashboard`, `⌁ Status Page` → **`Partial System Degradation — Updated 13h ago`** (real provider status-page health).
13. Footer: `↻ Refresh`, `⚙ Settings… ⌘,`, `ⓘ About CodexBar`, `⊠ Quit ⌘Q` (jackin uses `r Refresh · Tab Switch provider · Esc Close` instead — keep jackin's keybind footer).

**What this resolves / proves:**
- **F5 contradiction resolved:** the account-action rows (`Buy Credits`, `Cost`, `Subscription Utilization`, `Add Account`, `Usage Dashboard`, `Status Page`) ARE real CodexBar UI → **keep them** (line 94 wins). Step 5's "remove scaffold" applies only to jackin's *doubled/flattened junk* (`Cost  Today unavailable · 30d unavailable …`, duplicate `Status`/`Status Page` showing snapshot provenance). Render each action row as a single clean label with a `›` chevron, exactly like CodexBar.
- **`Status Page` is provider status-page health**, not a repeat of `Source`/`Status`. jackin currently renders `Status Page  ProviderApi · Authoritative · cached by capsule daemon ›` — wrong; it must show the provider's incident status (like `Partial System Degradation — Updated 13h ago`) or honest `unknown`.
- **Missing sections to add for Codex:** `Limit Reset Credits` (manual resets), `Credits` meter, sparkline, `Top model`. jackin's Codex tab currently shows none of these.
- Cost grid label set matches the roadmap preview; CodexBar fills real values from local Codex logs (jackin shows `unavailable` — local-log scan not finding/attributing files; covered by spend ingestion, separate from quota).

### Claude tab (CleanShot 2026-06-24)

1. **Header:** `Claude` · `Max` (right). Second line: `Refreshing…` (status) · (plan right).
2. **Session** — `92% left` / `10% in reserve` · `Resets in 4h 5m` / `Lasts until reset`.
3. **Weekly** — `55% left` / `27% in reserve` · `Resets in 1d 22h` / `Lasts until reset`.
4. **Sonnet** — `85% left` · `Resets in 1d 22h`.
5. **Daily Routines** — full bar · `100% left` (no reset line).
6. **Cost grid:** `Today $266.63` | `30d cost $7,231.50` · `30d tokens 11B` | `Latest tokens 425M`.
7. Sparkline · `Top model: claude-opus-4-8` · `Estimated from local Claude logs at API rates; token t…`.
8. Action rows: `Cost ›`, `Subscription Utilization ›`, `Add Account…`, `Usage Dashboard`, `Status Page`. (No `Buy Credits`, no `Credits`, no `Limit Reset Credits` — those are Codex-only.)

Notes: **no `Extra usage` and no `Opus` bucket shown for this account** → both are conditional (render only when the API returns them). `Top model` here is `claude-opus-4-8`.

### z.ai tab (CleanShot 2026-06-24)

1. **Header:** `z.ai` · `Updated just now`.
2. **Tokens** — `99% left` · `Resets in 3d`.
3. **MCP** — `100% left` · `Resets in 19d` · detail line `0 / 100 (100 remaining)`.
4. **5-hour** — `100% left` · `Resets 5 hours window`.
5. Action rows: `Hourly Usage ›`, `Usage Dashboard`. (No cost grid, no Add Account, no Status Page.)

Notes: CodexBar bucket labels are `Tokens` / `MCP` / `5-hour`; jackin currently labels them `Session token limit` / `Token quota` / `Time / MCP quota` — align to CodexBar. MCP shows a `N / 100 (remaining)` detail line.

### MiniMax tab (CleanShot 2026-06-24) — CodexBar WORKS here

1. **Header:** `MiniMax` · `Updated just now`.
2. **General · 5h** — `0% used` · `Usage: 0 / 100` · `Resets in 1 hour`.
3. **General · Weekly** — `1% used` · `Usage: 1 / 100` · `Resets in 4 days`.
4. **Video** — `0% used` · `Usage: 0 / 100` · `Resets in 15 hours`.
5. Action rows: `Usage Dashboard` only.

Notes: **This proves jackin CAN get MiniMax data — CodexBar renders it live** (F3 is fixable, not impossible). Buckets are **per model/feature × window**: `<model> · <window>` (`General · 5h`, `General · Weekly`, `Video`), from `model_remains[]`. MiniMax uses **`% used`** semantics (not `% left`) and a `Usage: N / 100` count line. jackin's single `Coding plan` bucket is wrong — render one bucket per `model_remains[]` entry × interval/weekly window.

### Kimi tab (CleanShot 2026-06-24) — CodexBar WORKS here

1. **Header:** `Kimi` · `Updated just now`.
2. **Weekly** — `100% left` · `Resets in 2m`.
3. **Rate Limit** — `100% left` / `60% in reserve` · `Resets in 2h 2m` / `Lasts until reset`.
4. Action rows: `Usage Dashboard` only.

Notes: **proves Kimi data is reachable** — CodexBar reads it via the Code-API endpoint (`api.kimi.com/coding/v1/usages`), the exact F4 fix path. Buckets: `Weekly` + `Rate Limit` (jackin calls the latter `5-hour rate limit` → align label to `Rate Limit`). Rate Limit carries a pace line.

### Amp tab (CleanShot 2026-06-24) — CodexBar WORKS here

1. **Header:** `Amp` · `alexey@zhokhov.com` (right; note: different account than chainargos). Second line: `Updated 1m ago` · `Amp Free` (plan).
2. **Amp Free** — `100% left` · `Resets now`.
3. **Credits** — `Individual credits: $4.94`.
4. Action rows: `Usage Dashboard` only.

Notes: **proves Amp data is reachable** — CodexBar reads it via `AMP_API_KEY` → `ampcode.com/api/internal` REST, the F7 fix path. Buckets: `Amp Free` meter + `Credits` (individual-credits dollar label).

### Grok tab (CleanShot 2026-06-24) — CodexBar WORKS here

1. **Header:** `Grok` · `alexey@chainargos.com` (right). Second line: `Updated 1m ago` · `SuperGrok` (plan).
2. **Weekly** — `18% left` (green bar) · `Resets in 6d 15h`.
3. Action rows: `Usage Dashboard`, `Status Page`.

Notes: **proves Grok data is reachable** — via RPC `x.ai/billing` (or web fallback), the F8 path. This account shows a single `Weekly` bucket; plan `SuperGrok`.

### ★ Conclusion: all 7 provider tabs render live in CodexBar on the host

Codex, Claude, z.ai, MiniMax, Kimi, Amp, Grok — **every one works in CodexBar on
the operator's machine**. Therefore every jackin failure is a jackin-side drift
(credential delivery, wrong endpoint/credential-type, swallowed errors, render),
not a provider/API limitation. The implementation plan below closes each drift to
reach CodexBar parity.

### Cross-provider render conventions (binding)

- **Per-provider section sets differ — jackin must NOT render a uniform action-row stack on every tab.** Codex: meters + Limit Reset Credits + cost grid + sparkline + Top model + Credits + Buy Credits + Cost + Subscription Utilization + Add Account + Usage Dashboard + Status Page. Claude: meters + cost grid + sparkline + Top model + Cost + Subscription Utilization + Add Account + Usage Dashboard + Status Page. z.ai: meters + Hourly Usage + Usage Dashboard. MiniMax: meters + Usage Dashboard. Render only the sections each provider actually has.
- **`% left` vs `% used`:** Claude/Codex/z.ai = `N% left`; MiniMax = `N% used`. Honor each provider's native convention.
- **Pace line** (`N% in reserve` / `N% in deficit` / `On pace`) appears on Claude/Codex meters (second left line); reset on the right (`Resets in …`) with optional `Lasts until reset`.
- **Conditional buckets:** Opus, Extra usage (Claude), Spark (Codex), Credits — render only when present.
- **`Status Page`** = provider incident health (e.g. `Partial System Degradation — Updated 13h ago`), never the snapshot's source/confidence.
- **Header:** provider name left, account/plan right; status (`Updated …` / `Refreshing…`) on the second line.

## Proposed jackin overlay previews (operator sign-off)

These are the **target jackin Capsule overlay renders** for each tab — designed
from the CodexBar host screenshots above, in the roadmap's box-drawing style
(`capsule-usage-quota-overlay.mdx:763-805`), adapted to jackin's terminal
constraints (keyboard footer, per-provider section sets, honest provenance).
Numbers are the live CodexBar values for illustration. **Review each block and
comment; I'll revise until each is signed off, then WS8 implements to match.**
Convention: `█` filled / `·` empty meter; `[Tab]` = active; `›` = expandable row.

### Tab model (DECIDED 2026-06-24)

- **Tabs:** `Overview` + provider tabs. No Instance, no agent tabs. Usage/quota is a per-provider-account concept; one agent (e.g. `claude`) can route to several providers (Anthropic / Z.AI / MiniMax / Kimi), so each tab is the **provider/account**, and the focused pane maps to whichever provider it is routed to. `Overview` is the cross-provider glance (operator-requested, kept).
- **Tab set + order:** `Overview · OpenAI · Anthropic · Amp · xAI · Z.AI · Kimi · MiniMax` (provider-org labels, no slashes; order adjustable). Each = the single forwarded account for that provider in this instance (same auth as the docker instance).
- **Default selected tab** = the focused pane's provider.
- **Reuse the shared `jackin_tui::components::TabStrip`** (the settings / workspace-editor widget) — NOT a custom strip. Adopt its focus model: GREEN underline = tab bar focused → `←/→` switch provider; `Tab` → focus moves into content (underline turns WHITE) → scroll/iterate content; `Tab`/`Esc` returns focus to the strip. Same behavior as settings tabs.
- **Dropped:** Instance (per-container ledger). Overview kept. **Open consequence:** per-instance spend (today/since-start/by-codename) loses its home — decide later whether it surfaces in the status bar, a CLI view, or a future tab. Roadmap `:122` forbids mixing instance numbers into account tabs.

> All preview blocks below use this shared tab bar (label line + `━` underline under
> the active tab, exactly like jackin's settings/capsule tab bar):
> `Overview · OpenAI · Anthropic · Amp · xAI · Z.AI · Kimi · MiniMax`.

### Overview

```text
┌ Usage ─────────────────────────────────────────────────────────────────────────────┐
│  Overview   OpenAI   Anthropic   Amp   xAI   Z.AI   Kimi   MiniMax                 │
│  ━━━━━━━━                                                                          │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Focused provider: Anthropic · alexey@chainargos.com · Max                          │
│                                                                                    │
│ OpenAI      19% left    Resets 13h 6m   fresh · provider                           │
│ Anthropic   55% left    Resets 1d 22h   fresh · provider                           │
│ Amp         100% left   Resets now      fresh · provider                           │
│ xAI         18% left    Resets 6d 15h   fresh · provider                           │
│ Z.AI        99% left    Resets 3d       fresh · provider                           │
│ Kimi        100% left   Resets 2m       fresh · provider                           │
│ MiniMax     1% used     Resets 4d       fresh · provider                           │
│                                                                                    │
│ Most constrained: xAI 18% left                                                     │
│                                                                                    │
│ Enter Provider detail    r Refresh focused    Esc Close                            │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

### OpenAI

```text
┌ Usage ─────────────────────────────────────────────────────────────────────────────┐
│  Overview   OpenAI   Anthropic   Amp   xAI   Z.AI   Kimi   MiniMax                   │
│             ━━━━━━                                                                   │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Codex                                                      alexey@chainargos.com      │
│ Updated 2m ago                                                              Pro 20x    │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account availability                                                                   │
│                                                                                        │
│ Session                                                                                │
│ ██████████████████████████████████████████████████████·                                │
│ 97% left   6% in reserve                            Resets in 4h 33m · Lasts until reset│
│                                                                                        │
│ Weekly                                                                                  │
│ ██████████·············································                                   │
│ 19% left   11% in reserve                           Resets in 13h 6m · Lasts until reset│
│                                                                                        │
│ Codex Spark 5-hour                                                                      │
│ ███████████████████████████████████████████████████████                                │
│ 100% left                                                          Resets in 4h 58m     │
│                                                                                        │
│ Codex Spark Weekly                                                                      │
│ ███████████████████████████████████████████████████████                                │
│ 100% left                                                          Resets in 6d 23h     │
│                                                                                        │
│ Limit Reset Credits                              2 manual resets · Next expires 17d 17h │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account cost and tokens                                                                 │
│ Today              $186.32        30d cost          $3,544.59                           │
│ 30d tokens         5B             Latest tokens     243M                                │
│                                                                                        │
│ ▁▁▂▃▂▁▄▆█▂▁▁▁▃▄▃▂                                                                       │
│ Top model: gpt-5.5                                                                      │
│ Estimated from local Codex logs at API rates; token totals are local                   │
│                                                                                        │
│ Credits                                          0 left · 1K tokens                     │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Buy Credits         provider billing action — opens outside Capsule                  › │
│ Cost                today / 30d / tokens breakdown                                    › │
│ Subscription Utilization   Weekly 19% left · Resets in 13h 6m                         › │
│ Add Account         configure provider auth outside jackin'                           › │
│ Usage Dashboard     read-only provider account summary                               › │
│ Status Page         Partial System Degradation — Updated 13h ago                     › │
│                                                                                        │
│ Source: provider API · authoritative · fresh · Updated 2m ago                          │
│ Status: ok                                                                             │
│                                                                                        │
│ r Refresh    Tab Switch provider    Esc Close                                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

### Anthropic

```text
┌ Usage ─────────────────────────────────────────────────────────────────────────────┐
│  Overview   OpenAI   Anthropic   Amp   xAI   Z.AI   Kimi   MiniMax                    │
│                      ━━━━━━━━━                                                        │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Claude                                                                          Max    │
│ Updated just now                                                                        │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account availability                                                                   │
│                                                                                        │
│ Session                                                                                │
│ ███████████████████████████████████████████████████·····                               │
│ 92% left   10% in reserve                            Resets in 4h 5m · Lasts until reset│
│                                                                                        │
│ Weekly                                                                                  │
│ ██████████████████████████████·························                                  │
│ 55% left   27% in reserve                           Resets in 1d 22h · Lasts until reset│
│                                                                                        │
│ Sonnet                                                                                  │
│ ███████████████████████████████████████████████·······                                 │
│ 85% left                                                           Resets in 1d 22h     │
│                                                                                        │
│ Daily Routines                                                                          │
│ ███████████████████████████████████████████████████████                                │
│ 100% left                                                                               │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account cost and tokens                                                                 │
│ Today              $266.63        30d cost          $7,231.50                           │
│ 30d tokens         11B            Latest tokens     425M                                │
│                                                                                        │
│ ▁▂▁▃▂▂▃▃▂▁▁▃█▅▁▁▂▂▃                                                                     │
│ Top model: claude-opus-4-8                                                              │
│ Estimated from local Claude logs at API rates; token totals are local                  │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Cost                today / 30d / tokens breakdown                                    › │
│ Subscription Utilization   Weekly 55% left · Resets in 1d 22h                         › │
│ Add Account         configure provider auth outside jackin'                           › │
│ Usage Dashboard     read-only provider account summary                               › │
│ Status Page         All systems operational — Updated 5m ago                          › │
│                                                                                        │
│ Source: provider API · authoritative · fresh · Updated just now                        │
│ Status: ok                                                                             │
│                                                                                        │
│ r Refresh    Tab Switch provider    Esc Close                                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

> Note: `Extra usage` and `Opus` buckets render here **only when the API returns them** (this account showed neither). When present, `Extra usage` shows `Monthly cap: <currency> X / Y · N% used`.

### Z.AI

```text
┌ Usage ─────────────────────────────────────────────────────────────────────────────┐
│  Overview   OpenAI   Anthropic   Amp   xAI   Z.AI   Kimi   MiniMax                    │
│                                              ━━━━                                     │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ GLM / Z.AI                                                                     GLM Pro │
│ Updated just now                                                                        │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account availability                                                                   │
│                                                                                        │
│ Tokens                                                                                  │
│ ██████████████████████████████████████████████████████·                                │
│ 99% left                                                              Resets in 3d      │
│                                                                                        │
│ MCP                                                                                     │
│ ███████████████████████████████████████████████████████                                │
│ 100% left   0 / 100 (100 remaining)                                  Resets in 19d      │
│                                                                                        │
│ 5-hour                                                                                  │
│ ███████████████████████████████████████████████████████                                │
│ 100% left                                                       Resets 5 hours window   │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Hourly Usage        recent hourly token breakdown                                    › │
│ Usage Dashboard     read-only provider account summary                               › │
│                                                                                        │
│ Source: provider API · authoritative · fresh · Updated just now                        │
│ Status: ok                                                                             │
│                                                                                        │
│ r Refresh    Tab Switch provider    Esc Close                                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

### MiniMax

```text
┌ Usage ─────────────────────────────────────────────────────────────────────────────┐
│  Overview   OpenAI   Anthropic   Amp   xAI   Z.AI   Kimi   MiniMax                    │
│                                                            ━━━━━━━                    │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ MiniMax                                                                   Coding Plan  │
│ Updated just now                                                                        │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account availability                                                                   │
│                                                                                        │
│ General · 5h                                                                            │
│ ·······················································                                   │
│ 0% used   Usage: 0 / 100                                              Resets in 1 hour  │
│                                                                                        │
│ General · Weekly                                                                        │
│ █······················································                                   │
│ 1% used   Usage: 1 / 100                                              Resets in 4 days  │
│                                                                                        │
│ Video                                                                                   │
│ ·······················································                                   │
│ 0% used   Usage: 0 / 100                                             Resets in 15 hours │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Usage Dashboard     read-only provider account summary                               › │
│                                                                                        │
│ Source: provider API · authoritative · fresh · Updated just now                        │
│ Status: ok                                                                             │
│                                                                                        │
│ r Refresh    Tab Switch provider    Esc Close                                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

> MiniMax uses **`% used`** (not `% left`) and a `Usage: N / 100` count line; one bucket per `model_remains[]` entry × window (`<model> · <window>`).

### Kimi

```text
┌ Usage ─────────────────────────────────────────────────────────────────────────────┐
│  Overview   OpenAI   Anthropic   Amp   xAI   Z.AI   Kimi   MiniMax                    │
│                                                     ━━━━                              │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Kimi                                                                  Kimi For Coding  │
│ Updated just now                                                                        │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account availability                                                                   │
│                                                                                        │
│ Weekly                                                                                  │
│ ███████████████████████████████████████████████████████                                │
│ 100% left                                                              Resets in 2m     │
│                                                                                        │
│ Rate Limit                                                                              │
│ ███████████████████████████████████████████████████████                                │
│ 100% left   60% in reserve                          Resets in 2h 2m · Lasts until reset │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Usage Dashboard     read-only provider account summary                               › │
│                                                                                        │
│ Source: provider API · authoritative · fresh · Updated just now                        │
│ Status: ok                                                                             │
│                                                                                        │
│ r Refresh    Tab Switch provider    Esc Close                                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

> Data comes from the Kimi Code API endpoint `GET https://api.kimi.com/coding/v1/usages` (Bearer) — the F4 fix path.

### Amp

```text
┌ Usage ─────────────────────────────────────────────────────────────────────────────┐
│  Overview   OpenAI   Anthropic   Amp   xAI   Z.AI   Kimi   MiniMax                    │
│                                  ━━━                                                  │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Amp                                                          alexey@zhokhov.com        │
│ Updated 1m ago                                                            Amp Free     │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account availability                                                                   │
│                                                                                        │
│ Amp Free                                                                                │
│ ███████████████████████████████████████████████████████                                │
│ 100% left                                                              Resets now       │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Credits                                              Individual credits: $4.94          │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Usage Dashboard     read-only provider account summary                               › │
│                                                                                        │
│ Source: provider API · authoritative · fresh · Updated 1m ago                          │
│ Status: ok                                                                             │
│                                                                                        │
│ r Refresh    Tab Switch provider    Esc Close                                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

> Data via `POST https://ampcode.com/api/internal?userDisplayBalanceInfo` with `AMP_API_KEY` (Bearer) — the F7 fix path.

### xAI

```text
┌ Usage ─────────────────────────────────────────────────────────────────────────────┐
│  Overview   OpenAI   Anthropic   Amp   xAI   Z.AI   Kimi   MiniMax                    │
│                                        ━━━                                            │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Grok                                                        alexey@chainargos.com      │
│ Updated 1m ago                                                            SuperGrok     │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Account availability                                                                   │
│                                                                                        │
│ Weekly                                                                                  │
│ ██████████·············································                                   │
│ 18% left                                                              Resets in 6d 15h  │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ Usage Dashboard     read-only provider account summary                               › │
│ Status Page         All systems operational — Updated 1h ago                          › │
│                                                                                        │
│ Source: provider API · authoritative · fresh · Updated 1m ago                          │
│ Status: ok                                                                             │
│                                                                                        │
│ r Refresh    Tab Switch provider    Esc Close                                          │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

> Data via Grok ACP RPC `x.ai/billing` (gRPC-web `GetGrokCreditsConfig` fallback) — the F8 fix path.

**Open design questions for these previews:**
- Account-action rows (`Cost`/`Subscription Utilization`/`Buy Credits`/`Add Account`/`Usage Dashboard`/`Status Page`): keep CodexBar's set per provider as drawn? Or trim further for a terminal?
- `Source`/`Status` footer: keep two lines as in the roadmap preview (`Source: …` / `Status: ok`), or collapse to one?
- Sparkline + `Top model` + cost grid: keep on every provider that has local spend, or only when local logs exist (else hide the section vs show `unavailable`)?
- Bucket label alignment to CodexBar (`Tokens`/`MCP`/`5-hour` for z.ai; `Rate Limit` for Kimi; `<model> · <window>` for MiniMax) — confirm.

## Status bar / compact signal (operator sign-off)

The always-on signal lives in the branch context bar (bottom line). It is the
first place the operator notices "almost out" before sending an expensive prompt.
Per roadmap `:641-695`: follows the **focused pane's** provider+account, shows the
**most-constrained bucket**, prefers remaining quota, degrades to lifecycle state
honestly, elides gracefully on tight terminals. Numbers below are the live
CodexBar values.

### Full form (healthy) — `Branch · <branch> · <Provider> · <account> <Window>: <used> · <left> · resets <countdown>` … `<instance-id>`

```text
 Branch · feature/auth · Claude · alexey@chainargos.com  Weekly: 45% used · 55% left · resets 1d 22h         5vvkqps1
 Branch · docs · Codex · alexey@chainargos.com  Weekly: 81% used · 19% left · resets 13h 6m                  5vvkqps1
 Branch · main · GLM / Z.AI  Tokens: 1% used · 99% left · resets 3d                                          5vvkqps1
 Branch · main · Grok · alexey@chainargos.com  Weekly: 82% used · 18% left · resets 6d 15h                   5vvkqps1
 Branch · main · MiniMax  General · Weekly: 1% used · 99% left · resets 4 days                                5vvkqps1
 Branch · main · Kimi  Weekly: 0% used · 100% left · resets 2m                                               5vvkqps1
 Branch · main · Amp · alexey@zhokhov.com  Amp Free: 0% used · 100% left · resets now                        5vvkqps1
```

The displayed window = the focused provider's **most-constrained** bucket (lowest
remaining), e.g. Claude shows `Weekly 55%` not `Session 92%`; Codex shows
`Weekly 19%` not `Session 97%`.

### Lifecycle states (no trustworthy %)

```text
 Branch · docs · Codex · stale · last 19% left · updated 24m ago        5vvkqps1
 Branch · main · Amp · needs login                                       5vvkqps1
 Branch · main · Kimi · needs token                                      5vvkqps1
 Branch · main · MiniMax · unsupported                                   5vvkqps1
 Branch · main · Claude · usage unavailable   (daemon disconnected)      5vvkqps1
```

### Compact / elision (tight terminal — drop reset → branch → used → quota detail; keep provider + remaining/state)

```text
Claude Weekly 55% left
Claude 55% left
Claude stale
Codex 19% left
Amp login
```

### Status-bar rules (binding)

- Follows the focused tab/pane provider+account; never a global average or a sibling agent's quota.
- Most-constrained bucket; prefer remaining (`55% left`) over used. Show both only when room.
- No trustworthy %: show `login` / `secret` / `stale` / `unsupported` / `unavailable` — never a fabricated number.
- Daemon disconnected → `usage unavailable`, no percentage.
- Shell tab with no active agent → no usage signal.
- Color advisory only; text/glyph must read in monochrome.
- The signal is secondary to tab label + branch/repo context; elide in the order above.

## Display approach — conclusion (how usage affects jackin)

Two surfaces, one daemon-fed authenticated cache (never recomputed live per-surface — fixes F1):

1. **Compact status signal** (branch context bar, always-on) — focused provider's most-constrained bucket; the "am I about to run out" glance.
2. **Usage overlay** (`u` prefix / palette `Usage`) — the detail modal. Default view = **focused provider tab**, not Overview (roadmap `:633`). Tabs: `Overview`, `Instance`, then provider/account tabs (`OpenAI / Codex`, `Anthropic / Claude`, `Amp`, `xAI / Grok`, `GLM / Z.AI`, `Kimi`, `MiniMax`).

Cross-cutting display decisions (these become the binding contract synced to the roadmap):

- **Per-provider section sets** — render only the sections a provider actually has (Codex: Spark/Limit Reset Credits/Credits; Claude: Sonnet/Daily Routines/Extra usage; z.ai: Tokens/MCP/5-hour; MiniMax: model×window; Kimi: Weekly/Rate Limit; Amp: Amp Free/Credits; Grok: Weekly). No uniform action-row scaffold on every tab.
- **Honest provenance everywhere** — `Source: <source> · <confidence> · <status> · <updated>` reflects the real fetch; `Status Page` shows real provider incident health; availability vs spend labeled separately; surface real errors (no `.ok()` swallowing). Fixes F6.
- **% left vs % used** per provider native convention; pace line (`On pace`/`N% in reserve`/`N% in deficit`) where the provider exposes utilization+window.
- **Conditional buckets** (Opus, Extra usage, Spark, Credits) render only when present.
- **Keyboard footer** stays jackin-native (`r Refresh · Tab Switch provider · Esc Close`), not CodexBar's menu footer.
- **One authenticated source** — status bar and overlay read the same daemon cache key; a failed refresh preserves last-good (fixes F1).

What does NOT change: the `C-\` menu, tab strip behavior, branch/repo context precedence. The overlay reuses the existing Capsule dialog component; the status signal extends the existing branch context bar segment.

**Decisions still needed before this is final** (carried from earlier Open questions): token-refresh-writes-container-only policy; web/cookie fallback opt-in (API-token-only likely suffices since all 7 worked in CodexBar); keep-vs-trim account-action rows; `Source`/`Status` one line vs two.

---

> **Next:** once you sign off the overlay + status-bar previews and the decisions
> above, I sync all agreed decisions into `capsule-usage-quota-overlay.mdx` —
> removing what is genuinely done+zero-issues, keeping/correcting what is not yet
> implemented or inaccurate, so the roadmap tracks only remaining work.

## Formal Implementation Plan

Principle: **byte-for-byte parity with CodexBar's data-extraction approach** for
every provider. CodexBar works on the operator host; jackin must use the same
credential sources, endpoints, headers, field maps, and refresh flows. Where
jackin runs in-container and cannot do what a host app does (Keychain, host
writes), it forwards the same credential and calls the same endpoint from inside.

### Reference & source-of-truth anchors

- **CodexBar reference (how it works):** `github.com/steipete/CodexBar`. The clone used for this review is ephemeral (scratchpad); the exact file:line references are captured per work-stream below and must be transcribed into the new `reference/usage-telemetry/` docs so the project no longer depends on a live clone. CodexBar provider sources live under `Sources/CodexBar/Providers/<Provider>/` and `Sources/CodexBarCore/Providers/<Provider>/`.
- **jackin adapter source of truth:** `crates/jackin-capsule/src/usage.rs` (all `*_snapshot` / `fetch_*` / parse fns) and `crates/jackin-capsule/src/usage/tests.rs`.
- **jackin credential forwarding source of truth:** `crates/jackin-runtime/src/instance/auth.rs`, `crates/jackin-runtime/src/instance.rs`, `crates/jackin-runtime/src/runtime/launch.rs`, `crates/jackin-capsule/src/runtime_setup.rs`.
- **Render source of truth:** `crates/jackin-capsule/src/tui/components/dialog.rs` (usage dialog) + the `FocusedUsageView` shape in `usage.rs`.
- **Binding visual contract:** the previews in `docs/content/docs/reference/roadmap/capsule-usage-quota-overlay.mdx:763-805` (Claude), Codex/Overview/Instance previews above them. (Pending: the operator's CodexBar host screenshots become the visual acceptance reference alongside these previews.)

### CodexBar source map (where each provider is implemented)

| Provider | CodexBar key files |
|---|---|
| Claude | `CodexBarCore/Providers/Claude/ClaudeOAuth/{ClaudeOAuthCredentials,ClaudeOAuthUsageFetcher,ClaudeOAuthCredentialModels}.swift`, `Providers/Claude/{ClaudeUsageFetcher,ClaudePlan}.swift` |
| Codex | `CodexBarCore/Providers/Codex/CodexOAuth/{CodexOAuthCredentials,CodexOAuthUsageFetcher,CodexTokenRefresher}.swift`, `CodexAdditionalRateLimitMapper.swift`, `CodexCLISession.swift` |
| Kimi | `Providers/Kimi/{KimiUsageFetcher,KimiModels,KimiSettingsReader,KimiProviderDescriptor,KimiCookieHeader}.swift`, `Providers/KimiK2/*`, `Providers/Moonshot/*` |
| MiniMax | `Providers/MiniMax/{MiniMaxUsageFetcher,MiniMaxAPIRegion,MiniMaxAuthMode}.swift`, `docs/minimax.md` |
| Grok | `Providers/Grok/{GrokAuth,GrokRPCClient,GrokWebBillingFetcher,GrokProviderDescriptor,GrokStatusProbe}.swift` |
| Amp | `Providers/Amp/{AmpUsageFetcher,AmpCLIProbe,AmpProviderDescriptor,AmpSettingsReader}.swift` |
| Z.AI/GLM | `Providers/Zai/{ZaiUsageStats,ZaiAPIRegion,ZaiSettingsReader}.swift` |

---

### WS0 — Establish parity harness & source-of-truth doc (do first)

- **Why:** every later change must be checkable against CodexBar without a live clone.
- **Steps:** (1) Create `docs/content/docs/reference/usage-telemetry/index.mdx` and transcribe the per-provider recipes (creds → endpoint → headers → field map → refresh) from the CodexBar section of this review. (2) Add a `usage.rs` doc-comment header pointing to that page as the contract. (3) Add the docs page to the PR docs gate / TODO stale-docs checklist so any adapter change updates it.
- **Verify:** `bun run check:roadmap-sidebar` / `check:repo-links` clean; doc lists all 8 providers.

### WS1 — Credential delivery into the container (dominant fix; unblocks F1·F2·F7·F8)

- **CodexBar approach:** reads creds live on host (env → keychain cache → file → OS Keychain). jackin's parity = forward the *same* resolved credential into the container reliably, then read it in-container.
- **Problem:** forwarding is `AuthForwardMode::Sync`-gated and (Codex) first-launch-only via a `.done` marker; non-Sync roles or container reuse → no/stale creds.
- **jackin targets:** `auth.rs` (Claude `:514`, Codex `:127-160`, Grok `:888-919`, Amp `:614-655`), `launch.rs` (mounts), `runtime_setup.rs` (`setup_claude` `:301`, `setup_codex` `:413-431`, marker `should_copy_auth` `:252`).
- **Steps:** (1) Re-forward credentials on every launch (refresh the in-container copy when the host/forwarded source is newer), not only when the marker is absent. (2) Surface forwarding outcome per provider in the launch summary (which creds forwarded, which mode) so "needs login" is explainable. (3) Decide policy: for usage-only reads, forward credentials independent of the agent's `AuthForwardMode` where the operator opted into telemetry, or document that telemetry requires Sync. (4) Prefer env-key REST paths (WS6/WS7) so file-forwarding isn't the only route.
- **Verify:** in a freshly launched container for each provider, the forwarded credential file exists and matches the host source; after a host re-login + container restart, the container copy updates.

### WS2 — Claude adapter (F1 overlay, F9 refresh, F10 fields)

- **CodexBar refs:** `ClaudeOAuthUsageFetcher.swift` (endpoint/headers/UA detection), `ClaudeUsageFetcher.swift:941-1099` (bucket assembly, extra_usage cents/100), `ClaudePlan.swift:53-141` (subscriptionType→rateLimitTier), `ClaudeOAuthCredentials.swift:993-1059` (refresh).
- **jackin targets:** `usage.rs` `claude_snapshot:446`, `load_claude_oauth_credentials:1442`, `fetch_claude_oauth_usage:1566`, `ClaudeOAuthUsageResponse:1466`, extra-usage fmt `:3311`; overlay paths `daemon/input_dispatch.rs:148,158,223,298`, `usage.rs` cache `:111-156,1276-1313`, `multiplexer_utils.rs:211-247`.
- **Steps:** (F1) one canonical provider cache key for status-bar + overlay; extend `preserve_cached_quota_on_stale_refresh` to preserve last-good on `NeedsLogin`/`Error`; don't `force_refresh` on overlay open. (F9) parse `expiresAt`/`refreshToken`; refresh against `platform.claude.com/v1/oauth/token` client_id `9d1c250a-e61b-44d9-88ed-5944d1962f5e`, writing **only the container copy** (never host — see Open questions). (F10) divide extra_usage by 100 + default USD; add bucket aliases (`seven_day_oauth_apps`, routines aliases, `iguana_necktie`); plan fallback to `rateLimitTier`; detect Claude Code version for UA.
- **Verify:** overlay Claude tab == status bar live quota at all times incl. after `r`; after token expiry the capsule refreshes and recovers; extra_usage shows correct currency/amount; Daily Routines survives alias.

### WS3 — Codex adapter (F2)

- **CodexBar refs:** `CodexOAuthUsageFetcher.swift` (URL/headers — already matched), app-server RPC authority (`CodexCLISession.swift`), `CodexTokenRefresher.swift` (refresh, client `app_EMoamEEZ73f0CkXaXp7hrann`).
- **jackin targets:** `usage.rs` `codex_snapshot:559`, `codex_account_label:4480`, `fetch_codex_rpc_usage:1914`, `fetch_codex_oauth_usage:2323`, launch gate `ManagedCliLaunchGate:1719`; `runtime_setup.rs setup_codex:413`.
- **Steps:** (1) derive the account identity from the `id_token` JWT (decode `email`/`sub`) or the app-server RPC, not from a `tokens.email` field that real auth.json lacks; (2) decouple `status` from the cooldown-gated RPC — a successful `/wham/usage` quota fetch must lift status out of `NeedsLogin` even without the email; (3) refresh expired tokens (container copy only); (4) refresh the in-container `auth.json` on relaunch (WS1).
- **Verify:** Codex tab shows live plan + Session/Weekly/Spark matching the running CLI `/usage`, even when `codex app-server` can't be spawned in the capsule.

### WS4 — Kimi credential-type branching (F4)

- **CodexBar refs:** `KimiUsageFetcher.swift:12-40` (Code-API `GET https://api.kimi.com/coding/v1/usages`, Bearer), `:54-118` (web POST + JWT session headers), `KimiModels.swift:7-10` (Code-API response), `KimiProviderDescriptor.swift:42-93` (strategy dispatch), `KimiSettingsReader.swift:4-6`.
- **jackin targets:** `usage.rs` dispatch `:390-396`, `kimi_snapshot:827`, `fetch_kimi_usage:2774`, `load_kimi_local_token:2801`, parse `:2603-2664`, `kimi_bucket:2738`.
- **Steps:** (1) classify the credential: Code-API token (`KIMI_CODE_API_KEY` or `~/.kimi-code/credentials/kimi-code.json` `access_token`) vs web `kimi-auth` JWT. (2) Code-API token → `GET https://api.kimi.com/coding/v1/usages` `Authorization: Bearer`, `Accept: application/json`; add `KimiCodeAPIUsageResponse` shape (top-level `usage` + `limits[]`). (3) web JWT → keep the POST but add the JWT-derived `x-msh-device-id`/`x-msh-session-id`/`x-traffic-id` headers + browser UA. (4) `.auto`: try API then web (CodexBar order). (5) accept reset aliases.
- **Verify:** with the forwarded Kimi-Code token (CLI works), Kimi tab shows live Weekly + 5-hour buckets via `api.kimi.com`.

### WS5 — MiniMax fallback + loop/error (F3)

- **CodexBar refs:** `MiniMaxUsageFetcher.swift:27-108` (web/cookie primary), `:110-149` (API token), `:128-148/235-243/403-411` (retry across hosts incl. networkError→next), `MiniMaxAPIRegion.swift:22-63`, `docs/minimax.md:21-22` (auto falls back to web).
- **jackin targets:** `usage.rs` dispatch `:398-406`, `minimax_snapshot:910`, `fetch_minimax_usage:3057`, `resolve_minimax_remains_urls_from:3089`, parse `:2919-2998`, `minimax_bucket:3011`.
- **Steps:** (1) don't abort the URL loop on transport error — treat `networkError` as "try next" (`continue`), matching CodexBar; (2) add the web/cookie fallback path (platform/www hosts) for coding-plan keys, OR if web-cookie is out of scope for Capsule, render honest `unsupported` with the *real* reason; (3) stop discarding the error (`.ok()`) — surface the true `status_code`/`status_msg`.
- **Verify:** present coding-plan key resolves on whichever host/path works; failures show the true cause, not generic "endpoint unavailable."

### WS6 — Grok ungate + forward + fallback (F8)

- **CodexBar refs:** `GrokRPCClient.swift:98-116` (RPC `x.ai/billing`), `GrokWebBillingFetcher.swift:66-172` (gRPC-web fallback), `GrokAuth.swift:87-172` (auth.json scope map), `GrokStatusProbe.swift:85-111` (probe not gated on cred file).
- **jackin targets:** `usage.rs` grok `:756-821`, `fetch_grok_rpc_billing:2178`, `grok_account_label_or_presence:4503`; `auth.rs:888-919`, `instance.rs:874-952` (forwarding + binary staging).
- **Steps:** (1) ensure `~/.grok/auth.json` is forwarded (WS1) and binary staged (already done); (2) don't gate the RPC on `has_credentials` — run `grok agent stdio` and report its real error; (3) add the grok.com gRPC-web `GetGrokCreditsConfig` fallback using the auth.json `key`.
- **Verify:** Grok tab shows live credits/on-demand matching the working CLI.

### WS7 — Amp env-key REST + binary (F7)

- **CodexBar refs:** `AmpUsageFetcher.swift:104,246-257` (`POST https://ampcode.com/api/internal?userDisplayBalanceInfo`, Bearer `AMP_API_KEY`), `AmpCLIProbe.swift` (CLI), `:259-280` (web).
- **jackin targets:** `usage.rs` amp `:684-749`, `fetch_amp_cli_usage` (`amp --no-color usage`) `:3365`, parse `:3405-3429`; `instance.rs:815-832` (forward secrets.json; no binary staging).
- **Steps:** (1) add the `AMP_API_KEY` → `ampcode.com/api/internal` REST path as primary (CodexBar's primary; container-friendly env-key like Z.AI); (2) if relying on `amp usage`, stage the amp binary into the container PATH (mirror grok staging); (3) keep secrets.json forwarding as a fallback.
- **Verify:** with `AMP_API_KEY` set (no CLI login), Amp tab shows Free quota + credits.

### WS8 — Render contract + provenance honesty (F5, F6)

- **Binding contract:** roadmap previews `:763-805`; operator CodexBar screenshots (pending).
- **jackin targets:** `tui/components/dialog.rs` (usage dialog builder ~`:1307-1320`), `FocusedUsageView` in `usage.rs`, all `*_snapshot` source/confidence/status assignment.
- **Steps:** (1) render exactly the preview sections in order (header, Account availability meters incl. all buckets, two-column Account cost and tokens, sparkline, `Top model`, single provenance line, two-line `Source`/`Status` footer); (2) delete scaffold rows (`Cost`, duplicate `Status`/`Status Page`); (3) resolve Step-5-vs-line-94 account-action-rows contradiction (operator decision); (4) `Source`/`Status` must render the snapshot's real `(source, confidence, status)` — no hard-coded `authoritative · fresh`; (5) label Account-availability (authoritative) vs Workspace-spend (estimate) provenance separately; (6) stop swallowing fetch errors so failures show the true reason.
- **Verify:** side-by-side diff of each provider tab vs preview/CodexBar screenshot across the three width bands; each label once; provenance matches the real fetch outcome.

### WS9 — Docs + roadmap reconciliation (M1)

- **Steps:** create `reference/usage-telemetry/{index,credential-forwarding,cost-and-provenance}.mdx` (Documentation Plan); reopen falsely-"landed" roadmap steps; fix `capsule-usage-quota-overlay.mdx:32` Keychain wording; resolve Step-5/line-94 contradiction; update roadmap overview + sidebar `meta.json`.
- **Verify:** `bun run check:roadmap-sidebar`, `check:repo-links`, `check:links:fresh` clean; roadmap status reflects real state.

### Suggested sequencing

WS0 → WS1 (unblocks the cluster) → WS2/WS3 (Claude/Codex, highest-value) →
WS4/WS5/WS6/WS7 (per-provider, parallelizable) → WS8 (render, after data is real)
→ WS9 (docs, continuous; finalize at end). Each WS lands with adapter unit/snapshot
tests **plus** an in-container live check, since the roadmap's failure was trusting
isolated tests over the live overlay path.

## Open questions for the operator

- Which MiniMax and Kimi credential types are configured for these panes (coding-plan API key? web token? OAuth)? This decides F3/F4 between "fix the endpoint" and "document unsupported."
- Enterprise Claude (Scentbird, 401) — still in scope for this PR, or tracked separately per roadmap line 92?
- **Token-refresh policy (blocks F9 / WS2 / WS3).** CodexBar refreshes expired OAuth tokens and rewrites the auth files. Roadmap line 104 says jackin **never writes host credentials**. Proposal: refresh in-container and write **only the container credential copy** (`/home/agent/.claude/.credentials.json`, `~/.codex/auth.json`), never the host Keychain/file. Confirm this is acceptable, or keep the honest `needs login`/`stale` degradation and require the operator to re-forward.
- **Web/cookie fallbacks (F3 MiniMax, F4 Kimi web, F8 Grok web).** CodexBar imports browser cookies / scrapes web endpoints as fallbacks. Roadmap line 105 says Capsule never imports cookies. Confirm: implement only the API-token endpoints (and render honest `unsupported` when only a web session exists), or allow an explicit operator opt-in for cookie sources?
- **Account-action rows (F5).** Step 5 says remove scaffold; line 94 says retain `Cost`/`Subscription Utilization`/`Buy Credits`/`Add Account`/`Usage Dashboard`/`Status Page`. The previews omit them. Match the preview (drop), or keep them in a separated non-duplicated section?
- **CodexBar reference durability.** The clone is ephemeral. OK to transcribe the recipes into `reference/usage-telemetry/` as the durable record (recommended), rather than vendoring CodexBar source?
