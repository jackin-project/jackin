# jackin❯ accounts, usage, and multi-account containers

Detailed implementation specification · 17 September 2026

Repository audited: `jackin-project/jackin`, `main` at `5bf20aaf37bbc49325072d199e09effe0678b047`.

This is the proposed product and engineering contract, not a claim that the feature has been implemented or tested on the user's Mac. Read it with `jackin-provider-research.md`, `jackin-implementation-and-verification.md`, and `jackin-goal-prompt.md`.

## 1. Outcome and scope

jackin❯ Console becomes the place to register, select, and monitor all configured AI accounts. Settings owns the account inventory. Usage shows every configured account, its real provider allowances and freshness, and detailed provider-specific information. A workspace can choose defaults, and a launch can select a different subset without changing those defaults. A single container can run two Claude Code sessions with different Anthropic accounts and a Codex session with an OpenAI account concurrently. Each pane and tab retains the actual account and model selection through reconnect and restore.

All thirteen requested entries are covered: Claude Code, Codex, Amp, Antigravity, Kimi Code, Z.AI, Muse Code, Cursor Agent, Grok Build, OpenRouter, omp, Hermes TUI, and OpenCode. Z.AI and OpenRouter are billing/routing providers rather than independent TUI binaries. Their functionality is delivered through explicitly supported clients. Gemini CLI is a separate Google client and discovery source. MiniMax remains in scope because it already exists in jackin❯ and is present in the supplied shell configuration.

The numbered list must not become a hardcoded filter that hides other providers configured inside these clients. During execution, inventory their complete current provider catalogs and every provider entry in the user's registered stores. Support registration and a verified launch mapping for each eligible configured route, including custom compatible endpoints. Add a usage collector wherever a verified authorized source exists; otherwise keep the account visible with the exact unavailable capability. Record a complete catalog-to-support ledger so unimplemented secondary providers cannot disappear behind a generic “all providers supported” claim. Vendor-specific cloud identity/endpoint mechanisms require their own verified adapters rather than an assumed generic API-key route.

Use and extend the existing Rust account configuration, provider adapters, broker, canonical projection, Console, Capsule, and verification harness. Keep provider I/O and secret resolution out of rendering. Update shared protocol/FFI consumers when required for consistency; a new native desktop design or public release is not part of this task.

### User journeys that must work

1. Install and start jackin❯ for the first time. Existing supported agent logins are registered automatically and are visible in Settings and Usage. An empty or damaged source does not stop the other discoveries.
2. Add another login on the host later. Settings → Accounts → Scan for new accounts finds it, explains the result, and adds the selected new source without overwriting existing account choices.
3. Add a subscription profile from a custom folder, an API key, an environment reference, or a 1Password reference. Configure a provider-compatible client and model.
4. Open Usage. Current inventory appears immediately, last-known values carry their timestamps, and due provider refreshes run asynchronously. Leave the screen open and see periodic updates.
5. Open an account detail view and see every supported limit, reset, per-model allowance, balance, credit pool, or other meaningful usage field exposed by that account's available API/CLI capabilities.
6. Save global launch defaults; override them for a workspace; override the workspace selection for one start. Each choice has predictable precedence.
7. Launch one container with exactly Claude account A, Claude account B, and Codex account C. Start all three TUIs. See their account labels in tabs. No credentials from unselected account D enter that container.
8. Use one Kimi subscription through Kimi Code, Claude Code, or supported Codex configuration without creating three fictitious independent quotas.
9. Use OpenRouter through a supported client with an exact saved model ID; a changed model does not change the selected billing account silently.

## 2. What the audit establishes

The repository already contains named accounts, profile/API-key credential variants, environment and 1Password references, a first-create discovery path, Settings account forms, workspace authorization/default bindings, a Console Usage route, host usage discovery, eight host usage surfaces, a durable broker, a canonical projection, a Capsule usage dialog, a native Rust/Swift bridge, and extensive test infrastructure.

Two structural gaps define the implementation:

- `crates/jackin/src/console/adapter/run.rs` reads current broker state at Console startup. `u` in `crates/jackin-console/src/tui/input/list.rs` opens a copy of those rows. Manual `r` uses a synchronous per-account refresh/join loop. The route is not yet the nonblocking, periodic monitor requested here.
- `AgentCredentialEnv` in `crates/jackin-protocol/src/account_credentials.rs` is keyed by agent name. Existing account bindings and launch staging resolve one account per `Agent`. This cannot correctly express two independently authenticated instances of the same agent in one container.

The Console presentation also drops stable account IDs and much of the canonical detail/freshness information when converting to `UsageAccount`/`UsageWindow`. Selection is positional. These are concrete extensions to make, not reasons to introduce a second usage backend.

The existing broker's process-independent generations, shared retry deadlines, last-good state, capability-scoped relay, and adaptive cadence are valuable foundations. Preserve them and prove the new routes use them.

Source anchors: [Console adapter](https://github.com/jackin-project/jackin/blob/5bf20aaf37bbc49325072d199e09effe0678b047/crates/jackin/src/console/adapter/run.rs), [Console Usage](https://github.com/jackin-project/jackin/blob/5bf20aaf37bbc49325072d199e09effe0678b047/crates/jackin-console/src/tui/screens/usage.rs), [credential transport](https://github.com/jackin-project/jackin/blob/5bf20aaf37bbc49325072d199e09effe0678b047/crates/jackin-protocol/src/account_credentials.rs), [broker contract](https://github.com/jackin-project/jackin/blob/5bf20aaf37bbc49325072d199e09effe0678b047/crates/jackin-protocol/src/usage_broker.rs).

## 3. Domain model and invariants

The following names describe required concepts; choose final Rust type/module names after examining the implementation branch. Do not create a parallel abstraction for an existing type that can be evolved cleanly.

| Concept | Meaning | Required identity and fields |
|---|---|---|
| Agent | Executable client/TUI | Stable `AgentId`; executable, version detector, install source, host/target architecture support, config/auth/state layout, supported launch modes |
| Provider service | Billing and API product | Stable `ProviderServiceId`; provider, product, region/endpoint, auth modes, protocols, usage capabilities; e.g. Kimi Code and Moonshot PAYG are distinct services |
| Registered account | Operator-managed credential/account entry | Opaque stable `AccountId`; label, service, enabled state, credential references, optional verified billing identity, discovery provenance |
| Credential source | Where/how a credential is resolved | Source ID, kind, exact registered path/profile/keychain locator or secret reference, ownership policy, revision, validation status; no plaintext secret in UI DTOs |
| Billing identity | Provider-established person/organization/workspace scope | Provider subject, tenant/org/project/workspace where applicable; separate from local display label and credential ID |
| Quota scope | The allowance actually measured | Stable service + billing subject + scope/model/key/product identity; used for de-duplicating shared observations without merging independent key caps |
| Agent configuration | How a client uses an account | Stable configuration ID; agent, account, protocol/endpoint, exact model or verified auto mode, optional provider/model routing options |
| Workspace policy | What the workspace can launch by default/on demand | Allowed accounts/configurations, default launch set, optional inherited defaults; explicit policy distinct from defaults |
| Launch manifest | Selection immutable within a revision for one container | Launch revision, selected account/configuration IDs, permitted capabilities, per-instance staging references, image/version provenance |
| Agent instance | A particular running client in that manifest | Stable `AgentInstanceId`; agent + account configuration + model; pane/session IDs; independent runtime home/state |
| Usage snapshot | Observation of an account/quota scope | Source, observed time, freshness, generation, identity evidence, typed metrics/windows, partial errors |

### Identity rules

- Generate stable registration IDs independent of display names, filesystem ordering, raw secrets, token rotations, and mutable email addresses.
- Renaming an account does not change workspace bindings, selected rows, history associations, or running instances.
- Two providers sharing an email are different accounts. The same person in two organizations/workspaces may have different allowances. Two API keys in one organization may have separate key caps.
- Duplicate references to the same credential source are idempotent. Multiple credential sources with verified identical billing identity can share a canonical usage observation, but preserve distinct execution configurations and key-specific caps.
- Until identity is verified, retain a registered account as unresolved rather than discarding it, manufacturing a provider ID, or merging it by label.
- A source that changes to another logged-in identity is an account change, not a silent update of the prior account's quota. Invalidate stale association and require an explicit binding/reconciliation decision where necessary.
- A canonical quota cannot be added twice to a total merely because Claude Code, Codex, and Kimi Code all access it. The UI can identify shared allowances. Never sum unrelated subscription percentages.
- Register all actual API keys discovered in a multi-provider auth store. Do not stop at the first provider or collapse keys solely because the variable name is an alias.
- Distinguish an account with valid inference credentials but no usage permission from an invalid account.

### Capability dimensions

Represent support per service/auth mode and installed client version:

`discover`, `validate_identity`, `launch_interactive`, `read_limits`, `read_balance`, `read_token_totals`, `read_model_limits`, `refresh_credentials`, `isolate_multiple_accounts`.

Each dimension is independently supported, unsupported, needs additional permission, temporarily unavailable, or not yet verified. A provider can launch successfully and legitimately have no published quota API. "Unsupported" is a runtime truth for a specific feature, not permission to skip an available integration.

## 4. Settings → Accounts

### Inventory

Show a stable, keyboard-navigable list grouped by provider service, with search/filter when it no longer fits. Each row includes account label, provider/product, credential kind, validation state, supported clients, default assignments, and last successful usage update. Paths are available in the Settings detail where they help the operator identify the source; keep raw paths out of public usage/protocol messages and diagnostics.

Required actions: Add account; Scan for new accounts; Edit; Rename; Enable/disable; Validate/reconnect; Set defaults; View usage; Remove registration. Removal removes jackin❯'s reference and flags affected workspace/default configurations; it must not delete a host agent login directory or log the user out of a provider.

Scan results enter the current Settings draft as candidates. Selecting candidates and Apply/Save commits through the existing configuration lock and atomic-write path; cancelling before Apply leaves registration unchanged. Merge additions by stable source identity without replacing unsaved labels/defaults/source edits. A concurrent conflicting edit receives a targeted conflict or a safe re-merge, never last-writer-wins data loss. CLI scan may commit directly through the same importer because invoking it is the explicit import operation.

Disable/removal denies new launches and new host secret/usage grants for that registration. Already running instances keep their recorded labels and show that the registration was disabled or removed. Credentials already materialized inside an active TUI cannot be revoked merely by deleting jackin❯ metadata. Present an explicit stop/recreate action when the user wants to remove that access; provider-side revocation is a separate account operation. Do not silently kill useful work or claim upstream revocation occurred.

### Add/edit form

Required fields depend on the selected service and credential kind:

- Provider service and auth mode: subscription OAuth/native login, API key, supported access token, or client-managed multi-provider credential store.
- Account label and optional organization/project/workspace/region.
- Credential source: default discovered profile, explicit custom directory/profile, OS keychain reference, environment variable reference, existing 1Password item reference, or entered key saved through the supported protected store.
- Compatible agent configurations; endpoint/protocol where configurable; model selector and exact model ID.
- Optional usage/billing credential with explicitly separate permission and identity scope. This credential stays host-side and is never part of a launch credential bundle.
- Ownership: reference external agent-managed credentials, or use an explicitly jackin❯-managed login. Clearly explain which component refreshes the login.

Validate locally first. Test authentication/usage asynchronously and show separate results. A quota read failure must not erase a working account. A 1Password lock is `Needs secret access`, not `Invalid API key`. Storing a reference does not execute it during a filesystem scan.

### OpenRouter models

OpenRouter is an account/service configuration used by supported agents such as OpenCode, omp, or Hermes. Save the exact model identifier in the agent configuration, not just a friendly label. Fetch current models when requested, cache metadata, allow an explicit custom model ID for newly available models, and validate compatibility before start. Preserve suffixes, routing options, and provider preference only where the receiving client supports them. Do not substitute the model silently. An explicit `auto` mode is permitted only if the user selects it and the client/service supports it; the requested specific-model flow always pins an exact ID.

Absence from a stale/offline catalog is an unverified model, not proof of removal. Preserve a literal custom ID and distinguish discovery failure from an authoritative incompatible/removed-model response.

The same account can have multiple named configurations with different models. Those configurations share billing/account identity, not independent account balances.

## 5. Discovery and first initialization

### Bootstrap state machine

Use one explicit, versioned initialization/discovery state rather than only testing whether `config.toml` exists. At first eligible startup:

1. Load/validate configuration and reconcile the current schema.
2. Start bounded discovery in a worker. Render a usable Console with discovery status immediately.
3. Inspect catalog-known default paths and recognized existing jackin❯ credential references.
4. Create registrations for evidenced credentials; persist them transactionally with stable IDs and provenance. Do not require online success to record an existing login source.
5. Preserve per-source issues and show a concise summary: added, already registered, needs authentication, unreadable, or unsupported layout.
6. Mark initialization complete only after the transaction. An interrupted startup retries idempotently.
7. Later startups revalidate registered sources without automatically re-adding removed accounts or recursively discovering unrelated folders.

Existing installations predate this sentinel. Migrate a pre-existing configuration as initialized unless an explicit fresh-install marker establishes otherwise; do not infer first use from an empty accounts map or a missing new field. A freshly installed application that pre-creates its configuration must persist a fresh-install marker. An older ambiguous empty configuration remains initialized and can use Settings Scan. Test this transition separately from a genuine clean install so previously removed accounts stay removed. Concurrent bootstrap/scan processes must recheck source identity under the configuration lock before committing.

If exactly one eligible native account configuration exists for a selected/default client, it can be the deterministic fast-start default. With several eligible choices, require an explicit default selection or a launch picker; do not choose by directory iteration, provider remaining balance, or secret availability.

### Manual scan

Settings Scan for new accounts runs the same catalog scanners. Show candidates and allow selecting what to add; retain existing labels/defaults. Registered sources can be reported as unchanged or changed identity. A successful scan containing zero new accounts is a normal result. Scan must not alter host credentials or start paid inference.

Optionally include known sibling account folders matching narrowly defined patterns such as `.claude-*`, `.codex-*`, and registered Amp XDG roots. These are candidates that still need format/evidence validation. Do not scan the entire home tree. Custom arbitrary folders remain addable through the form.

### Path families and multi-provider stores

The provider research document defines the verified path/version matrix. Important distinctions to implement:

- Claude profile directories and macOS keychain-backed logins; `CLAUDE_CONFIG_DIR`; separately scoped Anthropic Console/API profiles where supported.
- Codex `CODEX_HOME` and selected credential-store mode; file and keychain layouts are not interchangeable.
- Amp's XDG data/config/cache roots; a single `.amp` assumption is insufficient.
- Antigravity CLI and Gemini CLI have separate auth stores and eligibility. `.gemini` is not proof of Antigravity auth.
- Kimi new `.kimi-code` and older `.kimi` families have version-specific layouts; migration-source scanning does not imply running old binaries.
- OpenCode uses XDG data storage for auth. Merely setting `OPENCODE_CONFIG_DIR` does not isolate accounts.
- omp and Hermes can contain credentials for multiple providers. A scanner returns a collection of account candidates/configurations, not one account named after the client.
- Muse/Cursor/Grok locations must follow verified installed version behavior, not guessed directories.

Limit file sizes, record schema/version, follow only permitted symlink targets, and handle unreadable or malformed files per source. Treat a folder with no recognizable credential as a configuration candidate, not an authenticated account. Runtime keychain access can prompt through the OS; discovery must not repeatedly unlock stores to enumerate usage.

### Supplied `.zshrc` reference

The attachment establishes these user workflows:

| Workflow | Required representation |
|---|---|
| `.claude-scentbird`, `.claude-scentbird-ai`, `.claude-chainargos` | Separate named Claude profile sources using `CLAUDE_CONFIG_DIR` |
| `.codex-scentbird`, `.codex-chainargos`, `.codex-chainargos2` | Separate named Codex profile sources using `CODEX_HOME` |
| `.amp-scentbird/{data,config,cache}` | One named Amp source with three explicit XDG roots |
| Claude routed to Z.AI, Kimi, MiniMax | Agent configuration references a provider account, endpoint/protocol, and model mapping |
| Codex profiles for Kimi and Z.AI | Selected profile/model/endpoint and per-process API-key reference |
| 1Password per invocation | Keep a reference; resolve on authorized validation or launch; no global key export |
| Several normal/YOLO wrappers for one source | One account/configuration with an execution option, not duplicate accounts |

Do not copy the private 1Password locators into repository docs, fixtures, screenshots, or test output. Do not `source`, `eval`, or execute `.zshrc` while importing it. A static importer may recognize literal path/variable/profile/provider mappings and show ambiguous dynamic expressions as unresolved. Better to require selecting a custom source than to execute an arbitrary shell function. The pasted Markdown is evidence of intent, not executable configuration or proof that every alias/model still works.

This prohibition includes indirectly running login/interactive shells to capture environment variables. Read the current process environment directly. Passive scanners must not invoke helpers that create credentials, device IDs, caches or config files. Validate read-only discovery with source-tree byte comparisons and a shell-startup canary.

## 6. Usage data contract

### Metric semantics

Every observation includes an account/configuration link, quota scope, source kind and version, observation time, freshness state, and errors scoped to the affected metric. Extend the current canonical projection and `FocusedUsageView` through one migration if their shape cannot represent the required details.

Independently fetched metric groups each carry source identity, scope, provider observation time, transport completion time, last success, freshness and availability. Fresh main quota does not make retained old balance/history fresh. Optional enrichment from a different account, organization, region or project is rejected without damaging valid primary usage.

| Metric class | Required fields/behavior |
|---|---|
| Allowance window | Stable ID, semantic period, duration when supplied, limit/used/remaining when known, percentage representation, unit, reset time/meaning, model/pool scope |
| Balance | Currency or provider credit unit, prepaid/remaining value, expiry if supplied, scope; no percentage without a meaningful denominator |
| Spending cap | Cap, spend, remaining amount, billing period/reset; distinguish key cap, account cap, organization cap, and subscription extra-usage cap |
| Token totals | Input/output/cached/reasoning totals if supplied and relevant; measurement interval/source; never a claim of remaining allowance |
| Per-model/pool detail | Exact model/pool identifier, own windows/caps, shared allowance relationship, source eligibility |
| Rate limit | Requests or tokens per minute/day, remaining/reset when supplied; separate from prepaid/subscription balance |
| Plan/account metadata | Provider-returned plan, tenant, tier, auth status; don't infer solely from a filename or key prefix |

Typed period variants must distinguish rolling duration, fixed calendar period, provider-defined period, and unknown. Store absolute timestamps in UTC; display reset countdown plus exact local time, with timezone available. Do not call a quota reset "session expiration": OAuth expiry, subscription renewal, model context exhaustion, and usage-window reset are different events.

Use checked numeric types/decimal amounts as appropriate. Preserve precision for money and provider credits. Format percentages consistently in Rust and keep raw values for calculation. For ratio-derived percentages, only divide compatible quantities with a valid nonzero denominator. A provider's `used=120%, limit=100%` can display overage text with bar geometry capped at 100%; it must not wrap, underflow, or turn negative remaining into fabricated credit.

Unknown, not applicable, not started, no permission, unavailable, and exhausted are separate states:

- Unknown limit → display its actual availability reason: unverified identity, missing permission, source failure, unsupported schema, or confirmed unpublished limit. Reserve `Limit not published` for an established provider limitation; never draw an empty 0% bar for an unknown value.
- Inference key cannot read account balance → `Balance requires billing access`, while key-scoped metrics continue to work.
- Last-good data retained after failure → values plus `Updated … · stale` and a recovery message.
- Reset time passed → `Refresh due` until the provider reports the new allowance; do not synthesize a full refill.
- A provider explicitly reports a period not started → `Not started`; missing data alone does not establish this.
- Zero remaining from a valid authoritative response → exhausted, still distinct from authentication errors.

### Overview selection

For every account, show the short/session window when available and all applicable principal long-period windows (daily, weekly, monthly). Weekly and monthly may both apply. Include independent model/pool principal windows where hiding them would be misleading. Providers with balances only get balance rows rather than fake session/weekly cards.

The user requests percentage used. Default the new Console to an unambiguous `x% used` label and matching bar fill; offer used/remaining preference through the shared Rust formatting model. Existing surfaces may retain their explicit preference, but the same observation and preference must produce the same value everywhere. Do not drop `used_percent` when mapping the canonical DTO.

Show per-account rows rather than a global percentage. Aggregate only compatible monetary values with explicit same currency, same scope, and no overlapping/shared credit pools; such aggregation is optional and should not delay the required individual monitoring flow.

### Provider detail layout

1. Account/service identity, selected configuration context where relevant, plan and overall status.
2. Primary windows: session/rolling, daily, weekly, monthly as actually available.
3. Model/pool limits and shared-pool explanations.
4. Included/prepaid/extra-usage balances and caps; credit expiries when exposed.
5. Other supported usage details such as input/output/cached token totals and provider usage counters, with source period labels.
6. Freshness, last success, current refresh state, retry deadline, and a concise recoverable error.
7. Actions: refresh account, open Settings account, open the fixed provider usage destination, reconnect where needed.

Render meaningful typed fields rather than arbitrary response JSON. Provider response bodies may contain identifiers or secrets; they must not become a "details" dump.

For local token/history detail, require independently established event ownership and source coverage. Today's login or current model cannot establish who paid for old events. Unattributed records remain unattributed or excluded from account totals; parent/fork/subagent/copied records must be deduplicated. When multiple accounts become possible, invalidate/reclassify cached ownerless totals before the first cached frame, even if network refresh fails. Remote account-wide totals and local observed events are overlapping evidence, not additive independent consumption.

## 7. Refresh architecture

Keep one host broker as owner of provider usage reads, shared state, scheduling, retries, and publication. Console, CLI, Capsule relay, and native desktop consume its canonical observations. No renderer launches a subprocess or makes HTTP calls.

### Screen lifecycle

- On open: load the registered inventory and cached projection promptly; subscribe to updates; request due observations for every configured enabled account, independently of viewport position or the selected detail. An empty cache must still show account rows with loading states.
- Opening a recently refreshed screen can reuse a still-fresh observation. The screen-open path must request freshness through the broker, not force duplicate requests.
- While open: use broker scheduling. Default active cadence can retain the existing 2-minute direct-interaction interval, with 5/15/30-minute recent/idle/long-idle tiers, but a visible monitoring screen must have a documented heartbeat policy and must continue updating without key presses. User settings may choose a supported cadence; provider minimum intervals and retry deadlines always win.
- On return from sleep or network reconnection: recalculate due times and refresh with jitter; no burst of missed polls.
- On account detail: request that account if due and continue overview monitoring through the same schedule.
- Manual refresh: request immediately if allowed, join active work, and respect Retry-After/shared cooldown. Repeated `r` cannot create a queue of duplicate calls.
- On close: unsubscribe promptly. Do not cancel a provider generation another client owns or is awaiting. Broker lifecycle controls background work.

Use bounded concurrency across independent accounts and provider-specific limits. One slow provider must not block rendering or healthy accounts. Support incremental per-account publication with a consistent catalog revision; ignore late results from removed accounts, old credential revisions, or older generations.

Reuse shared backoff and last-good persistence. Credential refresh requires its own serialized ownership, not merely usage-read single-flight. A usage read and an agent process can otherwise rotate the same OAuth token concurrently.

### Timeouts and process probes

Give each adapter explicit connection/read/overall timeouts, maximum response size, cancellation, and child-process cleanup. Retain the normal broker deadline for HTTP work; an officially supported CLI usage operation that needs a different bounded budget must declare it and run off the UI thread. Do not silently let a global 30-second policy invalidate an adapter's documented longer budget. Test the chosen budgets and failures.

No usage poll may send a model prompt, buy credits, redeem a reset credit, or change a subscription. Diagnostic CLI invocations must use verified read-only commands and a source-bound account environment.

## 8. Console interaction and visual contract

Reuse jackin❯'s shipped frame, focus semantics, termrock components, colors, meter formatting, and footer conventions. Evolve the existing Usage route rather than adding a second navigation destination.

Wide layout: provider/account navigation on the left; overview or selected-account details on the right. The overview presents each account's primary windows together. Narrow layout: one focused pane with a clear back path; show essential labels, percentages, and reset text without horizontal overflow. Account labels must disambiguate duplicate names with a safe suffix.

Required states: first load; refresh with last-good values; all healthy; mixed success; all failed; empty registry; no enabled accounts; no published quota; missing credential; locked secret store; unsupported installed version; selected account removed; disconnected/offline; successful scan with no additions.

Required behaviors:

- Store selection by stable account ID. Reorder/rename/refresh does not jump to a different account. If the selected account disappears, return to Overview and show a persistent inline notice.
- Independent list/detail scrolling and visible focus. Selection remains visible as the list grows.
- Arrow keys and existing Vim-style equivalents; Enter for details; Escape/back; Tab for pane focus; `r` for refresh; visible Settings/Accounts action. Resolve exact key bindings with existing Console grammar and test conflicts.
- Footer includes applicable actions plus updating/last-update state. Do not label all providers available when only some succeeded.
- Provide text equivalents for colors and meters. Test long names, Unicode, narrow terminals, resize, and no-color output where supported.
- Display safe account labels, not private account IDs, raw keys, or long opaque capability strings as user-facing names.
- Preserve a custom tab name, but still make the actual agent/account/model visible in pane/status details. A rename cannot hide or change credential binding.

## 9. Workspace and launch policy

### Separate permissions from defaults

An allowed set defines which accounts/configurations a workspace may use. A default launch set defines which will be selected on a fast start. An explicit launch selection defines what actually enters one container. A default never grants permission outside the allowed set.

Selection precedence, highest first:

1. Explicit one-launch selection of account configurations/instances.
2. Workspace-role default launch set, if this existing scope is retained.
3. Workspace default launch set.
4. Global default launch set for the chosen role/client set.
5. An unambiguous sole eligible configuration; otherwise a picker.

This precedence must be implemented once and reused by CLI, Console, reconnect/new-session flows, and programmatic launch. Decide whether a scope replaces or inherits defaults explicitly; do not union account sets accidentally. An explicit empty selection is different from missing configuration. Explicitly selecting an unavailable account returns an actionable error; no silent fallback to a different account or to ambient auth.

Use replacement semantics for an explicitly configured scope. Filter inherited global candidates by workspace authorization before resolving them. Explicit workspace/role defaults must validate against their allowed set when saved; explicit per-launch selections fail atomically if any requested entry is invalid. If filtering leaves no eligible configuration, use the existing interactive selection/setup flow, or return a clear no-eligible-account error in noninteractive mode. Do not silently start an agent with ambient auth. An explicit empty set can request shell-only only through an existing explicit shell mode; otherwise reject it for an agent launch. A new tab validates against the existing container manifest, rather than recomputing admission from today's global defaults.

For a new container, forward only accounts required by the resolved selected instance configurations. Multiple named configurations referencing the same source can share a staged account capability while keeping distinct model settings. A selected but not-yet-started permitted account may be available for later tabs if included in the reviewed launch manifest.

For an existing container, adding a tab can choose only from its immutable permitted launch set. Adding another account requires an explicit capability/staging update with revision checking, or a new container if in-place updates cannot be made safely. A workspace/global settings edit must not silently expose new credentials to a running container.

Fingerprint the actual admitted bindings and relevant authorization/credential revisions. Adding or editing unrelated account D must not invalidate the container admitted with A/B/C, rotate its capabilities, or force a new container merely to add another permitted A/B/C session.

### Concrete required example

Illustrative logical configuration, not promised current TOML syntax:

```toml
[[agent_configurations]]
id = "claude-work"
agent = "claude"
account = "anthropic-work"

[[agent_configurations]]
id = "claude-personal"
agent = "claude"
account = "anthropic-personal"

[[agent_configurations]]
id = "codex-work"
agent = "codex"
account = "openai-work"

[workspaces.example]
allowed_accounts = ["anthropic-work", "anthropic-personal", "openai-work"]
default_launch = ["claude-work", "claude-personal", "codex-work"]
```

An implementation must support this behavior even if its final schema uses different names. Add parser/validation/documentation tests for the actual schema and remove obsolete one-account-per-agent resolution paths.

## 10. Container and session contract

### Launch isolation

Each agent instance gets an exact selected account configuration and private mutable runtime state. Use client-native config roots where supported (`CODEX_HOME`, `CLAUDE_CONFIG_DIR`, XDG roots, version-specific equivalent). If a client has no sufficient supported override, use a per-instance HOME and a deliberately constructed environment/config overlay. Preserve normal workspace/tool behavior; do not point every process at the same account home.

A per-instance HOME is sufficient only when the client actually scopes every required auth store to it. Antigravity's singleton native keyring is a counterexample: it additionally requires a proven account-scoped credential namespace/transport, such as private keyring contexts or a supported native mechanism. Test two accounts concurrently; do not promote folder separation into an auth-isolation claim.

Staging must:

- Materialize only required auth/config fragments and selected provider entries, not the entire host home, browser profile, multi-account SQLite store, or multi-provider auth JSON.
- Separate read-only source material from mutable sessions/history/caches. Do not mount the whole source login directory writable into several clients and expect token rotation to be safe.
- Preserve selected profile/model/protocol fields through agent-native config serialization. Never implement config changes with string replacement of arbitrary shell snippets.
- Remove inherited provider/auth/home variables before applying the selected account environment. Scrub credentials for every supported provider, not only the chosen agent's default provider.
- Keep secrets out of Docker command arguments, image layers, Docker labels, public Capsule config, logs, telemetry, and snapshots. Use jackin❯'s protected transport/staging boundary and test it end to end.
- Keep usage-only billing/admin credentials on the host. The container relay receives only scoped usage capabilities for launch-authorized accounts.
- Treat custom endpoints as part of credential scope. Do not forward credentials on arbitrary cross-origin redirects or silently retry a failed endpoint on another provider host.

### Trust boundary

The user's requested shared container is one trust boundary: processes with the same user/root privileges may be able to inspect other selected account credentials in that container. Per-process environments and directories prevent accidental account confusion; they do not create a hard adversarial boundary between selected agents. jackin❯ must guarantee that unselected accounts are absent and unauthorized, and must not claim stronger isolation than the container/process permissions actually provide. Separate containers remain the way to request stronger separation between selected accounts.

### OAuth ownership

Define ownership for each credential source and client before enabling multi-account execution:

- External/native source: discovery references it without taking over ownership. Prefer the native CLI/auth service for refresh if that is its supported contract.
- jackin❯-managed source: broker/credential service owns rotation with per-source lock, revision compare-and-swap, atomic writes, and identity validation.
- Coordinate rotating credentials by verified OAuth grant/credential lineage, not registration ID or directory alone. Two copied paths can hold the same grant. Keep any secret-derived correlation internal and protected, never a public account ID, and do not merge independent grants solely because they share a billing identity.
- Where a client insists on rotating refresh tokens itself, choose a supported external-token interface, a separately authenticated managed profile, or a proven serialized ownership mechanism. Blind copying and later writing refresh tokens back is not an acceptable generic solution.
- Multiple sessions sharing one account must be tested against the client's actual token lifecycle. Auth expiry/revocation is not a reason to switch billing identity silently.

### Instance and tab identity

Evolve public non-secret launch/session records and protected credential transport to be keyed by `AgentInstanceId` or explicit account-configuration binding, not `Agent` alone. The new-session request, split pane, PTY spawn, session restore, attach, and usage lookup all carry that binding.

Labels should read like `Claude · Work`, `Claude · Personal`, and `Codex · Work`; when the provider differs from the native agent, include it, for example `Claude · Z.AI · Work`. Show model in pane details and in compact tab labels where needed to disambiguate two configurations using the same agent/account. Labels derive from saved instance identity, never terminal title escapes or whichever credential was last loaded.

The container usage dialog exposes all selected authorized accounts, including accounts for clients not yet started, without accessing the global host catalog. Focusing an agent instance selects its account detail. Removing or changing a registration on the host invalidates/updates access according to explicit lifecycle policy, without applying another account's cached usage to it.

## 11. Provider implementation rules

Follow the evidence and capability matrix in the research companion. Prefer documented usage APIs or supported machine-readable CLI operations. Use source-derived private endpoints only as explicit, versioned adapters with fixtures, bounded errors, and visible fallback state. An endpoint present in CodexBar/OpenUsage is evidence of a working integration approach, not a vendor stability guarantee.

Fallback can use only source modes explicitly permitted and already proven to belong to the same account/billing scope. A missing custom Codex home cannot borrow the global Codex/OpenCode/omp login; a custom Claude profile cannot borrow the default keychain entry. Failure never authorizes changing provider product or region. Muse's internal key-exchange route is conditional until passive-read behavior is proven; cached official MSP observations remain valid evidence with their actual age.

Preserve provider-owned period duration, reset timestamps, units, pool identity, and scope. Never invent an equivalent number of tokens from dollars, a percentage, requests, or hours. Pricing estimates from local logs are not remaining subscription quota.

Verify the eligible agent-provider pair separately from wire compatibility. A subscription that permits particular clients is not automatically authorized for every client speaking the same protocol. Build positive compatibility entries and explain rejected combinations.

New runtime support includes the actual interactive TUI launch, installer/image integration, executable/version checks, terminal behavior, shutdown and cleanup, and account isolation. A row in an enum, a `--help` success, or a mock usage response is not complete client support.

## 12. Migration and repository alignment

Read current `AGENTS.md`, `RULES.md`, `ENGINEERING.md`, `TESTING.md`, and affected crate documentation. The audited repository requires migrations to finish and obsolete paths to be removed. Deliver one canonical schema and resolver; do not preserve old one-account-per-agent execution as a hidden alternative.

A deterministic, one-time importer of existing user configuration is acceptable as a data transition. It must preserve registered account IDs/references/default intent, validate the result, and write atomically. It is not permission to keep two runtime schemas indefinitely. If the repository policy requires explicit schema rejection instead, provide an actionable conversion command and fixtures before removing the old path; do not discard real login state.

Reconcile the existing `plans/unified-agent-usage` and related roadmap/research material. Older fixed provider counts, provider-only labels that obscure requested instance identity, and old branch/PR invocation instructions are historical context rather than the scope of this new task. Preserve valid broker/capability/testing invariants. Do not resume a historical PR or publish a desktop release merely because an old plan mentions one.

Update protocol version/build handshake and generated Swift bindings as needed. Old running clients receive an explicit incompatible-version/restart outcome. Do not silently deserialize a materially different credential envelope as the old schema.

## 13. Measurable acceptance targets

These are proposed targets to measure locally, not claimed baseline results. Record machine, build mode, account count, cache state, and test command with every measurement.

| Scenario | Target |
|---|---|
| Open Usage with 50 registered accounts and cached data | Account inventory and first frame within 250 ms on the target Mac; provider work off the render thread |
| Key input/scroll during slow refresh | p95 response below 100 ms; no synchronous per-account waits in the UI loop |
| One account stalls for its maximum budget | Healthy accounts update as they complete; escape/navigation remain responsive |
| 20 clients request the same account observation | One provider generation/call under the broker's proven single-flight contract |
| Different accounts from the same provider | Concurrent within adapter policy; no shared credential/global-env mutation |
| 50 accounts × several windows | Stable selection/order and bounded memory/response size; no all-history scan on every tick |
| Repeated scans and restart after interrupted bootstrap | Stable registrations; zero duplicate account rows or overwritten defaults |
| Three-account shared container | Correct identity/model per TUI, independent state, unselected credential canary absent |
| First run without network/secret-store access | Console remains usable; registrations/issues are explicit; later recovery works |

Every target needs a real test/measurement or an explicitly recorded failure. Fixture tests and Linux containers do not substitute for the required real macOS/OrbStack and authenticated provider verification described in the execution companion.
