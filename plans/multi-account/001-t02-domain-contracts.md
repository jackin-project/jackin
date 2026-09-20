# T02 frozen domain contracts (S1–S3 spine + lane interfaces)

Status: FROZEN 2026-09-17, amended 2026-09-21 by orchestrator. Lanes code against this. Changes need an
explicit orchestrator decision appended under "Amendments" with date + reason.

Source docs: `jackin-provider-research.md`, `jackin-accounts-usage-specification.md`,
`jackin-implementation-and-verification.md` (repo root, branch `feat/multi-account-support`).

## 1. Catalogs (S1, owner: s1-catalog-expand)

### Agents (12)

`claude, codex, amp, kimi, opencode, grok` (existing) plus:

| Variant | Slug | Label | Binary (container PATH) | Credential home (host-relative) |
|---|---|---|---|---|
| Antigravity | antigravity | Antigravity | `agy` | `.gemini/antigravity-cli` (+ macOS keychain `gemini`/`antigravity` singleton) |
| Gemini | gemini | Gemini | `gemini` | `.gemini` (`GEMINI_CLI_HOME` = parent of `.gemini`) |
| Cursor | cursor | Cursor | `cursor-agent`, `agent` (probe absolute paths; bare `agent` collides with Grok) | `.cursor` (+ keychain `cursor-access-token` lineage) |
| Muse | muse | Muse | `muse` | `.config/muse` (`auth.json` schema v2; secret in keychain `ai.meta.dev.credentials`) |
| Omp | omp | omp | `omp` | `.omp` (SQLite `agent/agent.db`) |
| Hermes | hermes | Hermes | `hermes --tui` | `.hermes` (`HERMES_HOME`, `-p` profiles) |

Z.AI, OpenRouter, MiniMax stay providers (no TUI). Muse fallback installer fails
closed (no verified standalone installer URL); Linux layout verified locally later.

### Providers (12)

`anthropic, openai, amp, xai, opencode, moonshot, zai, minimax` (existing) plus
`google` (Antigravity + Gemini billing), `cursor`, `meta` (Muse subscription),
`openrouter`.

`AiProvider::for_agent` returns `Option<AiProvider>`: `None` for Omp/Hermes (pure
multi-provider clients, no native billing). Antigravity/Gemini → Google,
Cursor → Cursor, Muse → Meta.

### Compatibility rules

- `Profile`: `owner == agent` AND (`Some(provider) == for_agent(agent)` OR agent in
  `{Opencode, Omp, Hermes}` holding a multi-provider store entry whose provider is
  a catalogued entry of that store). OpenCode native `Opencode` (Zen/Go) unchanged.
- `ApiKey`: existing Claude/Codex/Opencode matrix unchanged; new agents accept their
  native provider; Omp/Hermes accept any catalogued provider entry they can route
  (positive capability entries per client catalog, see ledger).
- `OAuthToken`: Claude-only (unchanged) until a verified second flow exists.
- Env discovery adds: `CURSOR_API_KEY`, `OPENROUTER_API_KEY`, `GEMINI_API_KEY`
  (+ `GOOGLE_API_KEY` alias only if docs support it — S1 decides, documents).
- OpenCode auth primary: `.local/share/opencode/auth.json` (XDG data); S1 keeps the
  old path as fallback iff it differs, noted in the lane report.

## 2. Config schema (S2, owner: orchestrator)

```toml
[bootstrap]
version = 1            # written on init completion; absent in pre-existing configs
fresh_install = false  # true only when an installer pre-created an empty config

[[agent_configurations]]
id = "claude-work"     # lowercase slug, validated like account IDs
agent = "claude"
account = "anthropic-work"
model = "opus-4-6"     # optional override; empty = account/client default
base_url = "https://..."  # optional override; empty = account/client default
# label auto-derived "Claude · Work" unless display_label set:
display_label = "Claude · Work"  # optional

default_launch = ["claude-work", "claude-personal", "codex-work"]  # global + per workspace + per role
```

- `AccountConfig`: keeps `enabled/name/provider/credential`. `ApiKey` keeps
  `value/base_url/model` as the **account-level defaults**; configuration-level
  `model`/`base_url` override them; client native defaults apply last. Single
  precedence chain, no parallel paths.
- `Profile` gains optional `xdg_roots = { data, config, cache }` (absolute dirs,
  Amp and any XDG-split client; validated: Amp-only unless a client declares XDG).
- `account_bindings` (`BTreeMap<Agent, String>`) kept at all three scopes as the
  preferred per-agent default. `default_launch` (`Vec<String>` of configuration
  IDs) added at global, workspace, and role scopes. Explicit scope **replaces**
  inherited scope (no union). Explicit empty `default_launch = []` means shell-only
  when the launch mode is an explicit shell mode, else a no-eligible-account error.
- Bootstrap state machine: no config file → init workflow (scan + seed + sentinel);
  config + `[bootstrap] fresh_install = true` → init workflow once; config without
  sentinel (pre-existing) → treated as initialized, sentinel backfilled on next
  editor save **without** scanning (A04/A21). Init completes only after the atomic
  transaction; interruption retries idempotently (A03). Concurrent scans dedup by
  source identity under the write lock (A22).
- Migration: in-place schema evolution, no dual runtime resolvers. All existing
  accounts/bindings preserved byte-for-byte in meaning.

## 3. Resolution (S2, owner: orchestrator)

`resolve_launch(config, workspace, role, one_launch: Option<&[String]>) ->
Result<Vec<ResolvedInstance>, ConfigError>` where `ResolvedInstance` is
`{ config_id, agent, account_id, model: Option<String>, base_url: Option<String> }`.

Precedence: one-launch selection → role `default_launch` → workspace
`default_launch` → global `default_launch` → sole eligible configuration / picker.
Inherited candidates filtered by workspace `accounts` authorization; explicit
selections validated atomically against authorization + compatibility (C06/C07/C16:
no silent substitution, no ambient fallback). New tabs validate against the
container manifest, not current defaults (C11/C13). `resolve_account` (single) is
kept for fast paths and fixed callers; the Console committed-agent path must use
the resolver so valid defaults suppress the picker (C04).

## 4. Credential transport (S3, owner: orchestrator)

`AgentCredentialEnv` is rekeyed from agent slug to **configuration/instance ID**
(unique per registry; synthesized default IDs are `{account-id}@{agent-slug}`).
Envelope gains `schema_version = 2`; Capsule rejects version mismatch with an
explicit restart/upgrade error (H03), never silent misread. Staged file layout,
0600/0700 permissions, and RO mount stay; per-instance maps carry only that
instance's vars (G5 collision resolved by namespacing, not sharing).

## 5. Provisioning + manifest (T06/T07 lanes, after S3)

- `ProvisionedAuth`: `slots: BTreeMap<String /*instance key*/, ProvisionedInstanceAuth>`
  with `ProvisionedInstanceAuth { agent, account_id, mode, home_dir, credential_paths,
  forward_auth }`. The six fixed `Option` slots and single-agent `AgentRuntimeState`
  are replaced (complete migration, stale all-agents comments fixed).
- Launch manifest (non-secret): `launch-manifest.json` + `launch-manifest.sha256`
  per container: revision, ordered instance bindings, account IDs, credential
  revisions, image/version provenance, capability allowlist. Fingerprint covers
  **exactly the admitted set** (C17: unrelated account D edits never invalidate
  A/B/C). Restore/reconnect/hardline validate against the manifest. Account-set
  change on a live container = explicit validated revision or new container (C14).
- Session identity: `Session`/spawn/split/restore/tab/history carry the instance
  binding (`config_id` + account + model); labels derive from saved identity
  (`Claude · Work`), never terminal titles. Relay capabilities allow only admitted
  accounts, several per provider (E05/E07).

## 6. Usage (T10/T11 lanes)

- `UsageProjectionV1` gains per-account `metric_groups: Vec<UsageMetricGroupV1>`:
  typed groups (window / balance / spend_cap / token_totals / rate_limit / plan),
  each with own scope, observed/fetched/last-success timestamps, freshness, and
  errors. Existing `windows` stay as the principal-window projection. No invented
  tokens-from-dollars; over-100% raw values kept, bar geometry clamped; reset time
  ≠ credential expiry ≠ renewal (F01–F10).
- Broker owns all provider I/O, single-flight generations, Retry-After/shared
  cooldowns, per-account incremental publication, credential-revision invalidation.
  Console subscribes + heartbeat; no render-thread provider calls, no missed-poll
  bursts after sleep (G01–G11, G17).
- Shared quota dedup by (service + billing subject + scope/model/key); independent
  key caps never merged (F10, F27/F28 ownership rules for local histories).

## 7. Ownership + refresh (T08 lane)

Per provider: native/external vs Jackin-managed, exactly one writer per credential
lineage (grant/lineage, not path/label), identity/revision guards, atomic
publication. No blind refresh-token clone-and-copy-back. Custom-profile failure
never borrows ambient/native/another-client credentials (D15/D21/D22).

## Amendments

### 2026-09-21: lossless account usage across consumers

The active accounts objective requires fractional percentages (including the
illustrative 99.98% Antigravity fixture), absolute reported quantities, dynamic
shared-pool model membership, billing scope, and manual-reset entitlements. The
current `UsageMetricValueV1::Window` integer percentage fields and
`UsageMetricScopeV1` single-model field cannot represent that contract. The
coordinator authorizes evolving the existing canonical protocol and projection
to retain those facts, with explicit schema/protocol mismatch handling and
corresponding FFI consumers and fixtures. This is required implementation work,
not a completed capability claim.

Console and Capsule must consume the same account-based canonical presentation.
Remove lossy local account DTOs and provider-only selection where they discard
separate registrations or supported details. Keep provider I/O, credential
resolution, and quota interpretation in the existing host broker/adapters.
Preserve shared pools as one pool with its actual model membership; never derive
an allowance from model availability, synthesize absolute quantities from
percentages, or treat refresh/reset-time display as entitlement redemption.
