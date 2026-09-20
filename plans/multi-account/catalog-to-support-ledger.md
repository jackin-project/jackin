# Catalog-to-support ledger (maintained)

Status: maintained. Distilled 2026-09-17 from the research draft
`/tmp/provider-catalog-ledger.md` (359 rows, not committed; fetch date 2026-09-17 UTC).
Home: this file lives next to [ledger.md](./ledger.md) and
[001-t02-domain-contracts.md](./001-t02-domain-contracts.md) because it is a working
ledger for the multi-account plan, not published documentation
(`docs/content/research/` holds published research pages; see
[research index](../../docs/content/research/index.mdx)).

For every client below: complete provider catalog scope, then per relevant provider —
auth modes, jackin launch route (implemented / planned / blocked + file refs where
code exists), jackin usage source (implemented / planned / exact unsupported reason),
evidence class + pinned SHA / fetch date.

## 0. No-fixed-list-filtering rule (B16)

Source: [jackin-implementation-and-verification.md](../../jackin-implementation-and-verification.md)
checklist item B16:

> B16 Every provider entry found in registered client stores appears in the
> catalog-to-support ledger and Settings; each eligible configured launch and
> available usage source has implementation/proof or an explicit blocker, with no
> fixed-list filtering.

Consequences for this ledger:

- Every provider id found in a registered client store/catalog appears here — in a
  scope list at minimum. Nothing is dropped for being obscure, low-value, or an edge
  case.
- Compression rule: providers sharing identical auth + launch + usage shape are
  covered by a per-client **default template**; any provider that differs from the
  default gets its own **deviation row**. A deviation row is added the moment a
  difference is found — the default never hides a known difference.
- Code consequence: jackin must not filter these catalogs against a hardcoded
  provider list. Unknown ids stay visible with status `not_run` / `unsupported`
  (with evidence), never silently dropped.

## 1. Pinned sources

| Client | Repo / origin | Pinned SHA / fetch | Registry path |
|---|---|---|---|
| OpenCode | anomalyco/opencode | `5a8335857b0ebec44ef6aa1d52b339cf25c329ca` | `packages/opencode/src/provider/`, `packages/core/src/plugin/provider/`, live `https://models.opencode.ai/api.json` (fetched 2026-09-17) |
| omp | can1357/oh-my-pi | `116190d317ca319ae17ab624cb479c76a1ca4704` | `packages/ai/src/registry/`, `packages/catalog/src/compat/rules/auth/*.kdl`, `packages/catalog/src/models.json`, `packages/ai/src/usage/` |
| Hermes | NousResearch/hermes-agent | `f5d192611032025d2757b07ad838921872126182` | `website/docs/integrations/providers.md`, `hermes_cli/auth.py` `PROVIDER_REGISTRY`, `hermes_cli/models_catalog_static.py` `CANONICAL_PROVIDERS`, `plugins/model-providers/*` |
| Codex docs | learn.chatgpt.com | live, fetched 2026-09-17 | `/docs/config-file/config-reference` |
| Claude docs | code.claude.com | live, fetched 2026-09-17 | `/docs/en/model-config`, `/docs/en/llm-gateway*`, `/docs/en/env-vars`, `/docs/en/plugins` |
| Ref contracts A–E | CodexBar `b6e65a83dc471817b7ff7678e68e0204c9dd604f`, openusage `56378e5765f85d38ff413036fd984afe3d4664e4`, kimi-cli `86f136422a0aae6b217ea49e7ea1d2e8a1defcd2`, grok-build `482711333c7195dc16a272777f86086d615e2afb`, muse-code-sdk `94e98141a8074d50d2ac418de7acb5b16d8024fa`, minimax-cli `bfbb4cb75ec343149eaccfd668c5011aa27bcf2b` (+ omp/Hermes/OpenCode SHAs above) | fetched 2026-09-17 | `/tmp/ref-contracts-A.md` … `/tmp/ref-contracts-E.md` (research inputs, not committed) |

## 2. Legend

### 2.1 Launch / usage status

`implemented` (code exists in this repo, file ref given) · `planned` (no code yet;
this ledger is the spec) · `blocked` (cannot be done; exact reason given).

### 2.2 Proof levels (same scale as [ledger.md](./ledger.md))

`implemented` < `fixture_verified` < `container_verified` < `live_verified`.
Also: `failed`, `unavailable` (credentials), `unsupported` (genuinely, with
evidence), `not_run`. In this ledger `implemented` means the cited code was
inspected in this tree; fixture/container/live verification is tracked in
[ledger.md](./ledger.md) provider lanes (all `not_run` at last update).

### 2.3 Evidence classes (per ref-contracts A–E headers)

- **D** — wire contract: request/handler code at a pinned SHA (URL + method + auth
  observed), or a live first-party vendor doc (fetch date pinned).
- **S** — observed in pinned first-party source or a live first-party surface
  (registries, auth KDLs, plugins, models.dev snapshot, CLI behavior), but not a
  wire contract. Catalog membership and store-schema facts are **S**.
- **R** — referenced only in docs/comments/fixtures, or third-party reference
  behavior (CodexBar / OpenUsage / omp collectors hitting private endpoints):
  not a vendor commitment.
- **U** — unverified or absent. Every **U** carries an id (`U1`…`Un`) resolved in
  the [U-register](#9-u-register-exact-missing-proofs).

### 2.4 Launch-route codes

- `OC` — launch `opencode` TUI in container with a staged per-instance XDG slice:
  filtered `$XDG_DATA_HOME/opencode/auth.json` single-provider entry + `opencode.json`
  provider/model preset; scrub ambient `*_API_KEY`.
- `OMP` — launch `omp` TUI with a staged `PI_CODING_AGENT_DIR` slice: filtered SQLite
  `agent.db` credential row(s) for the selected account only + model preset;
  `OMP_PROFILE` isolation.
- `HER` — launch `hermes --tui -p <profile>` with a staged `HERMES_HOME` profile
  slice: `auth.json` credential-pool entry + `.env` key + `config.yaml model:` block;
  no root-auth inheritance.
- `CX` — launch `codex` with a staged config.toml `[model_providers."<id>"]` block
  (`base_url` + `env_key`) for the selected account only.
- `CC` — launch `claude` with per-instance env (`ANTHROPIC_BASE_URL` + credential +
  `ANTHROPIC_MODEL` / alias pins) + isolated `CLAUDE_CONFIG_DIR`. Never a plugin.

### 2.5 Usage-source codes

`implemented <file>` — collector exists. `planned <endpoint>` — jackin-usage
collector to implement against the named endpoint. `none:` + exact reason —
unsupported, stays visible on the Usage screen with that reason.

## 3. OpenCode

Catalog scope: **220 providers live** (2026-09-17) · **159 in the pinned-SHA test
fixture** (`packages/opencode/test/tool/fixtures/models-api.json`) · delta: 62 added
since fixture, 1 removed (`github-models`, fixture-only). Model counts below are
live-catalog counts at fetch date.

Full live id list (220, every entry per B16):

```text
302ai abacus abliteration-ai above agentrouter agnes ai-router ai21 aiand aihubmix
aixy aki-io alibaba alibaba-cn alibaba-coding-plan alibaba-coding-plan-cn
alibaba-token-plan alibaba-token-plan-cn amazon-bedrock ambient amd anthropic anyapi
arcee atomic-chat auriko azure azure-cognitive-services bailing baseten berget
blueclaw bothub cerebras chutes clarifai claudinio cline-pass cloudferro-sherlock
cloudflare-ai-gateway cloudflare-workers-ai cohere coralbricks cortecs crof
crossmodel crusoe daoxe databricks deepinfra deepseek digitalocean dinference drun
ebcloud echo edenai empiriolabs evroc fastrouter fireworks-ai freemodel friendli
frogbot github-copilot gitlab gmicloud google google-vertex google-vertex-anthropic
greenpt groq helicone hetzner hpc-ai huggingface hyper iflowcn impossibl inception
inceptron inco infer inference inferx infomaniak io-net iteracompute jalapeno jiekou
kenari kilo kimi-for-coding klokintegration kosmik kuae-cloud-coding-plan lilac
llama llmgateway llmgateway-providers llmtech llmtr lmstudio longcat lucidquery lynkr
meganova melious merge-gateway meta minimax minimax-cn minimax-cn-coding-plan
minimax-coding-plan mistral mixlayer moark modal model-oracle-ai modelis modelscope
moonshotai moonshotai-cn morph nan nano-gpt nearai nebius neon neosmith neuralwatt
nova novita-ai nvidia oci ofox ollama-cloud openai opencode opencode-go openreason
openrouter opper orcarouter ovhcloud pendra perplexity perplexity-agent pioneer poe
poolside privatemode-ai qihang-ai qiniu-ai qvac regolo-ai requesty routing-run
runinfra sakana salad-cloud sap-ai-core sarvam scaleway scnet-token-plan scx-ai
sensenova siliconflow siliconflow-cn snowflake-cortex stackit standardcompute stepfun
stepfun-ai stepfun-ai-step-plan stepfun-step-plan subconscious submodel synthetic
tencent-coding-plan tencent-token-plan tencent-tokenhub tensorx the-grid-ai
thinkingmachines tinfoil togetherai tokengo tokenrouter trustedrouter umans-ai
umans-ai-coding-plan unorouter upstage v0 vancine venice vercel vispark vivgrid
volcengine volcengine-coding-plan vultr wafer.ai wallaby wandb watsonx xai xiaomi
xiaomi-token-plan-ams xiaomi-token-plan-cn xiaomi-token-plan-sgp xpersona zai
zai-coding-plan zeldoc zenifra zenmux zhipuai zhipuai-coding-plan
```

Fixture-only id (removed live): `github-models`.

Auth architecture (**S**): credential = `auth.json` union
{`oauth`{refresh,access,expires,accountId}, `api`{key,metadata},
`wellknown`{key,token}} at `$XDG_DATA_HOME/opencode/auth.json`
(+`OPENCODE_AUTH_CONTENT` override); provider auth methods come from plugin
`auth.provider` hooks (`oauth`|`api` + prompts); env keys per models.dev `env[]`
also honored. Only 2 first-party OAuth integrations at this SHA: `openai` (ChatGPT
browser PKCE localhost:1455 + headless device-code) and `opencode` (device-code
against opencode.ai/console). Models config: each provider record =
{api?, name, env[], id, npm?, models{…cost?, limit{…}…}}; snapshot baked at build,
refreshed from `https://models.opencode.ai` (5-min TTL). Usage: per-session token +
cost accounting from models.dev `cost` only — **no per-provider quota/balance/credit
API anywhere in the pinned tree** (zero usage/balance/credit refs in provider
plugins).

### 3.1 Default template (applies to every id above unless a deviation row exists)

| Aspect | Value |
|---|---|
| Auth | `env(<PROVIDER>_API_KEY)` per models.dev `env[]`; `@ai-sdk/openai-compatible` or equivalent 3rd-party SDK. Ev **S**. |
| Launch | `planned` `OC`. Related implemented code: [adapter](../../crates/jackin-core/src/agent/adapters/opencode.rs), [store enumerator](../../crates/jackin-config/src/accounts/stores/opencode.rs). Proof `not_run`. |
| Usage | `none:` no per-provider quota API in OpenCode; only local session tokens/cost. Shown on Usage screen with this reason. Ev **S**. Proof `unsupported` (evidenced by zero refs in pinned tree). |

### 3.2 Auth deviations (auth ≠ plain env key)

| Provider | Auth | Ev |
|---|---|---|
| `openai` | `oauth` (ChatGPT browser/headless PKCE + refresh) or `env(OPENAI_API_KEY)`; Responses wire | **S** |
| `opencode` | `oauth` (device-code via opencode.ai/console) or `env(OPENCODE_API_KEY)`; keyless free models w/ `apiKey=public` | **S** |
| `github-copilot` | Copilot OAuth token; chat-vs-responses route select (GPT-5+) | **S** |
| `amazon-bedrock` | `sdk` AWS chain (bearer/IAM/profile/ECS/IRSA); Converse/Mantle select | **S** |
| `google-vertex`, `google-vertex-anthropic` | GCP ADC / service-account; regional endpoint build (Anthropic-on-Vertex uses us/eu REP domains) | **S** |
| `azure`, `azure-cognitive-services` | `env(AZURE_API_KEY)` + resource-name var; deployment/compat URL build; chat/responses/messages select | **S** |
| `cloudflare-ai-gateway` | `env(CLOUDFLARE_API_TOKEN/CF_AIG_TOKEN)` + account/gateway id; native passthrough select | **S** |
| `cloudflare-workers-ai` | `env(CLOUDFLARE_API_KEY)`; Workers AI binding | **S** |
| `gitlab` | `env(GITLAB_TOKEN)` + `GITLAB_INSTANCE_URL`; Duo agentic/workflow chat | **S** |
| `sap-ai-core` | SAP AICore OAuth client-credentials (service binding) | **S** |
| `snowflake-cortex` | programmatic token; Cortex REST | **S** |
| `opencode-go` | `env(OPENCODE_API_KEY)`; same org as `opencode` — shared billing identity, see §8 | **S** |

Launch/usage for these rows: launch `planned` `OC` (proof `not_run`); usage `none`
per §3.1 except the three rows in §3.3.

### 3.3 Usage deviations (usage ≠ none)

| Provider | Usage source | Status | Ev | Proof |
|---|---|---|---|---|
| `opencode`, `opencode-go` | `GET https://opencode.ai/zen/go/v1/usage` (rolling/weekly/monthly; no Zen-balance/live-model fields) | `implemented` [opencode.rs](../../crates/jackin-usage/src/usage/opencode.rs) | **S**+**R** (endpoint known via reference collectors, not vendor-documented) | `implemented` |
| `openrouter` | `GET https://openrouter.ai/api/v1/key` (ordinary-key cap, remaining, period spend and BYOK); `/credits` and delayed `/activity` require a Management key | `implemented` [openrouter.rs](../../crates/jackin-usage/src/usage/openrouter.rs) and wired through the canonical OpenRouter surface; management-only enrichment remains unavailable under the current account credential contract ([U12](#9-u-register-exact-missing-proofs)) | **D** (official key, credits and activity contracts) | `fixture_verified` |

## 4. omp (oh-my-pi)

Catalog scope: **84 auth rules** (`packages/catalog/src/compat/rules/auth/*.kdl` +
`_order.kdl` login roster) · **69 providers in `models.json`** · 20 `UsageProvider`s
in `DEFAULT_USAGE_PROVIDERS`. Note: `minimax-cn` exists in models.json but has **no
auth KDL** (gap, see [U2](#9-u-register-exact-missing-proofs)).

Full auth-rule id list (84, every entry per B16; login kind + model count in
parentheses, `nomodels` = absent from models.json):

```text
abliteration(api-key,3m) aiand(api-key,11m) aimlapi(no-login,349m)
alibaba-coding-plan(custom,12m) alibaba-token-plan(custom,8m)
amazon-bedrock(no-login,182m) anthropic(oauth-code,25m) azure(no-login,40m)
baseten(api-key,17m) bedrock-mantle(no-login,5m) cerebras(api-key,8m)
charm-hyper(api-key,nomodels) cline-pass(api-key,20m)
cloudflare-ai-gateway(custom,95m) commandcode(api-key,69m) coreweave(api-key,39m)
cursor(custom,119m) deepinfra(api-key,106m) deepseek(api-key,4m) devin(oauth-code,2m)
exa(api-key,nomodels) firepass(api-key,2m) fireworks(api-key,35m)
github-copilot(custom,47m) gitlab-duo-agent(oauth-code,1m) gitlab-duo(oauth-code,16m)
gmi-cloud(api-key,1m) google-antigravity(oauth-code,20m)
google-gemini-cli(oauth-code,7m) google-vertex(no-login,31m) google(no-login,44m)
groq(no-login,20m) huggingface(api-key,76m) kagi(api-key,nomodels) kilo(custom,566m)
kimi-code(device-code,7m) litellm(api-key,nomodels) llama.cpp(api-key,nomodels)
lm-studio(api-key,nomodels) meta(api-key,5m) minimax-code-cn(api-key,9m)
minimax-code(api-key,9m) minimax(no-login,8m) mistral(no-login,31m)
moonshot(api-key,17m) muse-code(device-code,5m) nanogpt(api-key,1074m)
novita(api-key,117m) nvidia(api-key,169m) ollama-cloud(api-key,51m)
ollama(api-key,nomodels) openai-codex-device(custom,nomodels)
openai-codex(oauth-code,6m) openai(no-login,55m) opencode-go(api-key,35m)
opencode-zen(api-key,95m) openrouter(oauth-code,525m) parallel(api-key,nomodels)
perplexity(custom,nomodels) qianfan(api-key,1m) qwen-portal(api-key,2m)
sakana(api-key,3m) siliconflow-cn(api-key,nomodels) siliconflow(api-key,nomodels)
synthetic(api-key,11m) tavily(api-key,nomodels) together(api-key,39m)
typesafe(api-key,nomodels) umans(api-key,9m) venice(api-key,156m)
vercel-ai-gateway(api-key,308m) vllm(api-key,nomodels) wafer-serverless(api-key,20m)
xai-oauth(device-code,9m) xai(api-key,31m) xiaomi-token-plan-ams(api-key,4m)
xiaomi-token-plan-cn(api-key,4m) xiaomi-token-plan-sgp(api-key,4m) xiaomi(custom,6m)
yolo-auto(api-key,1m) zai-coding-plan(oauth-code,nomodels) zai(api-key,16m)
zenmux(api-key,264m) zhipu-coding-plan(api-key,15m)
```

Auth architecture (**S**): single registry `PROVIDER_REGISTRY` derived from the auth
KDLs; credentials in SQLite `agent.db` (`api_key`|`oauth`, multi-row account pools,
refresh leases, `usage_history` table). Login kinds: `oauth-code` (PKCE browser
callback), `device-code`, `api-key` (paste+validate), `custom` (bespoke TS handler
in `registry/oauth/`), none (env/config-only or bare rule). `auth-broker` (+gateway)
mirrors credentials for remote use; refresh owned solely by AuthStorage.

### 4.1 Default templates (by login kind; overridden by deviation rows)

| Login kind | Auth | Launch | Usage |
|---|---|---|---|
| `api-key` | paste+validate into `agent.db`. Ev **S**. | `planned` `OMP`. Related implemented code: [adapter](../../crates/jackin-core/src/agent/adapters/omp.rs), [store enumerator](../../crates/jackin-config/src/accounts/stores/omp.rs), [attribution adapter](../../crates/jackin-usage/src/usage/omp.rs) (attributes to underlying providers; omp has no native quota API). Proof `not_run`. | `none:` no omp UsageProvider and no vendor quota API evidenced. Ev **S**. Proof `unsupported`. |
| `oauth-code` / `device-code` | PKCE browser callback / device-code; refresh owned by AuthStorage. Ev **S**. | Same `planned` `OMP` as above. | Same `none` as above unless a §4.2 row exists. |
| `custom` | bespoke TS handler in `registry/oauth/`. Ev **S**. | Same `planned` `OMP` as above. | Same `none` as above unless a §4.2 row exists. |
| no-login (`aimlapi`, `amazon-bedrock`, `bedrock-mantle`, `google-vertex`, `minimax`) | env/config-only or SDK hook (`sdk` for bedrock/vertex). Ev **S**. | Same `planned` `OMP` as above. | `none:` as above. |
| no-login (`azure`, `google`, `groq`, `mistral`, `openai`) | env/config-only works (**S**); managed-login flow **U** ([U1](#9-u-register-exact-missing-proofs): no auth KDL at pinned SHA). | Same `planned` `OMP` as above. | `none:` as above. |

Tool/search-only providers (`exa`, `kagi`, `tavily`, `parallel`, `typesafe`) and
local shells (`ollama`, `lm-studio`, `llama.cpp`, `vllm`, `litellm`) intentionally
lack quota semantics: usage `none` (no account / local server), ev **S**.

### 4.2 Usage deviations (an omp UsageProvider or vendor endpoint exists)

Status `planned` below = jackin-usage collector (or attribution wiring) still to
write; `implemented` = collector exists in this tree. Endpoints marked **R** are
private/reference-grade until vendor-documented or Mac-live-verified
([U11](#9-u-register-exact-missing-proofs)).

| Provider | Usage source | Status | Ev |
|---|---|---|---|
| `anthropic` | `planned GET {base}/api/oauth/usage` + `profile`; OAuth scope-gated (same contract as native Claude lane) | `planned` attribution to [claude.rs](../../crates/jackin-usage/src/usage/claude.rs) ([U11](#9-u-register-exact-missing-proofs)) | **R** (ref-contracts-A; omp `usage/claude.ts`) |
| `openai-codex`, `openai-codex-device` | `planned` wham/usage + reset-credit inventory; prefer app-server `account/*` (same contract as native Codex lane) | `planned` attribution to [codex.rs](../../crates/jackin-usage/src/usage/codex.rs) ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `kimi-code` | `GET https://api.kimi.com/coding/v1/usages` (OAuth + key paths) | `implemented` [kimi.rs](../../crates/jackin-usage/src/usage/kimi.rs); attribution wiring `planned` | **D** (kimi-cli `usage.py` + omp `usage/kimi.ts` at pinned SHAs) |
| `zai`, `zai-coding-plan` | `planned api.z.ai /api/monitor/usage/quota/limit` + model-usage; CREDIT vs TOKENS_LIMIT pools; plan-scoped | `implemented` [zai.rs](../../crates/jackin-usage/src/usage/zai.rs); attribution wiring `planned` | **R** |
| `minimax-code` | `GET {base}/v1/token_plan/remains` (Global `api.minimax.io` / China `api.minimaxi.com`) | `implemented` [minimax.rs](../../crates/jackin-usage/src/usage/minimax.rs); attribution wiring `planned` | **S** |
| `minimax-code-cn` | same remains API on CN host | `planned` — verify CN-host behavior ([U3](#9-u-register-exact-missing-proofs)) | **U** |
| `opencode-go` | `GET https://opencode.ai/zen/go/v1/usage` | `implemented` [opencode.rs](../../crates/jackin-usage/src/usage/opencode.rs); attribution wiring `planned` | **R** |
| `openrouter` | `GET {base}/key` (ordinary-key cap, remaining and period spend); `/credits` and delayed `/activity` require a Management key | `implemented` [openrouter.rs](../../crates/jackin-usage/src/usage/openrouter.rs); management-only enrichment remains unavailable under the current account credential contract ([U12](#9-u-register-exact-missing-proofs)) | **D** (official key, credits and activity contracts) |
| `github-copilot` | `planned api.github.com copilot quota` | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `google-antigravity` | `planned daily-cloudcode-pa :retrieveUserQuotaSummary`; bind identity | `planned` — see also native [antigravity.rs](../../crates/jackin-usage/src/usage/antigravity.rs) (`agy -p /usage --output-format json`) ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `google-gemini-cli` | `planned` cloudcode-pa Code Assist quota; OAuth only | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `muse-code` | cached MSP observation; key-exchange is a separate op, never auto-poll | `planned` — see native [muse.rs](../../crates/jackin-usage/src/usage/muse.rs) ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `xai-oauth` | `planned` Grok CLI billing endpoint; weekly/monthly split | `planned` — see native [grok.rs](../../crates/jackin-usage/src/usage/grok.rs) ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `cursor` | `planned cursor.com/api/usage-summary` + `/api/auth/me`; personal scope | `planned` — see native [cursor.rs](../../crates/jackin-usage/src/usage/cursor.rs) ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `devin` | `planned SeatManagementService/GetUserStatus`; no REST usage endpoint | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `cline-pass` | `planned /users/me` + `/users/me/plan/usage-limits` | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `charm-hyper` | `planned` credits endpoint | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `synthetic` | `planned api.synthetic.new/v2/quotas` | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `umans` | `planned api.code.umans.ai/v1/usage` | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `alibaba-token-plan` | `planned` bailian console BroadScopeAspnGateway token-plan API | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **R** |
| `ollama-cloud`, `ollama` | `none:` no quota endpoint; register view only (local/cloud shells) | — | **S** |
| `minimax-cn` | auth **U** ([U2](#9-u-register-exact-missing-proofs)): models.json-only, no auth KDL | `blocked` launch until auth resolves | **U** |

Proof for all `planned` rows above: `not_run`. Proof for `implemented` collectors:
`implemented` (code inspected; lane verification still `not_run` in
[ledger.md](./ledger.md)).

## 5. Hermes

Catalog scope: **55 rows** across three overlapping sources at the pinned SHA:
`PROVIDER_REGISTRY` (`auth.py`, 38 rows: 6 oauth/bespoke + 32 api-key tuples),
`CANONICAL_PROVIDERS` (`models_catalog_static.py`, 39 slugs), `plugins/model-providers/*`
(38 plugins), plus curated `_PROVIDER_MODELS` (42 keys). Overlap notes:
`actual` + `alibaba-coding-plan` + `opencode-free` are registry-only; `router`,
`commandcode` (+`commandcode-anthropic`), `deepinfra`, `meta-ai`,
`nebius-token-factory`, `upstage`, `custom` are plugin-only; `alibaba-cn`,
`-coding-plan-cn`, `-token-plan(-cn)` ride the alibaba plugins; `moa` is virtual
(no credential/endpoint).

Full id list (55, every entry per B16):

```text
nous nous-api openai-codex openai-api xai-oauth xai qwen-oauth copilot copilot-acp
gemini vertex zai kimi-coding kimi-coding-cn stepfun anthropic alibaba alibaba-cn
alibaba-coding-plan alibaba-coding-plan-cn alibaba-token-plan alibaba-token-plan-cn
minimax minimax-cn minimax-oauth deepseek nvidia ai-gateway opencode-zen opencode-go
opencode-free kilocode huggingface xiaomi tencent-tokenhub tencent-tokenplan
ollama-cloud lmstudio custom bedrock azure-foundry arcee gmi actual router fireworks
novita nebius-token-factory commandcode commandcode-anthropic deepinfra meta-ai
upstage openrouter moa
```

Auth architecture (**S**): `PROVIDER_REGISTRY` is credential truth; store =
`~/.hermes/auth.json` (per-provider state + credential pool, fcntl-guarded) +
`~/.hermes/.env` keys; profiles = separate Hermes homes under
`~/.hermes/profiles/<name>` (`config.yaml`/`.env`/`auth.json`/`state.db`; no
root-auth inheritance). Models config: curated `_PROVIDER_MODELS` + live `/v1/models`
probes + OpenRouter live catalog (disk-cached, TTL) + `$HERMES_HOME/models_dev_cache.json`;
`config.yaml model:{default|model, provider|auto, base_url, api_mode}` + per-task
auxiliary routing. Usage: local-only session token accounting in state.db; `/usage` =
session/context display; `/usage reset` redeems Codex reset credits (openai-codex
only); Nous Portal billing/top-up/remote-spend via portal APIs. **No generic
per-provider quota API**; per-provider billing scope documented in the
subscription-plans table (cells marked not-documented stay open questions).

### 5.1 Default template (applies to every id above unless a deviation row exists)

| Aspect | Value |
|---|---|
| Auth | `env(<PROVIDER>_API_KEY)` (exact var per provider, see registry). Ev **S**. |
| Launch | `planned` `HER`. Related implemented code: [adapter](../../crates/jackin-core/src/agent/adapters/hermes.rs), [store enumerator](../../crates/jackin-config/src/accounts/stores/hermes.rs), [attribution adapter](../../crates/jackin-usage/src/usage/hermes.rs) (attributes to underlying providers or Nous Portal; no Hermes-native quota API). Proof `not_run`. |
| Usage | `none:` no quota API evidenced (gateway-side metering, cloud-billing consoles, or vendor reporting outside Hermes as applicable). Shown on Usage screen with this reason. Ev **S**. Proof `unsupported`. |

### 5.2 Auth deviations (auth ≠ plain env key)

| Provider | Auth | Ev |
|---|---|---|
| `nous` | `oauth_device_code` (Portal; JWT `inference:invoke`, opaque fallback) | **S** |
| `nous-api` | `env(NOUS_API_KEY)` aggregator path — config comment only, contract **U** ([U4](#9-u-register-exact-missing-proofs)) | **S**+**U** |
| `openai-codex` | `oauth_external` device-code; imports `~/.codex/auth.json`; dead-refresh quarantine | **S** |
| `xai-oauth` | `oauth_external` browser login (SuperGrok/Premium+) | **S** |
| `qwen-oauth` | `oauth_external` PKCE (reuses Qwen CLI login) | **S** |
| `copilot` | copilot token `env(COPILOT_GITHUB_TOKEN,GH_TOKEN,GITHUB_TOKEN)` / `gh auth` | **S** |
| `copilot-acp` | `external_process` (spawns `copilot --acp --stdio`); quota owned by the external process | **S** |
| `vertex` | OAuth2 service-account/ADC; per-request regional base | **S** |
| `bedrock` | `aws_sdk` boto3 chain | **S** |
| `azure-foundry` | wizard endpoint + `env(AZURE_FOUNDRY_API_KEY)` | **S** |
| `minimax-oauth` | `oauth_minimax` browser PKCE (Coding Plan) | **S** |
| `anthropic` | `env(ANTHROPIC_API_KEY,ANTHROPIC_TOKEN)` for API keys; `CLAUDE_CODE_OAUTH_TOKEN` setup-token (prefix-routed) for Claude Max OAuth | **S**+**D** (OAuth token shape per ref-contracts-A) |
| `gemini` | `env(GOOGLE_API_KEY,GEMINI_API_KEY)`; native Gemini client (AI Studio has no key-scoped quota API; Code Assist path is separate OAuth) | **S** |
| `opencode-free` | keyless anonymous (registry-only); `none:` no account exists to meter | **S** |
| `opencode-zen` | `env(OPENCODE_ZEN_API_KEY)`; `x-opencode-session` pinning; `none:` no Zen balance API evidenced — do not invent | **S** |
| `lmstudio` | local `http://127.0.0.1:1234/v1`; opt `LM_API_KEY`; `none:` local server, nothing to meter | **S** |
| `custom` | user `base_url` (+opt key); Ollama/vLLM/llama.cpp/ARK; `none:` arbitrary endpoint, no common quota API | **S** |
| `actual` | `env(ACTUAL_API_KEY)` hosted relay OR loopback keyless (registry-only) | **S** |
| `router` | `env(RAMP_ROUTER_API_KEY,ROUTER_API_KEY)`; codex_responses gateway; `none:` gateway-side metering only | **S** |
| `ai-gateway` | `env(AI_GATEWAY_API_KEY)` Vercel AI Gateway; `none:` gateway-side metering only | **S** |
| `openrouter` | `env(OPENROUTER_API_KEY)` or OAuth PKCE via `hermes auth add` (aggregator, not in auth registry) | **S** |
| `moa` | virtual (no credential/endpoint); launch `blocked`: nothing to stage; `none:` aggregator preset, metered via reference models | **S** |

### 5.3 Usage deviations (usage ≠ none)

| Provider | Usage source | Status | Ev |
|---|---|---|---|
| `nous` | `planned` Portal billing/balance APIs; verify scope ([U5](#9-u-register-exact-missing-proofs)) | `planned` — see [hermes.rs](../../crates/jackin-usage/src/usage/hermes.rs) (Portal-billed attribution) | **S** |
| `openai-codex` | `planned` Codex wham/usage + banked reset credits; prefer app-server | `planned` attribution to [codex.rs](../../crates/jackin-usage/src/usage/codex.rs) ([U11](#9-u-register-exact-missing-proofs)) | **S**+**R** |
| `xai-oauth` | `planned` Grok CLI billing endpoint; weekly/monthly split | `planned` — see native [grok.rs](../../crates/jackin-usage/src/usage/grok.rs) ([U11](#9-u-register-exact-missing-proofs)) | **S**+**R** |
| `copilot` | `planned api.github.com copilot quota` | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **S**+**R** |
| `zai` | `planned api.z.ai quota/limit` + model-usage; plan/CN/team pools | `implemented` [zai.rs](../../crates/jackin-usage/src/usage/zai.rs); attribution wiring `planned` | **S**+**D** |
| `kimi-coding` | `planned api.kimi.com/coding/v1/usages` | `implemented` [kimi.rs](../../crates/jackin-usage/src/usage/kimi.rs); attribution wiring `planned` | **S**+**R** |
| `kimi-coding-cn` | same usages API on CN host | `planned` — verify CN host ([U6](#9-u-register-exact-missing-proofs)) | **S**+**U** |
| `anthropic` | OAuth grants: `planned {base}/api/oauth/usage`; API keys need org reporting (`none:` no key-scoped balance) | `planned` attribution to [claude.rs](../../crates/jackin-usage/src/usage/claude.rs) ([U11](#9-u-register-exact-missing-proofs)) | **S**+**D** (ref-contracts-A) |
| `alibaba-token-plan` | `planned` bailian token-plan API (same family as omp collector) | `planned` ([U11](#9-u-register-exact-missing-proofs)) | **S**+**R** |
| `alibaba-token-plan-cn` | same on CN host | `planned` — verify CN host ([U7](#9-u-register-exact-missing-proofs)) | **S**+**U** |
| `minimax`, `minimax-oauth` | Token Plan remains + PAYG balance endpoints; scope-split; OAuth grant path to verify | `implemented` [minimax.rs](../../crates/jackin-usage/src/usage/minimax.rs) for key path; OAuth path `planned` ([U8](#9-u-register-exact-missing-proofs)); attribution wiring `planned` | **S**+**D** |
| `minimax-cn` | same on CN host | `planned` — verify CN host ([U3](#9-u-register-exact-missing-proofs)) | **S**+**U** |
| `opencode-go` | `GET https://opencode.ai/zen/go/v1/usage` | `implemented` [opencode.rs](../../crates/jackin-usage/src/usage/opencode.rs); attribution wiring `planned` | **S**+**R** |
| `openrouter` | `GET /api/v1/key` key-scoped; `/credits` and delayed `/activity` need a Management key; exact model persisted | `implemented` [openrouter.rs](../../crates/jackin-usage/src/usage/openrouter.rs); management-only enrichment remains unavailable under the current account credential contract ([U12](#9-u-register-exact-missing-proofs)) | **S**+**D** (official key, credits and activity contracts) |
| `openai-api` | `none:` org usage/costs APIs need separate reporting authority; no key-scoped balance | — | **S** |
| `xai` | `none:` xAI API mgmt reporting separate; no key-scoped quota evidenced | — | **S** |
| `deepseek` | `none:` no key-scoped quota API evidenced (balance endpoint unverified here — [U9](#9-u-register-exact-missing-proofs)) | — | **S** |

Proof for all `planned` rows: `not_run`. Proof for `implemented` collectors:
`implemented` (code inspected; lane verification still `not_run` in
[ledger.md](./ledger.md)).

## 6. Codex

Catalog scope: Codex has **no fixed provider catalog**. Scope = 3 reserved built-ins
(`openai`, `ollama`, `lmstudio`) + arbitrary user-defined `model_providers.<id>`
tables. Per B16, every configured id appears in Settings; this ledger specifies the
shape all of them share.

Wire contract (**D**, learn.chatgpt.com `/docs/config-file/config-reference`,
fetched 2026-09-17): `model_providers.<id>` keys: `name, base_url, env_key`
(+`env_key_instructions`), `experimental_bearer_token` (discouraged),
`requires_openai_auth`, **`wire_api`**, `query_params`, `http_headers`,
`env_http_headers`, `request_max_retries` (4), `stream_max_retries` (5),
`stream_idle_timeout_ms` (300000), `supports_websockets`,
`supports_standalone_web_search`, `auth{command,args,timeout_ms,refresh_interval_ms}`
(command-backed Bearer [REDACTED] must not combine with `env_key`/bearer/
`requires_openai_auth`). **`wire_api`: "`responses` is the only supported value,
and it is the default when omitted."** Chat-completions wire deprecated
(openai/codex discussion #7782). Kimi/Z.AI officially support Codex Responses routes
(per [jackin-provider-research.md](../../jackin-provider-research.md)).

| Aspect | Value |
|---|---|
| Auth | `env_key` (Bearer from env), `experimental_bearer_token` (discouraged), `auth.command` (command-backed Bearer [REDACTED]), or `requires_openai_auth` (ChatGPT OAuth). Ev **D**. |
| Launch | `planned` `CX`: every Codex custom-provider launch stages a Responses-speaking `base_url` (+`env_key`) for the selected account only. Chat/Anthropic-native backends need a protocol relay, never a `wire_api` flag. Related implemented code: [adapter](../../crates/jackin-core/src/agent/adapters/codex.rs). Proof `not_run`. |
| Usage | `none:` Codex exposes no per-provider quota API — except the `openai` (ChatGPT subscription) route: `planned` wham/usage + reset-credit inventory via [codex.rs](../../crates/jackin-usage/src/usage/codex.rs) ([U11](#9-u-register-exact-missing-proofs)). Ev **S**+**R**. Proof `not_run` (`implemented` for the collector itself). |

## 7. Claude

Catalog scope: Claude has **no provider catalog and no plugin-based provider
routing**. Scope = Anthropic first-party endpoint + any gateway/proxy reached via
env rewrite. Per B16, every configured gateway endpoint appears in Settings with
its base URL as identity; this ledger specifies the shared shape.

Routing contract (**D**, code.claude.com docs fetched 2026-09-17):

- Plugins (`/docs/en/plugins`) extend skills/agents/hooks/MCP only — **zero
  provider/model-routing surface** (verified by full-text search). No plugin-based
  provider routing exists officially.
- Endpoint/credential (`env-vars` + `model-config` + `llm-gateway*`):
  `ANTHROPIC_BASE_URL` (proxy/gateway; non-first-party host disables MCP tool search
  by default + Remote Control), `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` (Bearer),
  `apiKeyHelper` (+`CLAUDE_CODE_API_KEY_HELPER_TTL_MS`), cloud flags
  (`CLAUDE_CODE_USE_BEDROCK`, `ANTHROPIC_VERTEX_BASE_URL`,
  `ANTHROPIC_BEDROCK_BASE_URL`, Foundry/Agent Platform/Claude-on-AWS). Gateways must
  expose an Anthropic-format endpoint; routing to non-Claude models explicitly
  unsupported.
- Model select: `--model`/`/model`, `ANTHROPIC_MODEL`, `model` setting,
  `ANTHROPIC_DEFAULT_MODEL`, alias pins
  `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU,FABLE}_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL`,
  subagent frontmatter/`modelOverrides`, `availableModels`+`enforceAvailableModels`,
  org defaults; discovery: `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1` populates
  `/model` from gateway `/v1/models`; window fix: `CLAUDE_CODE_MAX_CONTEXT_TOKENS`;
  embedders: `CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST` (host owns routing; user/managed
  keys ignored).

| Aspect | Value |
|---|---|
| Auth | Bearer key (`ANTHROPIC_API_KEY`/`ANTHROPIC_AUTH_TOKEN`), `apiKeyHelper`, Claude Max/Pro OAuth (`CLAUDE_CODE_OAUTH_TOKEN`), or cloud SDK flags. Ev **D**. |
| Launch | `planned` `CC`: per-instance env (`ANTHROPIC_BASE_URL` + credential + `ANTHROPIC_MODEL`/alias pins) + isolated `CLAUDE_CONFIG_DIR`; never a plugin; `/status` verifies (base-URL + auth-token lines). Related implemented code (native route): [adapter](../../crates/jackin-core/src/agent/adapters/claude.rs). Proof `not_run`. |
| Usage | First-party OAuth route: `GET {base}/api/oauth/usage` + `profile` — `implemented` [claude.rs](../../crates/jackin-usage/src/usage/claude.rs), ev **D** (ref-contracts-A). Gateway-rewritten routes: `none:` gateway-side metering only; no common quota API. Proof `implemented` (collector) / `not_run` (lane). |

## 8. Cross-client rules

- Same billing identity across clients must not multiply allowance: e.g. one Z.AI
  key in opencode+`zai`, omp+`zai`, hermes+`zai`, a Codex profile, and a Claude
  wrapper = ONE account, FIVE launch routes, ONE quota source (api.z.ai pools).
  Ledger rows are launch routes, not accounts. Shared-quota dedup key: (service +
  billing subject + scope/model/key); independent key caps never merged (per
  [001-t02-domain-contracts.md](./001-t02-domain-contracts.md) §6).
- OAuth refresh ownership: exactly one writer per grant lineage (opencode Auth, omp
  AuthStorage, hermes auth.json pool, or native CLI); jackin stages copies, never
  dual-refreshes (per domain contracts §7).
- Unsupported-but-visible: every `none:` usage row stays on the Usage screen with
  its exact reason; `U` rows need Mac-live verification before claiming support.

## 9. U-register: exact missing proofs

| Id | Cell(s) | Exact missing proof |
|---|---|---|
| U1 | omp `azure`, `google`, `groq`, `mistral`, `openai` auth | No auth KDL for these ids at pinned SHA `116190d`; missing: vendor login flow or a pinned-source statement that env-only is the complete contract. |
| U2 | omp `minimax-cn` auth | Present in models.json, absent from auth KDLs at pinned SHA; missing: which login kind (if any) owns this id. Launch `blocked` until resolved. |
| U3 | omp `minimax-code-cn`, hermes `minimax-cn` usage | Missing: live verification that the Token Plan remains API answers on the CN host (`api.minimaxi.com`) with the same shape. |
| U4 | hermes `nous-api` auth | Only a config comment references `NOUS_API_KEY`; missing: registry/plugin/code path proving the aggregator contract. |
| U5 | hermes `nous` usage | Missing: Portal billing/balance API paths + scope proof (which calls, which auth, what fields). |
| U6 | hermes `kimi-coding-cn` usage | Missing: live verification of the usages API on the CN host (`api.moonshot.cn`). |
| U7 | hermes `alibaba-token-plan-cn` usage | Missing: live verification of the bailian token-plan API on the CN host. |
| U8 | hermes `minimax-oauth` usage | Missing: proof that the remains API accepts the OAuth grant (vs key-only) and which scopes. |
| U9 | hermes `deepseek` usage | Missing: verify whether a key-scoped balance endpoint exists; currently `none` by absence of evidence. |
| U10 | opencode `opencode-zen` (hermes row) / Zen balance | Missing: any Zen balance/live-model endpoint; do not invent. Currently `none` with reason. |
| U11 | every `planned` **R** usage row (§4.2, §5.3, §6, §7) | Missing: Mac-live verification of each private/reference endpoint (URL + method + auth + response shape) before claiming support; lands in [ledger.md](./ledger.md) provider lanes. |
| U12 | opencode/omp/hermes `openrouter` management enrichment | Ordinary-key `/key` usage is implemented and wired. Missing by contract: a separate Management-key source in `AccountCredential`/usage discovery for `/credits` and completed-day `/activity`; a 403 is a scoped partial result, not an invalid ordinary key. |

## 10. Maintenance

- Re-fetch cadence: re-pin SHAs and the live `models.opencode.ai` snapshot on every
  S-lane that touches catalogs; update §1 dates, §3–§5 scope lists, and this line.
- New provider ids: append to the scope list + either rely on the default template
  or add a deviation row — never silently absorb a known difference into the
  default.
- Closing a U: replace the **U** marker with the new evidence class, cite the proof
  (SHA + path, or fetch date + URL, or ledger.md lane result), delete the register
  row only when no cell references it.
- Link check: every relative link in this file must resolve; re-run the check below
  after edits.
