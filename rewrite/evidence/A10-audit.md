# Audit: tuples 13–22 + finishing 1–12

Read-only. Branch `feature/openshell-native`; Phase A (A0–A9) is fully checked in `rewrite/PLAN.md:697-707`. Current states: T01 `Supported` (agent.rs:766), T09 `Supported` (agent.rs:846), T02/T05 `BlockedVendor`, T03/T04/T06/T07/T08/T10/T11/T12 `Unavailable` (agent.rs:760-881) with blocker files in `rewrite/blockers/T02..T12*.md`.

## 0. Hard prerequisite: design amendment (orchestrator-only)

- `AGENTS.md`: "The agents are the twelve tuples of decision D29 … Do not add code, files, or configuration for an agent outside that catalog." `rewrite/DESIGN.md:454-475` (D29) fixes the catalog at twelve. Adding T13–T22 requires a new `decisions.tsv` row + D29 edit + `GOAL`/`PLAN` extension first (A0 pattern, gate: reviewer finds no conflict with D01/D03/D05/D10/D24/D27).
- **Billing-semantics blocker for T21/T22**: D29 (`DESIGN.md:475`) requires "`supported` only with live gateway evidence that the subscription paid and that no paid fallback can occur". T21 (omp+Anthropic API key) and T22 (omp+OpenAI API key) are pay-as-you-go API keys by the operator's own wording. Either the design is amended to admit explicitly API-billed tuples, or T21/T22 can never leave `Unavailable`. This must be decided in U0, not in code.

## 1. Catalog collision analysis (the core representation problem)

`lookup_tuple(agent, provider)` (`crates/jackin-core/src/agent.rs:251-256`) keys on the pair, so T13–T22 are new unique keys — **no literal collision**. (ClaudeCode,Kimi), (Codex,Kimi), (Omp,Claude), (Omp,Codex) don't exist today. The collisions are in the *one-provider-per-agent* model:

| Surface | Location | Problem for 13–22 |
|---|---|---|
| `Agent::provider()` | agent.rs:63-75 (ClaudeCode→`Provider::Claude` :66, Codex→`Provider::Codex` :65, Omp→`Provider::Zai` :73) | Returns one provider for an agent that now has 5 (Claude Code), 5 (Codex), 6 (omp) |
| `AgentSpec` provider-scoped fields | agent.rs:282,288-290 (`provider`, `auth_class`, `provider_hosts`, `credential_env_names`) | Per-agent static but the data is per-tuple. `OMP_SPEC` hosts are only `["api.z.ai"]` (agent.rs:738); `CLAUDE_CODE_SPEC` hosts only `api.anthropic.com` (:557) with empty env list (:558) |
| `omp_provider_id`/`omp_credential_env`/`omp_base_url`/`omp_provider_hosts` | agent.rs:134-171 | String-matched on `"T09".."T12"` only — the existing workaround for one-agent-many-providers; omp-only, partial, and the pattern T13–T22 must generalize |
| `tuple_profile_asset` catch-all | agent.rs:244-247 | `_ =>` silently returns the **t12 omp-mimo asset** for any unknown id. New tuples added without a match arm get the wrong OpenShell profile with no error. Remove the catch-all, match exhaustively |
| `Provider` enum | `crates/jackin-core/src/provider.rs:8-24` | `Provider::Claude` *is* Anthropic and `Provider::Codex` *is* OpenAI; no separate variants needed for T21/T22 — `(Agent::Omp, Provider::Claude)` / `(Agent::Omp, Provider::Codex)` work as keys. `login_file` (:74-92) is provider-keyed: Claude→`.credentials.json`, Kimi→`credentials/kimi-code.json`; audit its consumers before T13/T21 (omp+Claude account would claim a Claude-style login file) |

**Recommended representation**: move per-tuple data into `AgentTuple` (or a `TupleSpec`): profile asset, omp provider id, credential env, base URL, hosts, env-var name for auth placeholder. Delete the four `omp_*` string accessors and the `tuple_profile_asset` catch-all. `AgentSpec` keeps only per-agent install/paths/launch data. This is one breaking unit; all consumers below then read tuple data uniformly.

## 2. Per-tuple change surfaces (grouped)

### T13–T16 Claude Code + Kimi/Z.AI/MiniMax/MiMo (class S, Anthropic-compatible)

- **Catalog**: TUPLES rows (agent.rs:760-881); states start `Unavailable("not yet proven on a real gateway")`. Models per pair from vendor docs (research `rewrite/research/agent-provider.md:190-196`: glm-5.3, kimi-for-coding, mimo-v2.6-pro candidates; MiniMax model unknown).
- **Assets**: 4 new profiles `crates/jackin-core/src/assets/t13-claude-kimi.yaml` … `t16-claude-mimo.yaml`, `binaries: [/usr/local/bin/omp→/usr/local/bin/claude]`, one credential each. Endpoints (host:443 only — OpenShell profiles carry no path): Z.AI `api.z.ai` (base `https://api.z.ai/api/anthropic`, research :1150); Kimi `api.kimi.com` (Anthropic-compatible endpoint documented, exact path to verify, research :1151); MiMo token-plan host `token-plan-sgp.xiaomimimo.com` (CN variant `token-plan-cn…` documented, research :1153 — region must be pinned); MiniMax **endpoint unknown, needs research** (research :1152). Base-URL path lives in the sandbox config, not the profile.
- **Policy hosts**: `crates/jackin-openshell/src/policy.rs:231-262` (`close_for_tuple`, `close_for_tuples`), `:273-278` (`close_for_instances`) — all read the profile YAML; no code change beyond the asset. D10 host closure enforced at `policy.rs:233`.
- **Sandbox config rendering**: `crates/jackin-session/src/files.rs` — new `render_claude_config(tuple)`: `settings.json` (`CLAUDE_CODE_SPEC.user_config` = `/sandbox/.claude/settings.json`, agent.rs:540) needs an `env` block with `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` (Z.AI/MiMo documented token var, research :1150,:1153). Today `agent_config_file` (`crates/jackin-session/src/plan.rs:511-556`) has **no ClaudeCode branch** — Claude falls into the codex default (:549-555) and would write Codex TOML into `settings.json`. Auth placeholder: `placeholder_file`/`render_placeholder_auth` (files.rs:51-61,104-110) renders from `spec.credential_env_names`, which is empty for Claude — must read per-tuple env names. `.credentials.json` shape: OAuth schema only, unproven (research :576-578) — for key tuples the settings env path is the mechanism, `[INFERENCE]` on exact settings schema for 2.1.311/2.1.317.
- **Account flow**: `crates/jackin/src/commands/account.rs:149-158` `default_agent`: `Provider::Kimi→KimiCode`, `_→Agent::Omp` — a Kimi account adding Claude Code needs `--agent claude` (accepted at :162-167); make multi-tuple providers require `--agent` with a typed error. `account_agent` (`crates/jackin-session/src/accounts.rs:106-116`) infers agent from provider and sends `Provider::Claude→Agent::Codex` (:114) — must take the requested agent. Static-key sync path `accounts.rs:900-948` already reuses the omp tuple's provider id for native agents (:913-917), so Kimi/Z.AI/Minimax/MiMo keys flow once per-tuple data is unified; `has_provider` (`crates/jackin-core/src/config.rs:528-536`) already admits Zai/Kimi/Minimax/Mimo static keys.
- **Console offers**: automatic once TUPLES rows exist (`crates/jackin-tui/src/project.rs:756-798`); filtered by role manifest `[agents.claude]` (:761) — roles need a Claude instruction file (A8 pattern: sentinel 55d2c14). Manifest validation `crates/jackin-core/src/manifest.rs:190-196` then passes once any Claude tuple is `Supported`.
- **Usage windows**: none — usage is account/provider-scoped: `crates/usage/jackin-usage/src/dispatch.rs:48-64` (Kimi/Minimax/Zai implemented; MiMo unsupported :59-63, shows Unavailable sentinel via `crates/jackin-tui/src/usage.rs:166-190`).

### T17–T20 Codex + Kimi/Z.AI/MiniMax/MiMo (class S, OpenAI-compatible custom provider)

- **Catalog/assets**: 4 rows + `t17-codex-kimi.yaml`…`t20-codex-mimo.yaml`, `binaries: [/usr/local/bin/codex]`. Endpoints: Kimi Responses documented with `model_provider` + `KIMI_API_KEY` (research :1156); Z.AI Responses `https://api.z.ai/api/v1` (research :1155); MiMo OpenAI-compatible documented (research :39); MiniMax **unknown, needs research** (:200). T01 regression risk: (Codex, Codex) must keep ChatGPT behavior.
- **Sandbox config rendering**: `render_codex_config` (`files.rs:13-30`) writes only `model` + `projects` trust. Must add `model_providers.<id>` (`name`, `base_url`, `env_key`, `wire_api`) and `model_provider`/`model` from tuple data. `env_key` **must equal the profile `credentials.env_vars` name** — OpenShell v0.1.7 accepts only profile env-var names as stored keys (T03 defect, `rewrite/blockers/T03-grok.md`, fix 7c30d78c4). `agent_config_file` dispatch (plan.rs:511-556) must branch on tuple/provider, not agent alone.
- **Auth placeholder**: Codex T17-20 use the env placeholder (`CODEX_SPEC.credential_env_names` agent.rs:476-481 is ChatGPT-shaped; per-tuple env needed). Codex auth.json write path `write_codex_auth` (`launch/run.rs:614-648`) is ChatGPT-only and keyed on `has_codex` (any Codex instance, run.rs:530-537) — must apply only to (Codex, Codex).
- **Account flow/console/usage**: same surfaces as T13-16 (`default_agent` :155 `Provider::Codex→Codex` collides with T22; `has_provider` config.rs:532-535 excludes `Provider::Codex` static keys — T17-20 don't need it (Kimi et al. already allowed) but T22 does, see below).

### T21/T22 omp + Anthropic / OpenAI (API key; blocked on §0 decision)

- **Catalog/assets**: 2 rows; `t21-omp-anthropic.yaml` (host `api.anthropic.com`), `t22-omp-openai.yaml` (hosts `api.openai.com`; T01's t01-codex.yaml covers the same hosts for the codex binary — profile `binaries` scoping keeps them distinct). Profiles: env_vars `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`, no refresh.
- **Sandbox config rendering**: `render_omp_config`/`omp_auth_file` (`files.rs:63-102`) read the omp accessors — after §1's refactor they get provider ids `anthropic`/`openai`, envs above, and **no baseUrl** (native endpoints). Multi-instance per-home auth writes already exist for omp (`launch/run.rs:541-562`).
- **Account flow**: `has_provider` (config.rs:528-536) must admit `Provider::Claude`/`Provider::Codex` accounts with non-`CodexLogin` credentials or sync returns `HostOnly` (:900-903) and no gateway provider is created. `CodexLogin` stays Codex-only (config.rs:488-491). `omp-host` store read (`commands/account.rs:71-100`) keys `~/.omp/agent/agent.db` by `omp_provider_id` — anthropic/openai ids must exist there `[INFERENCE]` that the host omp store holds such records. `default_agent` (:155-156) maps Codex→Codex and Claude→Omp — ambiguous with T22/T02; require `--agent omp`.
- **Usage**: T22 rides `codex_usage` (keyed accounts tested, `crates/usage/jackin-usage/tests/usage/apikey.rs:26-35`); T21 uses `claude_usage` which is OAuth-shaped — likely `Unavailable` for a keyed account; acceptable (sentinel path), verify in unit.

## 3. Shared surfaces to touch once

- `crates/jackin-core/src/config/edit.rs:222,229` — role bindings keyed by `agent.provider()`; binding for (ClaudeCode,Kimi) would store under `Provider::Claude`. Pass provider explicitly or key by (agent, provider). `check_binding` itself (config.rs:669-687) is provider-keyed and fine.
- `crates/jackin/src/commands/load.rs:126-128,146-148` — provider default `agent.provider()` when `--provider` absent; with multi-provider agents require the flag or default to the agent's only `Supported` tuple.
- `crates/jackin/src/commands/account.rs:247-258` — role-bind validation picks `account.provider().tuples().first()`; order-dependent once a provider has 3+ tuples (Kimi: T08, T10, T13, T17). Same first-tuple pattern in `crates/jackin-tui/src/project.rs:682-688` (bindings display) and `crates/jackin-testkit/src/world.rs:158-162`.
- `crates/jackin-core/tests/agent_table.rs:71-78` — asserts `spec().provider == agent.provider()`; `crates/jackin-core/tests/catalog.rs:18-21` asserts (ClaudeCode,Zai) is a typed error — both must be redesigned with the catalog.
- Image layer needs **no change**: installs and cache key are per-agent (`crates/jackin-image/src/key.rs:33-41,82-93,226-241`, `plan.rs:311-335`); binaries claude/codex/omp already in the catalog.
- Per-instance (non-lead) config copies: only omp auth + muse trust are written into per-agent homes today (`launch/run.rs:541-581`); Claude/Codex non-lead instances need their config+auth in `/sandbox/.jackin/homes/<agent>/` (`files.rs` pattern `write_home_auth` run.rs:588-612).

## 4. Design blockers (summary)

1. D29 twelve-tuple cap + AGENTS.md catalog rule → orchestrator amendment required before any code (§0).
2. T21/T22 API-key billing contradicts D29's subscription-paid support bar (§0).
3. `Agent::provider()` single-provider assumption leaks into bindings, CLI defaults, `account_agent`, `default_agent` (§1, §3) — provider can no longer be a function of agent anywhere.
4. `omp_*` string-keyed accessors cover only T09-T12; `tuple_profile_asset` `_` catch-all silently mis-assigns profiles (agent.rs:244-247).
5. `has_provider` excludes Claude/Codex static-key accounts (config.rs:528-536) — blocks T21/T22 sync.
6. `agent_config_file` dispatches per-agent with a Codex fall-through (plan.rs:549-555) — wrong format for Claude and for Codex custom providers.
7. Research gaps: MiniMax Anthropic-compatible and Codex endpoints unknown; MiMo SGP Anthropic-protocol endpoint unverified (Kimi Code's Anthropic trait differs, research :161); Kimi Anthropic-compat path unpinned. Update `rewrite/research/agent-provider.md`/`agent-artifacts.md` before the respective units.
8. Same-host multi-profile overlap (api.z.ai/api.kimi.com serve both Anthropic- and OpenAI-compat paths): profiles differ per tuple but policy hosts coincide — closure rule (D10) applies per profile; verify no `_provider_<name>` collision in merged multi-instance policies (policy.rs:251-262).

## 5. Recommended unit order (smallest first)

| # | Unit | Scope | Gate |
|---|---|---|---|
| U0 | Design amendment: D29→22 tuples, T21/T22 billing decision, research refresh for MiniMax/MiMo/Kimi endpoints | `rewrite/{DESIGN.md,decisions.tsv,PLAN.md}`, research docs | Reviewer verdict, no conflict with D01/D03/D05/D10/D24/D27 |
| U1 | jackin-core catalog: per-tuple `TupleSpec` data, delete `omp_*` + profile-asset catch-all, add T13-T22 rows as `Unavailable`, fix `Agent::provider()` consumers (edit.rs, agent_table test) | `jackin-core` (+ edit.rs consumers) | G-FAST; test plan pre-approved per AGENTS.md |
| U2 | 10 profile YAMLs + policy closure tests | `jackin-core/src/assets`, `jackin-openshell` | G-FAST |
| U3 | omp lane T21+T22: render/omp auth per-tuple, `has_provider`, `--agent` disambiguation, account add | `jackin-session`, `jackin` CLI | G-FAST; G-E2E per tuple flipped `Supported` + evidence file |
| U4 | Claude lane T13-T16: `render_claude_config`, settings env block, per-tuple placeholder, per-home copies, roles `[agents.claude]` | `jackin-session`, role repos | G-FAST; G-E2E where a plan key exists (expect T10-T12-style host-key blockers otherwise) |
| U5 | Codex lane T17-T20: `model_providers` renderer, env_key=profile env var, ChatGPT-only auth gating, T01 regression | `jackin-session` | G-FAST; G-E2E T01 + per tuple |
| U6 | Console/testkit first-tuple-by-provider fixes, offers verification, D27 identity gate | `jackin-tui`, testkit, baseline check | G-FULL + identity gate |
| U7 | Finish 1–12 per blockers (T03 quota, T04 strace path, T06 upstream artifact, T07/T08/T10-T12 keys, T02/T05 vendor reply) | live/evidence, blocker files | G-E2E per flip; update `rewrite/blockers/T*.md` |

U3 before U4/U5: the omp adapter already exists end-to-end (T09 proven), so T21/T22 are the smallest live increment; U4/U5 each add one new config renderer; U7 is independent and can interleave.


Unit ID mapping: audit U0 is PLAN A10 design part, U1-U2 are A10, U3 is A11, U4 is A12, U5 is A13, U6 is A15 console part, U7 is A14. The U IDs in the audit are superseded by the A IDs in rewrite/PLAN.md.
