# jackin❯ implementation plan and verification checklist

17 September 2026 · Baseline `5bf20aaf37bbc49325072d199e09effe0678b047`

This execution plan implements `jackin-accounts-usage-specification.md`. Provider-specific evidence and constraints are in `jackin-provider-research.md`. All checkboxes below are acceptance work for the implementation agent; none indicate tests already run on the user's computer.

## 1. Execution rules and completion evidence

Use one orchestrator and independent analysis, implementation, and verification agents. Keep each assignment bounded by a task and owned files; run unrelated tasks concurrently. Contract changes need one owner and review before dependent agents code against them. Agents should use isolated worktrees only when necessary; do not overwrite another agent's uncommitted changes. Preserve the user's current branch/PR scope. If this is a fresh checkout, establish an isolated working branch after inspecting current repository rules. Historical instructions in `plans/unified-agent-usage` about a different PR are not this task's branch authorization.

Before implementation, record current HEAD, dirty files/worktrees, current repository policy, installed host tools, macOS/architecture, Docker/OrbStack state, and relevant provider CLI versions. Compare current source with the audited baseline. Reuse improvements already landed since this report and revise task ownership accordingly.

Judge work by correctness and whether it fulfills this contract. Do not leave known broken paths because they seem marginal or expensive. Do not expand this into unrelated CI migration, global dependency updates, public release, or a new native app design.

Each implementation task must produce changed implementation, associated behavioral tests, documentation/schema updates where required, exact validation command and result, unresolved facts, and a reviewer decision. Research/review tasks produce evidence and decisions without unnecessary code edits. Store durable specifications and sanitized fixtures in the repository. Keep raw credentials, copied user auth files, live logs, and private-account screenshots out of commits; intentionally reviewed synthetic render baselines are appropriate repository fixtures. Use the repository's approved artifact/evidence process for execution reports; this prompt does not authorize posting messages or comments externally.

Status vocabulary: `not_started`, `in_progress`, `implemented`, `fixture_verified`, `container_verified`, `live_verified`, `blocked`, `failed`. Use separate columns for these proof levels. A checked planning document or a successful mock is not a live provider verification.

## 2. Work graph

| Task | Scope/owner | Dependencies | Concrete output and exit evidence |
|---|---|---|---|
| T00 | Orchestrator: baseline and policy | None | Current SHA/tool versions, delta audit, requirement inventory, commands and test discovery; no historical-plan assumptions |
| T01 | Provider researcher: capability manifest | T00 | All 13 requested entries plus Gemini/MiniMax, full current upstream provider catalogs and actual configured entries mapped to clients/services; path/auth/usage/version/source and unknowns; exhaustive coverage matrix |
| T02 | Domain owner: account/instance/usage contracts | T00, initial T01 | One reviewed schema for registrations, sources, configurations, admitted accounts, initial sessions, instance IDs, quota scopes, and typed observations |
| T03 | Config owner: registry evolution/import | T02 | Atomic schema transition, stable IDs, safe secret references, existing data/defaults retained; no dual runtime resolver |
| T04 | Discovery owner: bootstrap and scanners | T01–T03 | Versioned first-run workflow; manual scan service; multi-provider store enumeration; custom roots; safe static shell-reference import |
| T05 | Settings owner: account UI | T03–T04 | Scan/add/edit/rename/disable/default/validate/reconnect flows; async results; source/model fields; Settings render/input tests |
| T06 | Launch-policy owner: unified resolver | T02–T03 | Allowed set vs default admission vs initial instances; precedence reused across Console/CLI/programmatic starts; exhaustive table tests |
| T07 | Runtime owner: protected instance credentials | T02, T06 | Replace one-agent credential envelope/staging where necessary; exact admission set; independent profile/config/XDG/HOME per instance |
| T08 | Auth owner: refresh/source ownership | T01, T03, T07 | Native vs managed ownership contract, concurrency controls, identity/revision checks, no arbitrary copy-back; rotation/collision tests |
| T09 | Capsule owner: sessions/tabs/restore | T06–T08 | Account binding on spawn/split/new tab/restore; tabs and pane detail; capability-scoped usage; reconnect mismatch behavior |
| T10 | Usage domain owner: canonical rich observations | T02 | Numeric units/scope/reset/pool/provenance/errors, stable selection IDs, unresolved registered entries, shared quota dedupe; all consumers updated |
| T11 | Broker owner: scheduling and subscriptions | T03, T08, T10 | Due-on-open, periodic background work, per-account incremental results, shared deadlines, child-process budgets, late-result invalidation |
| T12 | Provider owner A: Anthropic/OpenAI/Amp | T01, T08, T10 | Extend existing adapters and auth sources; subscription/API distinctions; documented/local-machine interfaces preferred; contract fixtures |
| T13 | Provider owner B: Kimi/Z.AI/MiniMax | T01, T08, T10 | Current and observed response variants, product/region/protocol separation, model/routing presets, remaining/reset/balance semantics |
| T14 | Provider owner C: Google/Cursor | T01, T08, T10 | Antigravity CLI, Gemini eligibility/API scope, Cursor individual vs admin usage; private auth scope verification; no ambient-account mixing |
| T15 | Provider owner D: OpenRouter/OpenCode | T01, T08, T10 | Model selection; key vs management metrics; granular auth-store selection; OpenCode Go windows; Zen/balance capabilities |
| T16 | Provider owner E: Grok/Muse | T01, T08, T10 | First-party billing/MSP support, optional guarded internal source, exact identity/freshness and secret redaction |
| T17 | Agent-integration owners: new TUI runtimes | T01, T07–T08; parallel by agent | Actual Antigravity, Muse, Cursor, omp, Hermes clients plus supported Gemini integration; install/build/image/PTY/status/shutdown coverage |
| T18 | Console owner: Usage route | T10–T11; fixture contract allows parallel provider work | Async open/refresh/periodic monitor; overview/detail states; stable ID selection; all windows; accessible responsive layouts |
| T19 | Consumer owner: CLI/Capsule/FFI parity | T09–T11, T18 | Same facts/source/generation across surfaces; protocol/bindings migration; fixed provider catalogs deliberately reconciled |
| T20 | Independent verifier: deterministic integration | T04–T19 incrementally | All applicable checklist items below mapped to tests; no zero-test success; real process/broker/PTY and container fixtures |
| T21 | Host verifier: target Mac and live providers | T20 plus available local credentials | Apple Silicon macOS/OrbStack release-path evidence; configured account identity checks; TUI launches; real provider payload checks |
| T22 | Independent reviewers: correctness/UX/auth | T20–T21 | Review actual diffs, tests, rendered screens and live-evidence ledger; challenge unsupported API/protocol/identity claims |
| T23 | Orchestrator: finish and handoff | T22 | Remove superseded paths and temporary bypasses; final checks; complete requirement-to-evidence ledger; remaining blockers explicitly reported |

Do T01 and the source audit concurrently. Freeze the minimum T02 contracts before broad edits, then parallelize config/discovery, broker/usage, and provider adapters. T17 should split per client; installer/runtime changes must integrate with current jackin❯ catalog and construct image, not shell wrappers placed beside it. Verification starts with each vertical slice, not after all providers are implemented.

### First tracer bullet

Build one end-to-end slice before expanding the matrix:

1. Register two fixture-backed Claude profile accounts and one Codex account.
2. Save workspace admission/default choices.
3. Resolve three instances in one container manifest.
4. Launch three diagnostic fixture TUIs with independent auth/config roots.
5. Show correct account labels in tabs and exact selected usage in the Capsule relay.
6. Open host Console Usage and update all three through the shared broker without blocking input.
7. Prove a fourth unselected credential canary is absent from the container and inaccessible through its usage relay.

This proves the central account/container/usage path before dozens of provider branches rely on it. Follow it with real Claude/Codex local verification; fake TUIs cannot establish native OAuth or terminal behavior.

## 3. Repository implementation map

Paths are verified at the baseline; search by symbols and current structure before editing.

| Area | Existing code to extend | Required change |
|---|---|---|
| Agent catalog/runtime descriptors | `crates/jackin-core/src/agent.rs` and associated runtime/catalog modules | New actual client identities, versioned layouts, executable provenance, positive provider compatibility |
| Account config/discovery | `crates/jackin-config/src/accounts.rs`, `src/accounts/discovery.rs`, `src/editor/accounts.rs` | Account/source/configuration separation; all sources in multi-provider stores; Settings scan service; initialization sentinel |
| Secret references | `crates/jackin-env/src/accounts.rs`, `crates/jackin-core/src/account_env.rs` | Resolve selected account only; complete supported-variable scrub set; source revision handling |
| Account CLI | `crates/jackin/src/cli/account.rs`, `src/app/account_cmd.rs` | Reuse existing scan/add/list behavior; new schema/model/default/admission commands where needed |
| Console settings/workspaces | `crates/jackin-console/src/tui/screens/settings*`, editor state, `tui/prompts.rs`, `components/account_picker.rs` | Account scan action; defaults honored with multiple candidates; per-launch instance set, not one ID per agent |
| Workspace config | `crates/jackin-config/src/app_config/workspaces.rs` and role/config types | Authorization separate from defaults/admission/initial sessions; consistent precedence |
| Launch policy/staging | `crates/jackin-runtime/src/runtime/launch/account_config.rs`, `account_identity.rs`, `launch_pipeline/launch_core/orchestrate.rs`, `programmatic.rs` | Explicit admitted selection and per-instance materialization; revise fingerprints and restore checks |
| Profile provisioning/mounts | `crates/jackin-instance/src/lib.rs`, `src/auth.rs`; runtime launch `mounts.rs`, `capsule_setup.rs`; `jackin-core/src/container_paths.rs`; `jackin-capsule/src/runtime_setup.rs` | Evolve fixed `RoleState`/`ProvisionedAuth` slots, handoff/durable roots and first-seed setup; otherwise the new credential envelope still collides |
| Protected credentials | `crates/jackin-protocol/src/account_credentials.rs`; Capsule config loader | Evolve agent-keyed map and validation to instance/account bindings; reject mismatched protocol |
| Capsule spawn and labels | `crates/jackin-capsule/src/session.rs`, daemon/session lifecycle, `tui/model.rs`, `tui/daemon/pane_layout.rs` | Account identity through spawn/split/restore/title; apply selected credential by binding, not agent slug |
| Usage core | `crates/jackin-usage/src/usage.rs`, `usage/*.rs`, `host/accounts.rs`, `host/discovery.rs` | Complete service catalog and source-aware account observation; remove implicit global discovery bypass of registered inventory |
| Usage broker | `crates/jackin-usage/src/coordinator*`, `host/broker*`, snapshot store; runtime broker executable | Subscribed views, due refresh, credential revision, capability identity, bounded process probes |
| Canonical wire model | `crates/jackin-protocol/src/usage_broker.rs`, `control.rs`, `jackin-usage/src/host/projection.rs` | Preserve typed detail, stable IDs, both percent representations, metric scope and full freshness |
| Console Usage | `crates/jackin-console/src/tui/screens/usage.rs`; `crates/jackin/src/console/adapter/run.rs`; input/list dispatch | Replace synchronous per-account waits with effects/events; refresh on open and periodically; stable selection and detail completeness |
| Container usage capability | `crates/jackin-runtime/src/usage_relay.rs`; Capsule relay/dialog | Capability allowlist for selected accounts; more than one account per provider; no host catalog/secret access |
| CLI/desktop consumers | `crates/jackin/src/cli/usage.rs`; `jackin-usage-ffi`; `native/` generated bindings | Preserve shared semantics and compatibility through the new projection; no new native redesign required |
| Tests/policy | Existing account tests, `usage_broker_e2e*`, Console/Capsule render tests, contract fixtures, `TESTING.md` | Add behavioral scenarios, provider source inventory and no-direct-call proof; run mandated local lanes |

Keep a simple preferred-default mapping if it still serves that purpose. The defect is using that mapping as the entire container admission and per-session credential model. Arrays should not replace every mapping without semantics.

## 4. Verification checklist

For every item below, add the implementation test name/path and result to the execution ledger. Group related tests where one scenario genuinely proves several behaviors; avoid hundreds of trivial tests that merely repeat getters or serialized constants.

### A. Initialization, registration, discovery

- [ ] A01 First start with no config imports supported default profile sources automatically.
- [ ] A02 Genuine first installation with a pre-created empty config and fresh-install marker runs the explicit initialization workflow.
- [ ] A03 Interrupted discovery/config write retries without duplicate registrations or partially written configuration.
- [ ] A04 A later ordinary start does not re-add an account the user removed.
- [ ] A05 Settings Scan finds an account added after initialization; existing labels/defaults remain unchanged.
- [ ] A06 Scan reports no additions successfully; partial source failures remain visible without failing healthy scans.
- [ ] A07 Empty folders, metadata-only files, expired credentials, missing credentials, and valid profile sources have distinct outcomes.
- [ ] A08 Read limits, malformed JSON/TOML/SQLite, unreadable files, symlink loops/escaping targets, and paths with spaces are handled deterministically.
- [ ] A09 `.claude-*`, `.codex-*`, and the supplied Amp XDG-root pattern can be imported without running shell code.
- [ ] A10 Static `.zshrc` import handles literal path/profile/model mappings and reports dynamic expressions instead of evaluating them.
- [ ] A11 Scanning `.zshrc` and account stores never executes `op read`, command substitutions, functions, login flows, or model prompts.
- [ ] A12 Several normal/YOLO wrappers for one source resolve to one account with separate execution preferences when necessary.
- [ ] A13 Modern and older Kimi layouts are recognized by schema/version without copying incompatible auth formats into the wrong client.
- [ ] A14 OpenCode, omp, and Hermes stores enumerate each relevant provider/account entry separately.
- [ ] A15 Same source referenced twice is idempotent; different keys in one provider remain distinct when scopes/caps differ.
- [ ] A16 Account IDs survive rename, ordering changes, re-scan, restart, credential rotation, and model preset changes.
- [ ] A17 Changing the logged-in subject in an existing profile invalidates its previous identity/cache association.
- [ ] A18 Keychain-backed sources are discovered/validated appropriately on macOS; absence/lock prompts are classified separately.
- [ ] A19 Environment references remain references and can be unresolved in another launch environment without becoming an invalid stored account.
- [ ] A20 Removing a registration preserves host login directories and reports affected defaults/workspaces.
- [ ] A21 Upgrade of an existing pre-sentinel configuration, including an ambiguous empty registry, does not re-add previously removed accounts from disk.
- [ ] A22 Concurrent bootstrap/manual scans merge under the config lock with stable IDs and no duplicate sources or lost edits.
- [ ] A23 Discovery performs no indirect login-shell execution or native helper writes; source-tree byte checks and shell-startup canaries prove it.

### B. Settings, models, and compatibility

- [ ] B01 Add/edit supports subscription profile, custom source folder, API key, environment reference, and existing 1Password reference.
- [ ] B02 Inference validation and usage/billing permission validation display independent results.
- [ ] B03 Secrets never appear in account list, errors, validation output, help text, snapshots, or logs.
- [ ] B04 Supported agent-provider pairs are positive capabilities; unsupported combinations fail before any secret is transmitted.
- [ ] B05 Kimi Code can be configured for native Kimi, supported Claude Code, and supported Codex Responses routes with the same account identity.
- [ ] B06 Z.AI Claude/Codex and other eligible routes follow current official endpoint/protocol/client restrictions.
- [ ] B07 MiniMax Token Plan and PAYG credentials/endpoints remain distinct.
- [ ] B08 OpenRouter specific model ID survives save, reload, workspace override, launch, and restore.
- [ ] B09 Removed/unavailable OpenRouter model produces a specific validation error; it does not silently change to another model.
- [ ] B10 One account can have full/flash or short/large-context presets without duplicating its shared quota.
- [ ] B11 Disabled accounts are visible with correct state but are not launched or probed automatically.
- [ ] B12 Optional billing/admin credential belongs to the intended account/org and never substitutes for the execution credential.
- [ ] B13 Account/default forms support keyboard navigation, validation focus, cancellation, and persistence without layout regressions.
- [ ] B14 Scan completion preserves pending Settings edits/dirty state; external concurrent changes conflict or merge safely; cancellation before Apply commits no new registrations.
- [ ] B15 A valid explicit model absent from stale cached catalog stays selected with unverified status; only authoritative rejection marks it invalid.
- [ ] B16 Every provider entry found in registered client stores appears in the catalog-to-support ledger and Settings; each eligible configured launch and available usage source has implementation/proof or an explicit blocker, with no fixed-list filtering.

### C. Workspace/default resolution and container admission

- [ ] C01 Global defaults provide fast start when no more specific scope overrides them.
- [ ] C02 Workspace defaults override global defaults; workspace-role override behavior is explicit and tested.
- [ ] C03 A one-launch choice overrides defaults without persisting an unintended global/workspace change.
- [ ] C04 Multiple available accounts plus a valid selected default do not trigger an unnecessary picker.
- [ ] C05 Multiple eligible accounts without a default trigger a deterministic picker rather than random selection.
- [ ] C06 Workspace authorization restricts selections even when a global/default account exists.
- [ ] C07 Missing/disabled/deleted explicit selection fails with the correct account; no ambient fallback.
- [ ] C08 Missing selection and explicit empty selection have different, documented behavior.
- [ ] C09 Resolved container manifest includes exactly two Claude accounts and one Codex account in the central scenario.
- [ ] C10 Allowed workspace accounts not in the launch selection are absent from the container.
- [ ] C11 New-container CLI/Console/programmatic launches agree for identical inputs; new tabs/splits share binding validation against the already admitted manifest and cannot expand it from changed defaults.
- [ ] C12 Editing defaults after start does not silently alter the running container's admitted accounts.
- [ ] C13 New tab can select any already admitted account configuration and rejects accounts outside the manifest.
- [ ] C14 A requested account-set change on an existing container follows a tested explicit update/restart/new-container path.
- [ ] C15 Restore/reconnect detects changed account manifest or credential revision and preserves the intended billing identity.
- [ ] C16 Partially unauthorized inherited global defaults are filtered; invalid explicit selections/defaults fail atomically; empty-result behavior is deterministic.
- [ ] C17 Adding/renaming/disabling unrelated account D does not invalidate A/B/C instance reuse or rotate their capabilities.
- [ ] C18 Disable/removal denies new grants and shows active-instance state without claiming already materialized upstream credentials were revoked.

### D. Actual multi-account runtime and credentials

- [ ] D01 Two Claude processes have different `CLAUDE_CONFIG_DIR`/account metadata and prove their selected identities.
- [ ] D02 Two Codex processes have different `CODEX_HOME`/auth-store context, config profiles and session state.
- [ ] D03 Amp custom XDG profile stages all required roots; a refreshable login is not misused as an `AMP_API_KEY` token.
- [ ] D04 OpenCode account isolation includes XDG data/auth, not just a config-directory override.
- [ ] D05 Multi-provider JSON/SQLite staging contains selected entries only, with all unselected canaries absent.
- [ ] D06 omp broker/account-pool filters are not treated as an authorization boundary; container-accessible token/store scope is actually restricted.
- [ ] D07 Hermes concurrent profiles do not share mutable state or cloned rotating refresh tokens without a proven ownership mechanism.
- [ ] D08 Antigravity private keyring/profile supports the claimed multiple-account mode in the target Linux container; folders alone do not count as proof.
- [ ] D09 Cursor custom config scope is verified to isolate auth and relevant state; otherwise use a proven per-instance HOME/runtime mechanism.
- [ ] D10 Muse instance HOME/handshake state binds to the selected account; key-exchange responses never leak returned API keys into telemetry/cache.
- [ ] D11 Grok stored subscription auth cannot override an explicitly selected API billing account, and vice versa.
- [ ] D12 Ambient API keys, bearer tokens, base URLs, homes, profiles and model variables cannot override a chosen account.
- [ ] D13 Secret values are absent from Docker argv, labels, image history, Docker inspect Config.Env, public config, process diagnostics and errors; mount/staged-store inventory contains no unselected credential canary.
- [ ] D14 Only minimum selected auth/config is staged; no whole host home, browser store, host keychain, or unfiltered auth database is mounted.
- [ ] D15 Shared OAuth refresh has one proven owner; simultaneous polling and agent launches cannot publish stale rotated tokens over new ones.
- [ ] D16 Revocation, expiry, host logout, account swap, and source revision change invalidate the right cache and capability only.
- [ ] D17 Late refresh results cannot resurrect a removed account or overwrite another account's observation.
- [ ] D18 Linux amd64 and arm64 support are verified where claimed; installer/source/version/digest are recorded.
- [ ] D19 Each requested actual TUI is interactive in the container, handles resize/input/paste, and exits/cleans up correctly.
- [ ] D20 No provider login secret or refresh token is embedded in fixtures, generated docs, screenshots, or failure reports.
- [ ] D21 Copied paths with one OAuth grant coordinate refresh by credential lineage; different grants sharing billing identity do not become one unsafe writer.
- [ ] D22 Missing/expired explicit custom profiles fail without falling back to ambient native/other-client/keychain credentials.

### E. Tabs, sessions, and usage authorization

- [ ] E01 Tab labels distinguish Claude Work, Claude Personal, and Codex Work in one container.
- [ ] E02 Agent/provider/account/model metadata follows new session, split, pane move, tab rename, exit, resume and restore.
- [ ] E03 Custom tab title does not remove the ability to identify the selected account in pane/status details.
- [ ] E04 Usage detail opened from a pane selects the actual account and provider, including non-native provider routing.
- [ ] E05 Capsule shows only launch-admitted account capabilities; it cannot query other registered host accounts by guessed ID.
- [ ] E06 Usage discovery never changes which account launches or blocks launch solely because quota data is missing.
- [ ] E07 Multiple selected accounts for one provider remain separately addressable through the relay and cache.
- [ ] E08 A selected account not yet used by a TUI can be monitored if the provider permits host quota reads.

### F. Quota semantics and provider contract fixtures

- [ ] F01 Display session/rolling, weekly and monthly windows together where all are supplied.
- [ ] F02 Classify period by explicit duration/type rather than primary/secondary position or a translated label.
- [ ] F03 Percentage used and remaining modes have matching text and bar geometry; both DTO representations survive Console mapping.
- [ ] F04 Balance without a denominator has no fabricated percentage bar.
- [ ] F05 Unknown, no permission, not applicable, not started, exhausted and unavailable remain distinct.
- [ ] F06 Zero, tiny fractional values, over 100%, null limits, negative balances/adjustments and currency precision follow the contract.
- [ ] F07 Reset time, credential expiry and subscription renewal use different fields and labels; timezone/DST/countdown behavior is correct.
- [ ] F08 Passing a reset timestamp does not synthesize full quota before a fresh provider response.
- [ ] F09 API token totals/spend/rate limits are never presented as subscription tokens remaining.
- [ ] F10 Shared account/model pools are not summed twice; different key caps or organization scopes are not incorrectly merged.
- [ ] F11 Claude fixtures cover current named/array quota formats, extra usage, per-model limits, scope restrictions and source-bound auth.
- [ ] F12 Codex fixtures cover app-server rateLimitsByLimitId, variable durations, credits/reset-credit inventory, API auth, and missing/extra fields.
- [ ] F13 Amp fixtures cover free/daily, Agent dollars, Orb hours, monthly periods, workspace balances, and linked external subscription routing.
- [ ] F14 Antigravity fixtures cover 5h/weekly pools, official CLI JSON, missing account identity, unsupported old command version, local 401 and availability-only responses.
- [ ] F15 Gemini fixtures distinguish retired consumer OAuth from supported eligible accounts, project quotas and API-key/Vertex usage.
- [ ] F16 Kimi fixtures cover old/new response/layout families, weekly/rolling/monthly pools, extra usage wallet and shared account identity across clients.
- [ ] F17 Z.AI fixtures cover CREDIT_LIMIT/TOKENS_LIMIT, actual period/reset metadata, MCP/time quotas, team scope and HTTP-success error envelopes.
- [ ] F18 MiniMax fixtures cover Token Plan vs older field names, 5h/weekly periods, remaining amount, region and optional plan/balance failures.
- [ ] F19 Cursor fixtures cover personal/internal vs Enterprise admin authorization, pooled/personal caps, credits and distinct actual/estimated money units.
- [ ] F20 Grok fixtures cover weekly/monthly provider periods, current billing, prepaid/on-demand bounds, RPC/HTTP failures and auth precedence.
- [ ] F21 Muse fixtures cover MSP cached/changed usage, observedAtMs, over-100% values and optional internal key-response sanitization.
- [x] F22 OpenRouter fixtures cover successful /key with /credits 403, management-scope mismatch, null cap, BYOK, models, and the ordinary-key credential/discovery path. Delayed history remains management-key-only under U12.
- [ ] F23 OpenCode Go fixtures cover rolling/weekly/monthly status/percent/reset values; don't invent per-model quota from a shared response.
- [ ] F24 omp/Hermes usage is attributed to underlying provider accounts; their local counters do not become an independent subscription budget.
- [ ] F25 Optional enrichment failure preserves successful primary quota and records partial status; one malformed provider cannot blank all rows.
- [ ] F26 Fresh quota plus old balance/history retains separate timestamps; wrong-account/org/project/region enrichment is rejected; optional timeout does not delay primary publication.
- [ ] F27 Switching account A to B with the same log directory never assigns A's history to B; ownerless cached totals are reclassified before first multi-account display even offline.
- [ ] F28 Shared/copied/fork/subagent/release-channel events are deduplicated; local measured totals and provider-wide totals are not added twice.
- [ ] F29 Explicit scope fixtures cover Claude inference-only tokens, Z.AI MCP vs coding windows, Cursor Grok Bot vs Grok Build, and OpenRouter completed-day history. OpenRouter history is blocked by the missing separate Management-key source (U12); Go 1%-versus-fraction/entitlement responses remain pending.

### G. Broker scheduling, UI and process behavior

- [ ] G01 Opening Usage with an empty cache requests due data and displays registered rows immediately.
- [ ] G02 Opening with fresh cached data uses the broker's freshness policy without redundant provider requests.
- [ ] G03 The open screen refreshes periodically with no user input and displays next/last update context.
- [ ] G04 Long-idle/low-power behavior is explicit; wake and reconnect refresh without replaying every missed interval.
- [ ] G05 Manual refresh joins active work and respects shared Retry-After; repeated keypresses do not queue duplicate calls.
- [ ] G06 2-client and 20-client same-account requests produce the expected single provider generation.
- [ ] G07 Independent accounts progress concurrently within provider limits; a stalled account doesn't serialize the whole overview.
- [ ] G08 Closing a screen stops its subscription promptly and does not destroy shared work needed by another client.
- [ ] G09 Timeout/cancellation cleans up usage subprocesses and preserves broker ownership/last-good state correctly.
- [ ] G10 401 refresh/reconnect, 403 scope, 429 backoff, 5xx, timeout, malformed payload and offline each have specific recovery behavior.
- [ ] G11 Broker restart/owner loss/corrupt state recovery does not cross account boundaries or discard valid last-good data silently.
- [ ] G12 Account removal/rename/order change preserves stable selection; removal returns to Overview with a notice.
- [ ] G13 Overview/detail loading, empty, disabled-only, partial, all-failed, stale and recovered states render correctly.
- [ ] G14 60×18, 80×24, 100×32 and 120×40 render cases handle long Unicode labels, focus, scrolling and terminal resize.
- [ ] G15 Text conveys state independently of color; all controls have discoverable keyboard actions.
- [ ] G16 50-account cached-open and input latency measurements meet the spec's targets or produce explicit measured failures.
- [ ] G17 No direct provider calls originate in UI/FFI/CLI render adapters; source call inventory and process tests prove broker ownership.
- [ ] G18 Console, CLI, Capsule and affected native views agree on account, scope, metric, reset, source and generation for shared fixtures.

### H. Migration and final local acceptance

- [ ] H01 Current user configuration is converted or rejected with an actionable conversion path; no silent loss of accounts/default intent.
- [ ] H02 Schema transition is atomic/idempotent and removes the obsolete execution resolver/transport path.
- [ ] H03 Protocol/build mismatch returns a clear restart/upgrade error rather than misinterpreting account credentials.
- [ ] H04 Existing unit, integration, format, lint, docs and snapshot gates pass for all affected crates.
- [ ] H05 Actual Apple Silicon macOS 26 with OrbStack runs the required usage-broker E2E tests and produces JUnit evidence.
- [ ] H06 The live provider/account matrix records authenticated identity and available real fields for each configured account.
- [ ] H07 Central two-Claude-plus-one-Codex container scenario is repeated with real accounts and visible TUIs on the target Mac.
- [ ] H08 All requested actual client runtimes receive at least one real container TUI smoke verification where credentials are available; missing access stays blocked.
- [ ] H09 Provider field claims are compared with native usage command/dashboard and timestamps within a documented tolerance.
- [ ] H10 Independent reviewer checks implementation plus proof, not only the checklist or agent summaries.
- [ ] H11 Remaining provider limitations are precise capability states with evidence; no unavailable API is disguised as a fabricated number.
- [ ] H12 Final handoff names exact SHA, commands, measured results, fixtures, local live coverage and every blocker. No unrun check is reported passed.

## 5. Provider validation layers

Use four complementary layers:

1. **Parser/semantic fixtures:** reviewed, sanitized real response shapes and independent expected values. Assert unit/scope/reset/identity, not just successful deserialization. Include additive unknown fields and malformed required fields.
2. **Service/process integration:** local HTTP/RPC fixture servers and diagnostic fake TUIs launched through the real runtime path. Assert observed credentials via non-secret identity/canary results, broker call counts, concurrency and failure recovery.
3. **Container integration:** actual jackin❯-created container, real manifests/staging/relay/PTYs, private account homes and negative credential-access tests. Fixture providers make failure and isolation reproducible.
4. **Authenticated host acceptance:** installed real clients, actual configured accounts, provider-native read-only usage/identity calls, interactive TUI smoke and macOS/OrbStack transport. Credentials stay on the user's machine.

Prefer read-only usage/identity operations. Where proving a client/provider launch requires inference, use a bounded minimal task in a disposable workspace and record the fact. Never trigger a purchase, plan change, reset-credit redemption, destructive repo command or unrelated external communication as a usage test.

## 6. Local commands and environment

The following commands are documented in the audited repository. Recheck current `TESTING.md`, `mise.toml`, Cargo aliases/features and help output before using them. This plan does not claim they were executed here.

```sh
mise install
cargo nextest run -p jackin-core -p jackin-instance
cargo nextest run -p jackin-config -p jackin-env -p jackin-protocol
cargo nextest run -p jackin-usage -p jackin-usage-ffi
cargo nextest run -p jackin-runtime -p jackin-console -p jackin-capsule
cargo clippy -p jackin-config -p jackin-env -p jackin-protocol -p jackin-usage -p jackin-runtime -p jackin-console -p jackin-capsule --all-targets -- -D warnings
cargo xtask ci --fast
cargo xtask ci
```

Run focused tests while implementing, then full relevant gates once changes stabilize. After a failure or new change, rerun the affected gate; don't repeatedly rebuild everything without a reason. Include `jackin-core`, `jackin`, launch/auth crates or FFI checks when changed, even if not listed in the focused examples.

Docker-backed Rust test entry point:

```sh
cargo nextest run -p jackin --features e2e --profile docker-e2e
```

Follow the repository's current Capsule build/export instructions before this command. At the audited baseline, PR checkouts use `jackin-dev pr sync <PR_NUMBER>` plus the generated PR environment; outside that workflow the documented `build-jackin-capsule -- --export` preparation supplies the Capsule artifact. Use the existing approved mechanism rather than inventing a different test image.

**Mandatory target-Mac gate:** `TESTING.md` specifically requires Apple Silicon macOS 26 with OrbStack running for changes to usage discovery, broker/state/relay, refresh, or launch assembly:

```sh
cargo xtask ci --e2e
```

Verify `usage_broker_e2e` actually executes under the `docker-e2e` profile; zero matched tests is failure. Retain the JUnit report under `target/nextest/docker-e2e/` using the approved evidence workflow. A generic Linux Docker pass does not satisfy this repository requirement. [Audited testing policy](https://github.com/jackin-project/jackin/blob/5bf20aaf37bbc49325072d199e09effe0678b047/TESTING.md).

Every manual jackin❯ invocation must include `--debug`, as required by the audited testing policy. The canonical Console smoke entry is:

```sh
cargo run --bin jackin -- console --debug
```

For changed shared DTO/Swift bindings, also run the repository's bindings and native verification lane, including `mise run desktop-ci` and `mise run desktop-merge` on a logged-in Mac; the latter includes native UI tests. Do not mark native checks passed from a Linux compilation. For documentation integrated into jackin❯, run:

```sh
cargo xtask roadmap audit
cargo xtask docs repo-links
cargo xtask research check
```

TUI snapshots live in the Console and Capsule crates. Review changed frames using the repository workflow; do not blindly accept snapshots that encode a broken layout. Preserve at least the central three-account scenario and all major Usage state variants.

### Execution ledger template

| Requirement/check | Test or manual scenario | Source/CLI version | Environment | Expected result | Actual result/artifact | Proof level | Status/blocker |
|---|---|---|---|---|---|---|---|
| C09/D01/D02/E01/H07 | Real three-account shared container | Filled during execution | User Mac + OrbStack | Correct account in each TUI/tab; no account D | Not executed in research | Live/container | Not started |
| G06/H05 | 2/20-client broker tests | Current jackin❯ SHA | Required Mac lane | One shared generation | Not executed in research | Process/container | Not started |
| F22 | OpenRouter key success, management denied | Versioned fixture | Local test server | Key usage shown; balance permission message | `jackin-usage` parser, dispatch, discovery, cache/error tests pass | Fixture | Verified; completed-day activity remains U12 |
| F14/D08 | Antigravity identity and isolated auth | Installed `agy` version | Linux container on Mac | Selected account provenance; correct usage | Needs local proof | Live/container | Not started |

## 7. Definition of done

Completion requires the full product flow, not just provider adapters or a usage table: bootstrap → Settings → named account/configuration → workspace/default/admission → per-instance credentials → real TUI/tab → broker observation → overview/detail refresh → restore.

Every requested client/service must be represented in the final capability matrix and implemented to the extent that the verified vendor interfaces permit. An adapter reporting a truthful unsupported metric is correct when the provider genuinely offers no authorized source. A missing implementation of an available source is unfinished work. A live test blocked by absent login/access is unverified, not unsupported and not passed.

The orchestrator must continue all independent work while a local login or unavailable provider blocks one lane, then provide a specific pending action and evidence. It must never reduce the requirement to screenshots, accept fake API results as live proof, or claim that an installation can monitor universal token balances that the providers do not expose.
