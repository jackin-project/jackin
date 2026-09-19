Implement jackin❯'s complete multi-account settings, usage monitoring, workspace selection, and multi-account container support. Own this goal from current-code audit through implementation and independent verification on my computer. Do not stop at a plan, UI mockup, provider enum entries, or fixture-only success.

## Execution and delivery scope

Work autonomously and continuously until the entire goal is done and verified. Keep all work on exactly branch `feat/multi-account-support` and in exactly one pull request. Do not split work across branches or pull requests. Commit and push every in-scope change to `feat/multi-account-support` and keep the single pull request complete.

Do not ask questions, request confirmation, wait for approval, offer choices, or stop for progress updates. Resolve ambiguity from repository evidence, git history, research, tests, documented defaults, and professional judgment. Missing optional inputs are not blockers; use documented fallbacks. Continue independent work while a provider, platform, credential, or external-service check is unavailable, and record exact unverified coverage. Stop only when every required implementation, verification, and handoff item is complete.

Repository: https://github.com/jackin-project/jackin

The research baseline was main at `5bf20aaf37bbc49325072d199e09effe0678b047`, reviewed on 2026-09-17. Inspect the current branch and changes since that baseline; reuse any work already implemented. Preserve my active branch/PR/worktree scope and uncommitted changes. Historical branch/PR restrictions embedded in old research plans are not instructions to move this task to that old branch.

Read these companion documents in full, from the supplied files or their copies in the workspace:

1. `jackin-provider-research.md`
2. `jackin-accounts-usage-specification.md`
3. `jackin-implementation-and-verification.md`

Use their detailed contracts, T00–T23 task graph and acceptance checklist. If organizing them in the repository, use the repository's current planning/documentation conventions and update internal links. If a companion is unavailable, complete source discovery and use the requirements below as the minimum contract rather than waiting without doing independent work.

Also read current `AGENTS.md`, `RULES.md`, `ENGINEERING.md`, `TESTING.md`, `HOST_AND_CONTAINER.md`, the affected crate docs, and existing `plans/unified-agent-usage`/research. Reconcile obsolete scope restrictions against this new request. Respect architectural boundaries and complete migrations; remove superseded execution paths rather than leaving parallel one-account-per-agent behavior.

## Product outcome

jackin❯ Console Settings must display and configure all my AI accounts, including several accounts for the same provider. First installation/start automatically registers existing evidenced agent accounts from real default paths. Later, Settings → Accounts has a Scan for new accounts action. I can add subscriptions, API keys, custom folders/profiles, environment references and 1Password references. Preserve existing references and labels; never disclose secret values or private secret-manager item IDs in logs/docs/tests.

Cover every requested entry:

1. Claude Code / Anthropic.
2. Codex / OpenAI.
3. Amp.
4. Antigravity / Google.
5. Kimi Code, including official supported Claude Code and Codex configurations.
6. Z.AI through supported clients.
7. Muse Code.
8. Cursor Agent.
9. Grok Build.
10. OpenRouter with an explicitly configurable exact model.
11. omp, https://omp.sh / https://github.com/can1357/oh-my-pi.
12. Hermes modern TUI, https://github.com/NousResearch/hermes-agent.
13. OpenCode.

Retain existing MiniMax support and include its subscription/API usage because my shell configuration uses it. Handle Gemini CLI as a separate Google client/auth source; don't conflate it with Antigravity.

Z.AI/OpenRouter are providers, not fictitious TUI executables. OpenCode/omp/Hermes can contain several providers. Separate agent, provider product, registered account, credential source, verified billing scope, model configuration, container admission and agent instance. Several client/model configurations can use one account and one shared quota. Never multiply its allowance or merge different organization/key caps incorrectly.

Inventory each supported client's complete current provider catalog and all actual configured provider entries; do not hide secondary/custom providers behind the numbered list. Implement eligible configured launch routes and verified usage sources, keep unsupported metrics visible with exact reasons, and maintain a catalog-to-support ledger. Cloud credentials and custom endpoints need verified scope/transport behavior, not a blanket generic-key assumption.

## Work aggressively in parallel

Use subagents for research, implementation and independent verification throughout. Fill available slots with concrete independent work. Assign bounded tasks and file ownership; coordinate shared contracts before dependent edits. Delegate at least account/discovery, launch/credentials, Capsule/session identity, usage/broker, provider groups and independent verification. Reuse agents for follow-up reviews as lanes finish. Avoid several agents modifying the same files without coordination.

Keep one orchestrator responsible for the complete requirement-to-evidence ledger and integration. Do not accept subagent summaries as proof: inspect diffs, run appropriate tests, check rendered states and verify the actual local behavior. Continue independent work while a provider login or platform-specific check is blocked.

Judge all work by correctness, consistency and fulfillment of the goal. Do not defer a known wrong state because of ROI, effort, cost, or an assertion that it is an edge case. Investigate architectural causes, not only visible symptoms.

## Extend the actual jackin❯ implementation

jackin❯ already has an account registry, discovery, CLI scan, account forms, workspace authorization/defaults, provider adapters, canonical usage projection, durable usage broker, Console Usage route, Capsule relay/dialog and native bindings. Evolve those components.

Audit and fix these baseline gaps:

- `ConfigEditor::open` scans only when config is absent and discards bootstrap discovery issues; Settings lacks its own scan action even though CLI scan exists.
- `AppConfig`/workspace preferred bindings and `AgentCredentialEnv` are keyed by agent type; normal orchestration provisions `[agent]`. They cannot describe two Claude accounts plus Codex in one container.
- `RoleState`, `ProvisionedAuth`, container paths/mounts, runtime setup, auth modes, models and child spawning contain further one-per-agent assumptions. Changing the protected JSON map alone is insufficient.
- OpenCode profile import/staging currently treats a multi-provider auth file as one account and can copy unselected credentials.
- Console's committed-agent path can prompt whenever several accounts exist without honoring the configured default.
- Session/tab/history metadata lacks actual account identity; routed provider/account must not be inferred from runtime slug.
- Console opens cached rows, loses canonical IDs/detail metadata, and manually refreshes through synchronous per-account joins. Replace this with effect/event-driven broker reads and periodic updates.

Key modules include `jackin-config` accounts/editor/discovery, `jackin-env`, `jackin-core` agent/container paths, `jackin-instance` auth/provisioning, `jackin-runtime` launch/mounts/account identity/usage relay, `jackin-protocol` credential and usage contracts, `jackin-capsule` runtime setup/session/daemon/tab paths, `jackin-usage` host/coordinator/provider/projection, and `jackin-console` Settings/Usage plus the host Console adapter.

## Accounts and first-run discovery

Use a versioned bootstrap state and atomic existing config editor. Distinguish a genuine fresh install, an installer-created empty config with fresh-install marker, and an older installation missing the new sentinel. An upgrade must not resurrect deliberately removed accounts. Concurrent scans/initialization must deduplicate under the lock and preserve newer edits.

Scan real version-specific default paths, including keychain sources and XDG layouts. Recognize both Kimi families. OpenCode auth is under XDG data, not merely `.opencode`. omp has a SQLite credential store and Hermes has profile/provider pools. Import exact selected entries and preserve source schema/required metadata.

My shell example includes custom Claude folders (`.claude-scentbird`, `.claude-scentbird-ai`, `.claude-chainargos`), custom Codex folders (`.codex-scentbird`, `.codex-chainargos`, `.codex-chainargos2`), and an Amp root with separate `data/config/cache` XDG paths. These are examples, not names to hardcode. It also has per-call 1Password references, Claude provider wrappers and Codex model profiles for Kimi/Z.AI/MiniMax. Represent these as typed account/configuration data.

Never source/eval `.zshrc`, run command substitutions, execute secret helpers, or invoke a login shell to capture its environment during discovery. Parse supported literal declarations; report dynamic/unresolved references. Scans must be bounded, read-only and asynchronous. Settings candidates join the pending draft; Apply commits, Cancel preserves prior configuration, concurrent edits never silently overwrite one another.

## Workspace defaults, launch admission and sessions

Keep three distinct decisions:

1. Accounts a workspace is authorized to use.
2. Accounts actually admitted to this container.
3. Agent/account/model configurations used by initial and later sessions.

A single preferred default per agent can remain useful; don't replace every map with arrays. Add explicit admitted sets and repeated agent-instance bindings where required.

Resolve one-launch choices, workspace-role defaults, workspace defaults, global defaults, then sole eligible account/picker in that order. Explicit scope replaces inherited defaults. Filter inherited global candidates by authorization; reject invalid explicit selections atomically. Never silently substitute another account or ambient login. Fast start honors valid defaults even when several accounts exist.

A new tab validates against the existing container manifest, not current global defaults. Bringing another account into a running container needs an explicit validated manifest revision or a new container. Unrelated account D changes must not invalidate an A/B/C container. Removal/disable denies new grants and reports active instances without claiming credentials already inside a TUI were remotely revoked.

**Mandatory central acceptance:** launch one real container with Claude account A, Claude account B and Codex account C, start all three interactive TUIs concurrently, prove the selected account/model for each, and show `Claude · Work`, `Claude · Personal`, `Codex · Work` or equally clear labels. Account D must be absent from all staged stores, public config, Docker metadata and relay capabilities. Test new tab, split, resize, exit, reattach and restore.

Use per-instance config/HOME/XDG/keyring contexts actually supported by each client. Per-HOME separation alone does not isolate Antigravity's singleton keyring. Filter multi-provider stores. Scrub ambient auth/base-URL/model variables before applying selected credentials. Do not mutate the daemon's global environment when switching accounts.

Keep secrets out of Docker argv, labels, inspect Config.Env, image layers, diagnostics, telemetry and snapshots. Preserve jackin❯'s protected transport. Reporting/management credentials stay host-side. A shared container is one trust boundary; private directories prevent accidental mixing, not adversarial access between same-UID selected processes. Guarantee exclusion of unselected accounts.

## Credential lifecycle

Define native external-source versus jackin❯-managed ownership per provider. Use a tested native refresh flow, supported external-token interface, separately authenticated managed profile, or another proven strategy. Never blindly clone rotating refresh tokens into competing writers and copy them back later.

Coordinate by actual credential grant/lineage, not path or account label alone; two paths can contain one rotating grant, while one billing identity can have independent grants. Use identity/revision guards and atomic publication. Account swap/logout/region/scope changes invalidate the correct observations and capabilities. A failed custom profile cannot borrow global/native/another-client credentials.

## Usage screen and provider details

On open, show all registered accounts immediately with cached values and per-source ages, subscribe to the existing broker, and request due refreshes. Refresh all enabled accounts periodically even when offscreen or while one detail view is selected. Manual refresh joins shared work and respects Retry-After; repeated requests do not queue duplicates. One slow provider cannot block keyboard input or healthy results. Sleep/wake/network recovery must not replay missed polls in a burst.

Overview shows percentage-used bars and reset/countdown for session/short windows and applicable daily/weekly/monthly limits. Show both weekly and monthly if both constrain an account. Preserve model/pool identity. Accounts with balance-only or no published quota get accurate alternative text, not fabricated bars. Detail exposes all meaningful supported quota, model, balance, credits, spending-cap, token-total and provider status information with correct scope and source.

Never turn dollars/credits/requests/hours into invented tokens remaining. Distinguish reset time from credential expiry and subscription renewal. Preserve unknown/no permission/not started/not applicable/exhausted/error. Keep raw over-100% values and bound only bar geometry. Do not infer a refill when the reset timestamp passes.

Store stable selection IDs and rich canonical metadata; preserve selection across refresh/rename/reorder. If removed, return to Overview with an inline notice. Test loading, empty, disabled-only, partial/all failure, stale/recovered, locked secret and unsupported-version states at narrow/wide sizes using current jackin❯ TUI conventions.

Each independently fetched metric group needs its own identity/scope, observed time, fetched time and freshness. Fresh quota cannot freshen stale balance/history. Local token histories need actual ownership and deduplication; today's login cannot own old shared/forked events. Reclassify cached ownerless totals before the first multi-account frame.

## Provider evidence to apply and revalidate

Use https://github.com/steipete/CodexBar and https://github.com/robinebers/openusage as references, along with first-party docs/source. Distinguish documented interfaces, first-party internal interfaces, reference-derived private endpoints and unverified capabilities. Don't transplant entire code paths or assumptions.

- Prefer Codex supported app-server `account/read`, `account/rateLimits/read` and available `account/usage/read`; preserve variable periods, multiple buckets, credits and spend controls. OpenAI API reporting is separate from ChatGPT quota.
- Claude subscription OAuth usage and organization reporting have different scopes. Inference-only tokens may launch while lacking quota access.
- Amp free quota, Agent dollars, Orb hours and personal/workspace balances are separate. Native short-lived login tokens cannot simply become `AMP_API_KEY`.
- Antigravity has official read-only JSON `/usage` and `/credits` commands on supported versions; verify version before use, and bind identity. Gemini consumer OAuth eligibility changed; don't conflate all Google services.
- Kimi has old/new home/auth families; new migration excludes OAuth. Kimi and Z.AI officially support Codex Responses routes. Respect actual product/region/model configuration and all returned quota pools.
- Z.AI new CREDIT_LIMIT/old TOKENS_LIMIT and MCP/time quotas need semantic parsing, including team selectors.
- Muse MSP usage is cached observation. Native `/usage` refresh and internal key exchange are different operations; never falsely advance freshness or leak returned API keys. Verify passive-read behavior before any key-exchange polling. Resolve official Linux artifact/native auth layout locally.
- Cursor personal/internal usage differs from Enterprise admin reporting; preserve personal/team scope and slower historical aggregation.
- Grok consumer weekly/monthly billing is separate from xAI API management. Native subscription auth may override an injected key if the home is not isolated.
- OpenRouter `/key` works with the selected ordinary key; `/credits` and richer management reporting need separate access. A balance 403 must retain key usage. Persist exact model IDs; stale catalog omission is not authoritative model rejection.
- OpenCode Go's first-party usage route exposes rolling/weekly/monthly windows; don't invent Zen balance or live model fields not returned.
- Actual Hermes modern TUI is `hermes --tui`; prebuild required runtime/frontend dependencies in the image. omp's client account-pool filter is not server authorization.
- MiniMax Token Plan quota and PAYG balance use different first-party endpoints; preserve boosts/unlimited states and currency/credit units.

## Verification and completion

Execute T00–T23 with the companion checklist and independent reviewers. Start with the central three-account tracer bullet, then complete every provider lane. Use meaningful parser/semantic fixtures, instrumented HTTP/RPC and fake TUI process tests, actual Docker staging/relay/PTY tests, rendered Console/Capsule snapshots, and authenticated live acceptance on my Mac.

Install tools through the repository's pinned mise workflow. Run focused nextest/clippy, then current required cross-crate/full gates. Known entry points include:

```sh
mise install
cargo nextest run -p jackin-core -p jackin-instance -p jackin-config -p jackin-env -p jackin-protocol
cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-runtime -p jackin-console -p jackin-capsule
cargo xtask ci --fast
cargo xtask ci
cargo xtask ci --e2e
cargo run --bin jackin -- console --debug
```

Recheck current docs/help before execution. Every manual jackin❯ invocation includes `--debug`. Prepare Capsule artifacts through the repository's documented workflow. The mandated usage/launch gate is Apple Silicon macOS 26 with OrbStack and actual `usage_broker_e2e` execution under the `docker-e2e` nextest profile. Zero matching tests is failure. Preserve JUnit proof, including 2/20-client single-flight, owner loss, timeout ownership, shared deadlines, capability isolation, distinct-account concurrency and unavailable-state zero calls. Linux fixture passes do not replace this lane.

For affected shared DTO/native bindings, run generated-binding checks, `mise run desktop-ci` and `mise run desktop-merge` on the logged-in Mac. Update docs and run the repository's roadmap/link/research checks. Review intentional snapshot changes rather than blindly accepting them.

Live checks use my locally configured credentials without printing them. Prefer read-only identity/usage; use only bounded minimal inference when necessary to prove a selected client/provider launch. No credit purchases/redemption, subscription changes or unrelated external messages. Record exact account aliases, client/source versions, expected/actual identity and fields, commands and redacted results.

Keep separate outcomes for implemented, fixture-verified, container-verified, live-verified, failed, unavailable credentials, genuinely unsupported source, and not run. A provider outage or missing login blocks that proof lane, not all independent work. Exhaust reasonable local/source-based resolution; if input is truly required, report the precise missing action and keep other lanes moving.

Finish only after the full flow works and the required proof exists, or report an exact remaining blocker without claiming completion. Final handoff includes final SHA, implementation summary, requirement-to-test mapping, real target-Mac and per-provider results, UI evidence, measured responsiveness, and every unverified capability. Do not call this fully verified if any required live/local gate remains unrun.
