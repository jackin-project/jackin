# 08 — Codex follow-ups (round 2)

Questions: (Q1) How does first-party openai/codex render `plan_type` values (`pro`, `prolite`) to user-facing plan names — is `prolite` "Pro 5x" and `pro` "Pro 20x", or plain "Pro"? Can chapter 02's "Pro 5x vs Pro 20x not distinguishable from `plan_type`" and jackin❯'s `pro`→"Pro 20x" / `prolite`→"Pro 5x" mapping both be true? (Q2) In `codex-rs/app-server-protocol/src/protocol/` (common.rs and v2/), what are the `account/*` JSON-RPC methods — does `account/read` (called by jackin❯) exist, what does `account/usage/read` (documented in chapter 02) return, and is jackin❯ missing a method or did chapter 02 misname one? (Q3) What is the default value of `cli_auth_credentials_store` and the semantics of `auto` — on a default macOS install, does the token live in `~/.codex/auth.json` or the OS keyring?
Informs: jackin-desktop
Method: web + reference read of github.com/openai/codex
Vetted: 2026-07-24

Reference snapshot: `github.com/openai/codex`, branch `main`, commit `7bafdada8beaad9325ed69218f743f058e3598ab` (shallow clone, 2026-07-24). All `codex-rs/...` paths below refer to that commit. jackin❯ cross-refs are read-only against the working tree at `main` (HEAD `3e6376d`). No CodexBar/OpenUsage source was read (clean-room). No embedded instructions were detected in any fetched source.

## Findings

### Q1 — Plan multiplier labels (`pro` / `prolite` display mapping)

- First-party canonical display mapping lives in `KnownPlan::display_name()`: `Pro` → `"Pro"`, `ProLite` → `"Pro Lite"` (also `Free`→"Free", `Go`→"Go", `Plus`→"Plus", `Team`→"Team", `Business`→"Business", `Enterprise`→"Enterprise", `Edu`→"Edu"). Raw-value parsing: `"pro"` → `KnownPlan::Pro`, `"prolite"` → `KnownPlan::ProLite` — `codex-rs/protocol/src/auth.rs` lines 66–84 (`PlanType::from_raw_value`) and 107–121 (`display_name`), https://github.com/openai/codex/blob/7bafdada8beaad9325ed69218f743f058e3598ab/codex-rs/protocol/src/auth.rs (confidence: HIGH)
- The TUI status card uses `plan_type_display_name()`: `ProLite` → `"Pro Lite"`; team-like plans remapped to `"Business"`, business-like to `"Enterprise"`; everything else (incl. `Pro`) title-cased from the debug name → `"Pro"`. Its own test table asserts `(Pro, "Pro")` and `(ProLite, "Pro Lite")` — `codex-rs/tui/src/status/helpers.rs` lines 99–109 (function) and 214–233 (test), consumed at `codex-rs/tui/src/app_server_session.rs` lines 395 and 1313, rendered as `email (Plan)` in `codex-rs/tui/src/status/card.rs` lines 724–727 (confidence: HIGH)
- The strings `"5x"` and `"20x"` appear nowhere in the openai/codex repo (repo-wide grep over `.rs`/`.ts`/`.tsx`; only unrelated hits: "1.5x speed" agent description, a base64 test fixture). First-party Codex surfaces never render a multiplier label for any plan (confidence: HIGH)
- The login ID-token path also uses `display_name()` (`IdTokenInfo::get_chatgpt_plan_type`, `codex-rs/login/src/token_data.rs` lines 45–51), i.e. would produce `"Pro Lite"`, but has no non-test consumers at this commit (confidence: HIGH)
- Usage-limit-reached messaging treats `Pro` and `ProLite` as one arm (both told to purchase credits at `chatgpt.com/codex/settings/usage`); `Plus` is upsold "Upgrade to Pro" with no multiplier wording — `codex-rs/protocol/src/error.rs` lines 682–717 (confidence: HIGH)
- OpenAI's own Codex pricing page (fetched live 2026-07-24; `https://developers.openai.com/codex/pricing` 308-redirects to `https://learn.chatgpt.com/docs/pricing`) markets the Pro tier as "Choose 5x or 20x higher rate limits than Plus. From $100/month" and lists tiers "Pro 5x" and "Pro 20x"; the name "Pro Lite" does not appear on that page (confidence: HIGH)
- OpenAI Help Center "About ChatGPT Pro tiers" (https://help.openai.com/en/articles/9793128-about-chatgpt-pro-tiers) reportedly states Pro $100 = 5x higher usage than Plus, Pro $200 = 20x; direct fetch returned HTTP 403, so this is search-snippet-mediated (confidence: MED)
- Pre-launch press (Feb 2026) found the $100 tier in ChatGPT checkout code under identifiers `PROLITE` / `chatgptprolite` — https://winbuzzer.com/2026/02/24/openai-chatgpt-pro-lite-100-dollar-plan-found-checkout-code-xcxwbn/ and https://beebom.com/openai-chatgpt-pro-lite-plan-details-leaked/ (secondary sources) (confidence: MED)
- **Settlement.** The two chapter claims cannot both be true as written. `plan_type` DOES distinguish the two Pro tiers: `pro` and `prolite` are distinct enum values in both the backend OpenAPI model (`codex-rs/codex-backend-openapi-models/src/models/rate_limit_status_payload.rs` lines 109–110) and `KnownPlan`. Chapter 02's line "Pro 5x vs Pro 20x is not distinguishable from `plan_type` alone" is over-strong; the defensible nucleus is only that the enum carries no multiplier *label*, so rendering "5x"/"20x" requires an out-of-band mapping. jackin❯'s mapping (`crates/jackin-usage/src/usage/codex.rs` lines 70–76: `pro`→"Pro 20x", `prolite`/`pro_lite`/…→"Pro 5x") matches OpenAI *marketing* names (pricing page tiers + $100↔PROLITE checkout identifiers + help-center multipliers) but diverges from *first-party codex display strings*, which are "Pro" and "Pro Lite". The `prolite`↔"Pro 5x" and `pro`↔"Pro 20x" identification is an inference chained from marketing + leak evidence, never stated in any single first-party artifact (confidence: HIGH for "first-party renders Pro / Pro Lite"; MED for the tier↔multiplier identification)

### Q2 — App-server `account/*` RPC methods

- Full `account/*` surface at this commit, all in `codex-rs/app-server-protocol/src/protocol/common.rs` (https://github.com/openai/codex/blob/7bafdada8beaad9325ed69218f743f058e3598ab/codex-rs/app-server-protocol/src/protocol/common.rs):
  - Client→server requests: `account/login/start` (line 1034), `account/login/cancel` (1041), `account/logout` (1047), `account/rateLimits/read` = `GetAccountRateLimits` (1053), `account/rateLimitResetCredit/consume` (1059), `account/usage/read` = `GetAccountTokenUsage` (1065), `account/workspaceMessages/read` (1071), `account/sendAddCreditsNudgeEmail` (1077), **`account/read` = `GetAccount` (1187)**.
  - Server→client request: `account/chatgptAuthTokens/refresh` (1536).
  - Server→client notifications: `account/updated` (1705), `account/rateLimits/updated` (1706), `account/login/completed` (1749–1751).
  - Deprecated v1: `getAuthStatus`, annotated "DEPRECATED in favor of GetAccount" (1204–1208). (confidence: HIGH)
- `account/read` exists. Params `GetAccountParams { refreshToken: bool }`; response `GetAccountResponse { account: Option<Account>, requiresOpenaiAuth: bool }`, where `Account` is a `type`-tagged enum with wire tags `"apiKey"` (empty), `"chatgpt" { email, planType }`, `"amazonBedrock" { usesCodexManagedCredentials }` — `codex-rs/app-server-protocol/src/protocol/v2/account.rs` lines 20–38 (`Account`), 484–500 (`GetAccountParams`/`GetAccountResponse`) (confidence: HIGH)
- `account/usage/read` returns **historical/aggregate token statistics, not limit windows**: `GetAccountTokenUsageResponse { summary: AccountTokenUsageSummary { lifetimeTokens, peakDailyTokens, longestRunningTurnSec, currentStreakDays, longestStreakDays }, dailyUsageBuckets: Option<Vec<{ startDate, tokens }>> }` — a per-day token time series plus lifetime/streak gamification stats — `codex-rs/app-server-protocol/src/protocol/v2/account.rs` lines 392–395 and 435–449 (confidence: HIGH)
- **Settlement.** Neither a missing method nor a misname. jackin❯ calls `account/rateLimits/read` + `account/read` (`crates/jackin-usage/src/usage/codex.rs` lines 767–785) — both exist. Chapter 02 documented `account/rateLimits/read` + `account/usage/read` (chapter line 20); `account/usage/read` is real but is a *different* method (historical token stats), and chapter 02 simply omitted `account/read` from its app-server enumeration. Note the fit with jackin❯'s limits-only rule: `account/usage/read` is precisely the historical-trend payload jackin❯'s usage surfaces exclude, and jackin❯ does not call it (confidence: HIGH)
- Cross-ref divergence observed while verifying: jackin❯'s `account/read` decoder `CodexRpcAccountDetails` (`crates/jackin-usage/src/usage/codex.rs` lines 428–438) accepts tags `"apikey"` and `"chatgpt"` only, while upstream serializes `"apiKey"` (camelCase) and also emits `"amazonBedrock"`. Serde external-tag matching is exact, so `account/read` decode fails for API-key and Bedrock accounts; and although the comment at lines 775–776 says the account label is non-essential, the decode error at lines 786–792 propagates via `?` and fails the whole RPC usage result. Finding only, recorded for the jackin-desktop consumer (confidence: HIGH for the tag mismatch; HIGH for the propagation path as written)

### Q3 — Credential store default and `auto` semantics

- Config surface: `cli_auth_credentials_store: Option<AuthCredentialsStoreMode>` — `codex-rs/config/src/config_toml.rs` line 254 (confidence: HIGH)
- Enum and default — `codex-rs/config/src/types.rs` lines 104–117 (https://github.com/openai/codex/blob/7bafdada8beaad9325ed69218f743f058e3598ab/codex-rs/config/src/types.rs):

  ```rust
  pub enum AuthCredentialsStoreMode {
      #[default]
      /// Persist credentials in CODEX_HOME/auth.json.
      File,
      /// Persist credentials in the keyring. Fail if unavailable.
      Keyring,
      /// Use keyring when available; otherwise, fall back to a file in CODEX_HOME.
      Auto,
      /// Store credentials in memory only for the current process.
      Ephemeral,
  }
  ```

  Default = `File`. Note a fourth variant, `ephemeral`, beyond chapter 02's `file | keyring | auto` (confidence: HIGH)
- Resolution path: `cfg.cli_auth_credentials_store.unwrap_or_default()` feeds `resolve_cli_auth_credentials_store_mode`, which additionally forces `Keyring`/`Auto` → `File` on local dev builds (package version `"0.0.0"`); `Ephemeral` passes through — `codex-rs/core/src/config/mod.rs` lines 287–298 and 3994–3996; default asserted `File` in `codex-rs/core/src/config/config_tests.rs` lines 5540–5547; TUI bootstrap likewise `.unwrap_or_default()` at `codex-rs/tui/src/lib.rs` lines 1041–1044 (confidence: HIGH)
- `auto` semantics (implementation): `AutoAuthStorage` wraps a keyring backend plus `FileAuthStorage`. `load`: try keyring; `Ok(None)` or `Err` → fall back to file. `save`: try keyring; `Err` → fall back to file. `delete`: via keyring storage (which also removes the disk copies) — `codex-rs/login/src/auth/storage.rs` lines 404–453; backend selection `create_auth_storage_with_store` maps `File`→`FileAuthStorage`, `Keyring`→keyring (Direct on non-Windows, Secrets on Windows per `AuthKeyringBackendKind::default()`, `codex-rs/config/src/types.rs` lines 147–154), `Auto`→`AutoAuthStorage`, `Ephemeral`→in-memory map — storage.rs lines 500–527 (confidence: HIGH)
- File location: `get_auth_file` = `codex_home.join("auth.json")` (`codex-rs/login/src/auth/storage.rs` lines 150–151); `CODEX_HOME` defaults to `~/.codex` when the env var is unset (`codex-rs/utils/home-dir/src/lib.rs` lines 52–61) (confidence: HIGH)
- **Answer.** On a default macOS install (no `cli_auth_credentials_store` set), the ChatGPT OAuth token lives in **`~/.codex/auth.json` (file)**, not the OS keyring. The keyring is used only when the user opts in via `keyring` (hard requirement) or `auto` (keyring-first with file fallback) (confidence: HIGH)
- Official config reference (fetched live 2026-07-24; `https://developers.openai.com/codex/config-reference` 308-redirects to `https://learn.chatgpt.com/docs/config-file/config-reference`) lists allowed values `file | keyring | auto` for the credentials-store option and documents **no default**; `ephemeral` is absent from the docs — code is ahead of docs (confidence: MED — page fetched, but content summarized by the fetch tool)

## Dead ends and contradictions

- No first-party multiplier labels: repo-wide search for `"5x"` / `"20x"` in openai/codex found nothing plan-related — a first-party in-code `prolite`→"Pro 5x" mapping does not exist; the identification rests on marketing pages plus checkout-code identifiers (press). Chapter 02 line 21 ("not distinguishable from `plan_type` alone") is contradicted by the enum itself and should be read as "no multiplier label in the enum".
- `help.openai.com` blocks direct fetch (HTTP 403, Cloudflare) — the Pro-tier multiplier statement from article 9793128 could only be captured via search snippet (hence MED, not HIGH).
- `IdTokenInfo::get_chatgpt_plan_type` (the "Pro Lite" string from the ID token) has no non-test consumers at this commit — a dead end for finding an onboarding-screen plan label.
- Chapter 02's app-server citations (common.rs lines 1053–1070, 1706) still match this commit exactly; no drift in those line refs.
- Docs-vs-code contradiction (minor): the official config reference omits both the default value and the `ephemeral` variant that exist in source.

## Open unknowns

- No single first-party artifact ties wire value `prolite` to the marketing label "Pro 5x" (or `pro` to "Pro 20x"); the mapping is a two-hop inference (pricing page multipliers/prices + $100↔`PROLITE` checkout identifiers). OpenAI could relabel either tier without changing wire values.
- Whether every $200 Pro subscriber's backend `plan_type` is exactly `pro` (vs. any legacy or regional value) is not observable from the codex source alone; no live account probing was done.
- Which backend HTTP endpoint `account/usage/read` proxies to, and its availability per plan (`summary` fields and `dailyUsageBuckets` are all optional/nullable), was not traced in this round.
- Whether OpenAI plans to flip the CLI credential-store default from `file` to `keyring`/`auto` in a future release (the docs' silence on a default leaves room) — no primary statement found.
