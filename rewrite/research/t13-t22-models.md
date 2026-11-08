# T13-T22 vendor proof: endpoints, auth envs, model names

Status: research snapshot of 2026-11-08 for the D29 extension tuples T13-T22. Every claim below was read the same day from the cited vendor page with `curl`; Mintlify sites were read through their `.md` mirrors (same content as the page URL) and the Kimi VitePress pages through their served HTML. Where a vendor page does not state a fact, the entry says UNKNOWN. No model name in this file is a support claim; a tuple still needs its own live-reply proof per D29. This file does not amend the design.

Related: D29 in [`../DESIGN.md`](../DESIGN.md), per-pair record in [`agent-provider.md`](agent-provider.md) (dated section 2026-11-08).

## How to read this

- "Claude Code lane" facts configure the unmodified Claude Code CLI through its documented `ANTHROPIC_BASE_URL` and auth-token environment variables (semantics per [Claude Code environment variables](https://code.claude.com/docs/en/env-vars): `ANTHROPIC_BASE_URL` overrides the API endpoint, `ANTHROPIC_AUTH_TOKEN` supplies a `Bearer` Authorization header, `ANTHROPIC_API_KEY` supplies an `X-Api-Key` header).
- "Codex lane" facts configure the unmodified Codex CLI through a `model_providers.<id>` entry in `~/.codex/config.toml`. The official [Codex configuration reference](https://developers.openai.com/codex/config-reference) defines `base_url` ("API base URL for the model provider"), `env_key` ("Environment variable supplying the provider API key"), `experimental_bearer_token` (direct bearer token, discouraged; use `env_key`), and `wire_api` ("`responses` is the only supported value, and it is the default when omitted").

## Z.AI (T14 Claude Code, T18 Codex; plan key also serves T09 omp)

Endpoints from the vendor's own table in the [GLM Coding Plan quick start](https://docs.z.ai/devpack/quick-start): Anthropic Messages `https://api.z.ai/api/anthropic`, OpenAI Chat Completions `https://api.z.ai/api/coding/paas/v4`, OpenAI Responses `https://api.z.ai/api/v1`. The [model-switching guide](https://docs.z.ai/devpack/latest-model) restates all three per tool. Plan keys and standard model API keys are separate and not interchangeable ([tool guide](https://docs.z.ai/devpack/tool/codex), [API key management notice](https://docs.z.ai/devpack/notice/api-key-management)).

Claude Code lane ([guide](https://docs.z.ai/devpack/tool/claude)):

- `ANTHROPIC_BASE_URL` = `https://api.z.ai/api/anthropic`
- `ANTHROPIC_AUTH_TOKEN` = the GLM Coding Plan API key
- Model selection through `ANTHROPIC_MODEL` / `ANTHROPIC_DEFAULT_OPUS_MODEL` / `ANTHROPIC_DEFAULT_SONNET_MODEL` / `ANTHROPIC_DEFAULT_HAIKU_MODEL`; the guide's example values are `glm-5.3[1m]` and `glm-5.3-flash[1m]`, with the `[1m]` suffix enabling 1M context ([model-switching guide](https://docs.z.ai/devpack/latest-model)).

Codex lane ([guide](https://docs.z.ai/devpack/tool/codex)):

- `[model_providers.ZAI]` with `base_url = "https://api.z.ai/api/v1"`, `wire_api = "responses"` ("must be set to `responses`"), auth via `experimental_bearer_token = "<API Key>"` (the guide's form; `env_key` is the Codex-reference alternative).
- `model = "glm-5.3"` plus a vendor-provided `~/.codex/models.json` catalog entry for `glm-5.3` declaring reasoning levels `low`, `high`, `max` (default `max`), context window 1048576.
- The guide warns Codex requires the dedicated Responses endpoint `https://api.z.ai/api/v1`; the Chat Completions URL `https://api.z.ai/api/coding/paas/v4` is for other OpenAI-compatible tools, not Codex.

Allowed model names: `glm-5.3` and `glm-5.3-flash` (with optional `[1m]` suffix) are the models named on the plan pages consulted ([quick start](https://docs.z.ai/devpack/quick-start), [model switching](https://docs.z.ai/devpack/latest-model)). The GLM-5.3 entry in the Codex catalog is text-only; GLM-5.3-Flash is multimodal. A full plan roster beyond these two on the current plans: UNKNOWN from the pages consulted.

Live reply: recorded below (omp lane, T09 key reuse).

## Kimi (Moonshot) (T13 Claude Code, T17 Codex; membership key also serves T08/T10)

Claude Code lane ([guide](https://www.kimi.com/code/docs/en/third-party-tools/claude-code)):

- `ANTHROPIC_BASE_URL` = `https://api.kimi.ai/coding/` (trailing slash as documented)
- `ANTHROPIC_API_KEY` = the Kimi Code membership API key (the guide's table: this variable "Authenticates with your Kimi Code API Key"; without it a 401). Note: Kimi documents `ANTHROPIC_API_KEY`, not `ANTHROPIC_AUTH_TOKEN`, for this lane.
- Model selection through `ANTHROPIC_MODEL` plus the full set of `ANTHROPIC_DEFAULT_*_MODEL` and `CLAUDE_CODE_SUBAGENT_MODEL`; example model `k3.1-pro` and `k3.1-pro[1m]` for 1M context, effort via `CLAUDE_CODE_EFFORT_LEVEL`.
- A China Anthropic-protocol base URL for Claude Code: UNKNOWN (this guide documents only `https://api.kimi.ai/coding/`).

Codex lane ([guide](https://www.kimi.com/code/docs/en/third-party-tools/codex)):

- The Kimi Code server natively supports the OpenAI Responses API (guide introduction).
- `[model_providers.kimi]` with `base_url = "https://api.kimi.ai/coding/v1"` (Overseas) or `https://api.kimi.com/coding/v1` (China), `wire_api = "responses"`, auth via `env_key = "KIMI_API_KEY"` (the variable name is documented in the guide's field reference) or alternatively `experimental_bearer_token` in the config; the guide forbids setting both.
- Vendor catalog entries for `k3.1-pro` and `k3.1-pro-highspeed`, both 1M context.

Allowed model names ([models page](https://www.kimi.com/code/docs/en/kimi-code/models), six IDs): `k3.1-pro`, `k3.1-pro-highspeed`, `k3`, `k3-256k`, `kimi-for-coding` (K2.8 Preview), `kimi-for-coding-highspeed` (K2.7 Code Highspeed). Availability depends on membership plan (Plus/Pro tiers; legacy Moderato/Allegretto/Andante); 1M context is Pro-tier. Highspeed variants consume quota faster (about 2x for K3.1 Pro Highspeed, about 3x for K2.7 Code Highspeed). `[1m]` suffix applies where the model supports 1M context.

Live reply: no host key, live proof pending.

## MiniMax (T15 Claude Code, T19 Codex; plan key also serves T11)

Claude Code lane ([guide](https://platform.minimax.io/docs/m-plan/claude-code)):

- `ANTHROPIC_BASE_URL` = `https://api.minimax.io/anthropic`
- `ANTHROPIC_AUTH_TOKEN` = the M Plan API key (from the plan console)
- Example model env `MiniMax-M3.1-Flash[1m]`; verify with `/status` (base URL) and `/model` (model). The guide requires clearing conflicting `ANTHROPIC_AUTH_TOKEN`/`ANTHROPIC_BASE_URL` exports first.
- The Anthropic-compatible surface is documented in the [Anthropic SDK page](https://platform.minimax.io/docs/api-reference/text-anthropic-api): request URL `https://api.minimax.io/anthropic/v1/messages`, Bearer auth, models limited to `MiniMax-M3.1-Flash` and `MiniMax-M3` ("the Anthropic API compatibility interface currently only supports" these two). Effort `low|medium|high|xhigh|max` via `output_config.effort` (default `max`); `MiniMax-M3.1-Flash` always thinks (400 on `disabled`); `MiniMax-M3` thinks only when enabled.

Codex lane ([guide](https://platform.minimax.io/docs/m-plan/codex)):

- `[model_providers.minimax]` with `base_url = "https://api.minimax.io/v1"`, `wire_api = "responses"`, auth via `experimental_bearer_token = "<MINIMAX_API_KEY>"` (guide's form), plus `preferred_auth_method = "apikey"` and `forced_login_method = "api"` to select API-key authentication.
- `model = "MiniMax-M3.1-Flash"` with a vendor catalog entry declaring reasoning levels `low|medium|high|xhigh|max` (default `max`); `none` returns 400.

Allowed model names: `MiniMax-M3.1-Flash` and `MiniMax-M3` on both documented lanes ([Anthropic SDK page](https://platform.minimax.io/docs/api-reference/text-anthropic-api); the [list-models reference](https://platform.minimax.io/docs/api-reference/models/anthropic/list-models) shows exactly these two in its response example, `GET https://api.minimax.io/anthropic/v1/models`). Whether other M-series models are addable to these lanes: UNKNOWN from the pages consulted.

Live reply: no host key, live proof pending.

## Xiaomi MiMo (T16 Claude Code, T20 Codex; Token Plan key also serves T12)

Two credential paths with different hosts per the [Token Plan subscription page](https://mimo.mi.com/docs/en-US/tokenplan/Token%20Plan/subscription): pay-as-you-go keys (`sk-…`) and Token Plan keys (`tp-…` individual, `ttp-…` team). Token Plan base URLs are per plan and displayed in the plan console, and the subscription page documents all three regional clusters for both protocols:

- OpenAI protocol: China `https://token-plan-cn.xiaomimimo.com/v1`, Singapore `https://token-plan-sgp.xiaomimimo.com/v1`, European `https://token-plan-ams.xiaomimimo.com/v1`
- Anthropic protocol: `https://token-plan-cn.xiaomimimo.com/anthropic`, `https://token-plan-sgp.xiaomimimo.com/anthropic`, `https://token-plan-ams.xiaomimimo.com/anthropic`

Pay-as-you-go hosts ([Claude Code guide](https://mimo.mi.com/docs/en-US/tokenplan/integration/claudecode), [Codex guide](https://mimo.mi.com/docs/en-US/tokenplan/integration/codex-configuration)): `https://api.xiaomimimo.com/anthropic` and `https://api.xiaomimimo.com/v1`. D29 T16/T20 are class S (Token Plan), so the regional `token-plan-*` hosts above are the tuple-relevant ones.

Claude Code lane ([guide](https://mimo.mi.com/docs/en-US/tokenplan/integration/claudecode)):

- `ANTHROPIC_BASE_URL` = the plan's Anthropic-protocol base URL (regional list above)
- `ANTHROPIC_AUTH_TOKEN` = the Token Plan API key
- Model selection through `ANTHROPIC_MODEL` / `ANTHROPIC_DEFAULT_*_MODEL`; example `mimo-v2.6-pro`, `[1m]` suffix for 1M-context models (example `mimo-v2.6-pro[1m]`).

Codex lane ([guide](https://mimo.mi.com/docs/en-US/tokenplan/integration/codex-configuration)):

- `[model_providers.mimo]` with `base_url` = the plan's OpenAI-protocol base URL (regional list above), `wire_api = "responses"`, auth via `experimental_bearer_token` (`tp-your-api-key-here` in the Token Plan example).
- `model = "mimo-v2.6-pro"`, `model_provider = "mimo"`, `model_reasoning_effort = "high"`, vendor model catalog file `model-catalogs.json` with entries for `mimo-v2.6-pro`, `mimo-v2.6-flash`, `mimo-v2.6-pro-ultraspeed`.

Allowed model names ([models page](https://mimo.mi.com/docs/en-US/quick-start/summary/model)): current text-generation IDs `mimo-v2.6-pro`, `mimo-v2.6-flash`, `mimo-v2.6-pro-ultraspeed`; `mimo-v2.5-pro` and `mimo-v2.5` were deprecated 2026-10-21. Live model listing endpoint `GET https://api.xiaomimimo.com/v1/models` with `api-key` or Bearer auth ([reference](https://mimo.mi.com/docs/en-US/api/model/list-models-get)).

Live reply: no host key, live proof pending.

## Anthropic (T21 omp, API key only)

- omp provider id `anthropic`; credential resolution `ANTHROPIC_OAUTH_TOKEN` then `ANTHROPIC_API_KEY` ([omp providers doc](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md), core providers table). T21 is class A, so the API-key path is the tuple-relevant one; the OAuth-token variable belongs to omp's login flow, not a class A key.
- Vendor endpoint: the Claude API is a REST API at `https://api.anthropic.com` ([API overview](https://platform.claude.com/docs/en/api/overview)); `POST /v1/messages` with `Authorization: Bearer $ANTHROPIC_API_KEY` and `anthropic-version` header ([Messages API reference](https://platform.claude.com/docs/en/api/messages)).
- Allowed model names ([models overview](https://platform.claude.com/docs/en/about-claude/models/overview), current API IDs): `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5`, `claude-fable-5-1` (all 1M context, API alias equals the ID).
- Live reply: no host key, live proof pending.

## OpenAI (T22 omp, API key only)

- omp provider id `openai`; credential `OPENAI_API_KEY` ([omp providers doc](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md), core providers table).
- Vendor endpoint: `https://api.openai.com/v1` is the server URL of the official OpenAPI description of the OpenAI API ([openai/openai-openapi `openapi.yaml`](https://github.com/openai/openai-openapi/blob/master/openapi.yaml), `servers:` block). The platform API-reference HTML is script-rendered and not curl-readable, so the spec repo is the cited artifact.
- Allowed model names: `gpt-6.1-terra` (Model ID on its [platform model page](https://platform.openai.com/docs/models/gpt-6.1-terra)), `gpt-6-astra` ([page](https://platform.openai.com/docs/models/gpt-6-astra)), `gpt-5.3-codex` ([page](https://platform.openai.com/docs/models/gpt-5.3-codex)). The [Codex models doc](https://developers.openai.com/codex/models) additionally names `gpt-6.1-sol`, `gpt-6-luna`, `gpt-6-sol` (replacement for retired `gpt-5.4`), and records the `gpt-5.5` retirement and the August 2026 retirement of `gpt-5.4`/`gpt-5.4-mini` from Codex with ChatGPT sign-in. The complete model list for a given API key is `GET /v1/models`; the platform `docs/models` index page is script-rendered, so a full curl-readable roster is UNKNOWN from these pages.
- Live reply: no host key, live proof pending.

## omp provider IDs (context for the extension key reuse)

From the [omp providers doc](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md): provider IDs select the backend and prefix model selection (documented examples `inception/mercury-3`, `cloudflare-workers-ai/@cf/...`; the host command below used `zai/glm-5.3`). IDs and key variables relevant to T08-T12 and T21-T22: `zai` (`ZAI_API_KEY`), `moonshot` (`MOONSHOT_API_KEY` then `KIMI_API_KEY`), `minimax-code` (`MINIMAX_CODE_API_KEY`), `xiaomi-token-plan-cn|sgp|ams` (`XIAOMI_TOKEN_PLAN_CN|SGP|AMS_API_KEY`), `anthropic` (`ANTHROPIC_OAUTH_TOKEN` then `ANTHROPIC_API_KEY`), `openai` (`OPENAI_API_KEY`). `kimi-code` is an OAuth-backed omp provider reached through `/login` (doc's OAuth-provider list).

## Live reply record (2026-11-08)

Host: macOS arm64, omp 18.18.1, host profile `zai` (the only host omp store key for this program; per the operator, no other provider key is available on this host).

Command (no secret on the command line; the key stays in the omp store):

```
omp --profile zai --model zai/glm-5.3 --thinking max -p "Reply with exactly this token and nothing else: JACKIN_ZAI_GLM53_T09_LIVE_OK"
```

Observed stdout:

```
Working...
JACKIN_ZAI_GLM53_T09_LIVE_OK
```

Exit status 0, wall time about 1.7 s. This proves a live `glm-5.3` reply through the host omp Z.AI coding-plan lane (T09's lane) on 2026-11-08. It does not by itself prove any T13-T20 sandbox lane or the absence of paid fallback; those need their own per-tuple gateway proofs per D29.

All other providers (Kimi, MiniMax, MiMo, Anthropic, OpenAI): no host key, live proof pending.

## Per-tuple summary

| Tuple | Agent | Provider | Anthropic lane (base URL / auth env) | Codex lane (base URL / wire) | Documented models | Live reply |
|---|---|---|---|---|---|---|
| T13 | Claude Code | Kimi | `https://api.kimi.ai/coding/` / `ANTHROPIC_API_KEY` | — | `k3.1-pro`, `k3.1-pro-highspeed`, `k3`, `k3-256k`, `kimi-for-coding`, `kimi-for-coding-highspeed` (`[1m]` where supported) | pending |
| T14 | Claude Code | Z.AI | `https://api.z.ai/api/anthropic` / `ANTHROPIC_AUTH_TOKEN` | — | `glm-5.3`, `glm-5.3-flash` (`[1m]`) | omp lane proved 2026-11-08; Claude Code lane pending |
| T15 | Claude Code | MiniMax | `https://api.minimax.io/anthropic` / `ANTHROPIC_AUTH_TOKEN` | — | `MiniMax-M3.1-Flash`, `MiniMax-M3` (`[1m]` on Flash) | pending |
| T16 | Claude Code | MiMo | `https://token-plan-{cn,sgp,ams}.xiaomimimo.com/anthropic` / `ANTHROPIC_AUTH_TOKEN` | — | `mimo-v2.6-pro`, `mimo-v2.6-flash`, `mimo-v2.6-pro-ultraspeed` (`[1m]` where supported) | pending |
| T17 | Codex | Kimi | — | `https://api.kimi.ai/coding/v1` (overseas) / `https://api.kimi.com/coding/v1` (China), `responses` | same six IDs as T13 | pending |
| T18 | Codex | Z.AI | — | `https://api.z.ai/api/v1`, `responses` | `glm-5.3`, `glm-5.3-flash` | pending |
| T19 | Codex | MiniMax | — | `https://api.minimax.io/v1`, `responses` | `MiniMax-M3.1-Flash`, `MiniMax-M3` | pending |
| T20 | Codex | MiMo | — | `https://token-plan-{cn,sgp,ams}.xiaomimimo.com/v1`, `responses` | `mimo-v2.6-pro`, `mimo-v2.6-flash`, `mimo-v2.6-pro-ultraspeed` | pending |
| T21 | omp | Anthropic | — (omp provider `anthropic`; API at `https://api.anthropic.com`) | — | `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5`, `claude-fable-5-1` | pending |
| T22 | omp | OpenAI | — (omp provider `openai`; API at `https://api.openai.com/v1`) | — | `gpt-6.1-terra`, `gpt-6-astra`, `gpt-6.1-sol`, `gpt-6-luna`, `gpt-5.3-codex` (full roster via `/v1/models`) | pending |

UNKNOWN summary: Kimi's China Anthropic-protocol base for Claude Code (only `api.kimi.ai` documented on that page); the complete current plan model rosters of Z.AI and MiniMax beyond the names their pages list; a curl-readable full OpenAI API model index. All other facts above carry a vendor citation.
