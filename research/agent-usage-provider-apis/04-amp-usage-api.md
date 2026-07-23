# 04 — Amp usage API

Questions: (1) Which official Amp API endpoint(s) expose usage/quota — the "Amp Free" window, percent left, reset info? (2) Which endpoint(s) expose credit balances, both individual and workspace/team, and under which field names? (3) What auth does the usage surface take, and where do credentials live on disk (location/type only)? (4) Is the plan label ("Amp Free" vs paid) an API field or client-side inference? (5) Which display elements have no direct API source and need client-side computation?
Informs: jackin-desktop
Vetted: 2026-07-24
Method: web + codebase cross-reference. Primary artifact analyzed: the official Amp CLI binary `@ampcode/cli` v`0.0.1784824164-g9bdb69` (npm `latest` on 2026-07-24), platform package `@ampcode/cli-darwin-arm64`, strings extracted from the shipped `package/amp` executable. Minified identifiers below (`I5R`, `h1T`, …) are quoted verbatim from that bundle. Clean-room note: CodexBar/OpenUsage source was not read; no endpoints were taken from them. No embedded instructions were encountered in any fetched page; all fetched content was treated as data.

## Follow-up correction

Chapter [11](11-amp-daily-followup.md) is the current authority for Amp
Free cadence and line shape. An operator-requested source-level verification
found a redacted live `amp usage` capture, merged regression fixture, and
current parser proving:

- current Amp Free is `N% remaining today (resets daily)`;
- `Workspace <name>: $N remaining` can occur in the same
  `userDisplayBalanceInfo.displayText`;
- the daily line has no exact reset timestamp;
- daily is proven only for Amp Free; paid subscription `displayText` remains
  uncaptured.

Accordingly, the hourly-dollar/reset findings below describe the January
format and are superseded for implementation. The endpoint, bearer auth,
single-`displayText` response shape, and credential findings remain current.

## Findings

### 1. Endpoint(s) exposing usage/quota (Amp Free window, percent left, reset)

- There is exactly one usage/balance API call in the official Amp CLI: the internal JSON-RPC method `userDisplayBalanceInfo`, sent as `POST {AMP_URL}/api/internal?userDisplayBalanceInfo` with body `{"method":"userDisplayBalanceInfo","params":{}}`. The generic client builds every internal call as `JSON.stringify({method:R,params:h})` posted to `"/api/internal?"+encodeURIComponent(R)` with `Content-Type: application/json` (gzip + `Content-Encoding: gzip` when the body is large) — extracted from the `h1T` proxy function in the `@ampcode/cli` binary, https://registry.npmjs.org/@ampcode/cli-darwin-arm64/-/cli-darwin-arm64-0.0.1784824164-g9bdb69.tgz (confidence: HIGH)
- The `amp usage` subcommand ("Show your current Amp usage and credit balance") is a thin wrapper: `H0.userDisplayBalanceInfo({},{config:t})` then prints `c.result.displayText` — function `Qh0` in the same binary (confidence: HIGH)
- Response envelope is `{ok:boolean, result:{...}, error?:{code,message}}`; the CLI checks `c.ok`, `c.error.code==="auth-required"`, then reads `c.result.displayText` — `Qh0` in the binary; matches the `result`-unwrapping in `crates/jackin-usage/src/usage/amp.rs:140` (confidence: HIGH)
- The declared response schema for `userDisplayBalanceInfo` is `[K.object({}), I5R]` where `I5R=K.object({displayText:K.string()})` — the entire usage payload is ONE server-rendered text string. No structured fields for remaining, limit, percent, or reset exist in the API response — RPC schema map in the binary (confidence: HIGH)
- The CLI's renderer proves the text's shape: it splits `displayText` on newlines, treats everything before the first `:` as a bold label, and tokenizes content with the regex `(\+?\$[\d,.]+(?:\/\$[\d,.]+)?(?:\/hour)?)|(\busage paid\b|\bpaid\b)|(\s-\s(?=https?:\/\/))|((https?:\/\/)[^\s]+)|([^$\s][^\s]*)` — i.e. the server text carries `$remaining/$limit` pairs, `$X/hour` replenishment rates, "paid" markers, and URLs — functions `Uh0`/`e3T`/`Bh0` in the binary (confidence: HIGH)
- Amp Free reset mechanics per the January 2026 launch post: hourly replenishment — "The free credit grant replenishes hourly, giving you a total of $10 worth of credits per day or roughly $300 of credits per month" — https://ampcode.com/news/amp-free-frontier (2026-01-08) (confidence: HIGH for the January statement). **Superseded-risk (vet round 2, see chapter 10 Q3):** dated July 2026 evidence shows the post-subscription free-tier `displayText` saying "resets daily", a midnight-UTC daily reset rule extracted from the ampcode.com/settings production frontend (`Date.UTC(y,m,d+1)`), empirically confirmed 90%→100% at 00:02 UTC on 2026-07-20, after a server-side `displayText` format change on 2026-07-11. The January hourly-replenishment model and the July midnight-UTC model conflict; either the model changed circa 2026-07-11 or credit-replenishment and percentage-reset coexist as separate lines. Unresolved without an operator capture.
- Default server URL is `https://ampcode.com` (hidden settings key `url`, "The Amp server URL to connect to"), overridable via the `AMP_URL` env var — settings-schema and env-help strings in the binary (confidence: HIGH)
- Docs corroborate the CLI as the only documented programmatic surface: "check your balance in user settings or workspace settings, or by running `amp usage`" — https://ampcode.com/manual (confidence: HIGH)
- jackin❯ already calls exactly this endpoint with the same body — `crates/jackin-usage/src/usage/amp.rs:214-243` — and falls back to parsing `amp --no-color usage` output — `crates/jackin-usage/src/usage/amp.rs:339-343` (confidence: HIGH)

### 2. Credit balances — individual AND workspace/team

- Individual credits reach clients only as prose inside `displayText` (an "Individual credits: $N"-shaped line). The literal strings "Individual credits", "Amp Free", and "Signed in as" occur zero times in the CLI binary, so those labels are generated server-side; jackin❯'s parser targets them at `crates/jackin-usage/src/usage/amp.rs:345-372` (confidence: HIGH for server-side origin; MED that the current server text still uses those exact labels — label wording is unversioned server output)
- No separate workspace/team balance RPC exists in the CLI. The full
  internal-RPC method map contains no workspace-balance method; however,
  chapter 11's live current `amp usage` capture proves workspace balances
  can arrive as prose lines inside the existing
  `userDisplayBalanceInfo.displayText` (confidence: HIGH).
- Workspace credits exist as a product concept: "Workspace credits are pooled and shared by all workspace members. Workspace admins … purchase credits for the pool"; balance is checked "in user settings or workspace settings" (web pages) — https://ampcode.com/manual (confidence: HIGH)
- The only structured numeric credit fields in the whole CLI API schema are sandbox(orb)-credit gates: `reconcileSandboxUsage` → `{remainingCredits:number}` and `canConsumeSandboxCredits` (params `{minimumBalanceCredits?:number}`) → `{canConsume:boolean, remainingCredits:number, details?}` — RPC schema map in the binary. Whether `remainingCredits` reflects the individual or pooled workspace balance is not stated in the schema (confidence: HIGH for existence/field names)
- `getUserInfo` — the other candidate — is declared `[K.object({}), K.any()]` (untyped) in the CLI, so no balance/plan fields can be confirmed from the binary (confidence: HIGH that it is untyped client-side; response contents unknown)
- The speculative structured keys jackin❯ probes as fallbacks (`ampFreeRemaining`, `freeRemaining`, `remainingBalance`, `ampFreeLimit`, `hourlyReplenishment`, `individualCredits`, `individualBalance` — `crates/jackin-usage/src/usage/amp.rs:150-159`) have no counterpart in the CLI's declared schemas; the only real key is `displayText` (confidence: HIGH)
- Per-user spending limits ("entitlements", e.g. "$50/week for regular users") exist for Amp Enterprise Premium workspaces, configured in the workspace settings web page; no API is documented — https://ampcode.com/news/workspace-entitlements (confidence: HIGH for the feature; no API surface found)

### 3. Auth and credential storage (location/type only)

- Auth is a bearer token: every internal API request sends `Authorization: Bearer <apiKey>` where the key is resolved per server URL via `getToken("apiKey", url)` — request-builder strings in the binary (confidence: HIGH)
- Env var: `AMP_API_KEY` — "Access token for Amp (see https://ampcode.com/settings/security#access-token)" — env-help string in the binary; also documented at https://ampcode.com/manual (confidence: HIGH)
- On-disk file store: `<dataDir>/secrets.json` where `dataDir` defaults to `$XDG_DATA_HOME/amp` falling back to `~/.local/share/amp` (one bundled module additionally forces `~/.local/share` on darwin/win32 regardless of `XDG_DATA_HOME`). Keys are `"${kind}@${normalizedServerURL}"` with secret kinds `["apiKey","mcp-oauth-client-secret","mcp-oauth-token"]` — i.e. an `apiKey@https://ampcode.com/`-style JSON key — path/keying logic in the binary (constant `"secrets.json"`, `BvT`, key template `` `${r}@${l}` ``, kind list `D1T`) (confidence: HIGH)
- Alternate backend: OS-native keychain (getPassword/setPassword credential store) gated by the settings flag `experimental.cli.nativeSecretsStorage.enabled`; when enabled, secrets live in the OS credential store instead of `secrets.json` — `QvT` in the binary (confidence: HIGH)
- Non-secret settings live at `$XDG_CONFIG_HOME/amp/settings.json` (default `~/.config/amp/settings.json`), overridable via `AMP_SETTINGS_FILE` — binary path constants; corroborated by https://github.com/sourcegraph/amp-examples-and-guides/blob/main/guides/cli/README.md (confidence: HIGH)
- jackin❯ cross-reference: resolution order env `AMP_API_KEY` → `~/.local/share/amp/secrets.json` (scanning `apiKey@`-prefixed keys) → container handoff path — `crates/jackin-usage/src/usage/amp.rs:16-34`, `crates/jackin-usage/src/usage/amp.rs:245-266`, handoff constant at `crates/jackin-usage/src/usage.rs:184`. This matches the CLI's real storage format (confidence: HIGH)

### 4. Plan label source ("Amp Free" vs paid)

- There is no structured plan field in any CLI-visible API response; the only usage payload is `displayText:string`. Any "Amp Free" wording a client shows either comes from the server's prose or is client-side inference — RPC schema map + zero occurrences of "Amp Free" in the binary (confidence: HIGH)
- jackin❯ currently hardcodes the plan label client-side: `plan_label: … "Amp Free"` whenever any usage was fetched — `crates/jackin-usage/src/usage/amp.rs:99` (confidence: HIGH)
- That inference is now stale-risk: Amp launched paid subscriptions on 2026-07-18 — "Megawatt" ($20/month, "$20 included agent usage", "Low & medium modes only") and "Gigawatt" ($200/month, "$200 included agent usage", "All modes"), plus linked-ChatGPT-subscription usage — https://ampcode.com/news/subscriptions. How these plans appear in `displayText` is unverified (confidence: HIGH for the plans existing; LOW for any specific `displayText` representation)
- Free-tier status itself changed twice in 2026: new Amp Free signups paused ("Amp Free Is Full (For Now)", 2026-02-10, https://ampcode.com/news/amp-free-is-full-for-now) and ads removed ("Amp Free Is Ad-Free", 2026-03-30, https://ampcode.com/news/amp-free-is-ad-free) (confidence: HIGH)

### 5. Display elements with NO direct API source (client-side computation required)

- Percent left: current server text directly supplies the remaining
  percentage in its daily Amp Free line; the API still exposes it only
  inside prose, not as a structured JSON field. Clamp the parsed value and
  derive used geometry as `100 − remaining` — chapter 11 (confidence: HIGH).
- Reset: current server text supplies only `"resets daily"`, with no exact
  timestamp. Preserve that cadence verbatim; do not derive the retired
  hourly countdown or fabricate midnight — chapter 11 (confidence: HIGH).
- Used percentage: derived as `100 − remaining`; no dollar used/limit pair
  is present in the current daily line (confidence: HIGH).
- Plan label: client-side (see §4) (confidence: HIGH)
- Account identity: only available by parsing the server's "Signed in as …" prose line ("Signed in as" absent from the binary → server text) — jackin❯ parses it at `crates/jackin-usage/src/usage/amp.rs:348-350`; alternative structured source would be `getUserInfo`, which is `K.any()`-untyped (confidence: HIGH)
- In short: every numeric or semantic element (amounts, percent, reset, plan, identity) requires client-side parsing of one prose string; the API contributes structure only via the `{ok,result:{displayText},error}` envelope (confidence: HIGH)

## Dead ends and contradictions

- Official Amp CLI source repositories are not public: `github.com/sourcegraph/amp` and `github.com/ampcode/cli` both return HTTP 404 via the GitHub API (checked 2026-07-24). The npm-shipped binary is the closest primary artifact; findings above come from strings in that binary, not from a readable repo.
- npm package rename: `@sourcegraph/amp` is now a compatibility alias depending on `@ampcode/cli` — package README/manifest in https://registry.npmjs.org/@sourcegraph/amp and https://ampcode.com/news/npm-package-changes (2026-05-14). Tooling that pins `@sourcegraph/amp` still resolves, but the real bin is `@ampcode/cli`'s native executable (no longer a readable `dist/amp.js` bundle in the alias package).
- Task premise said the Amp Free window "resets daily" — now resolved by
  chapter 11's redacted live transcript and merged regression fixture.
  jackin❯'s replenishment-derived `"Resets in …"` path is obsolete for the
  current model. The public fixture does not prove an exact midnight
  timestamp, so only `"Resets daily"` is authorized.
- The "replenish" strings inside the binary are MCP-server restart logic, not billing — ruled out as quota evidence.
- No REST-style documented usage endpoint exists in https://ampcode.com/manual or the examples-and-guides repo; the manual points only at `amp usage` and the settings web pages.
- jackin❯'s structured-key fallbacks (`ampFreeRemaining`, `individualCredits`, …, `crates/jackin-usage/src/usage/amp.rs:150-159`) match nothing in the CLI's declared schemas — dead weight against the current API, kept alive only as defensive parsing.
- `threadDisplayCostInfo` (`{totalCostUSD, costBreakdown:{freeUSD,paidUSD}, costBreakdownURL, usedModelProviderKey, …}`) is per-thread cost, not quota — noted for completeness and out of scope for jackin❯ usage surfaces (limits-only rule).
- No embedded instructions were found in any fetched web content or the binary strings examined.

## Open unknowns

- Exact paid-subscription `displayText` — Megawatt/Gigawatt included usage,
  linked-ChatGPT/X allowances, and paid-only accounts — remains uncaptured.
  The current Amp Free daily and workspace-balance line shapes are resolved
  by chapter 11.
- Whether the ampcode.com web app's user/workspace settings pages fetch a structured workspace-balance JSON from separate (non-CLI) endpoints. Needs an operator-authenticated browser session inspecting network traffic on `ampcode.com/settings` and the workspace settings page.
- `getUserInfo` response contents (declared `K.any()` in the CLI) — may carry plan/workspace identifiers usable as a structured plan-label source. Needs authenticated capture.
- Whether `canConsumeSandboxCredits`/`reconcileSandboxUsage` `remainingCredits` reflects the individual balance, the pooled workspace balance, or a sandbox-specific bucket — schema is silent; needs authenticated observation.
- Any API surface for Enterprise Premium "entitlements" (per-user quota remaining/used) — nothing documented; needs authenticated workspace-admin session.
- Whether the paused-signup ("full") state or subscription plans changed the `auth-required`/error codes or added new RPC error variants relevant to usage polling — only `auth-required` is visible in the binary's usage path.
