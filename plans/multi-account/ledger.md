# PR #1002 requirement and evidence ledger

Scope: multi-account settings, discovery, usage, authorization, launch and containers on feat/multi-account-support only. This is a reconciled WIP inventory, not a completion declaration. Checklist source: jackin-implementation-and-verification.md §4 (A01–H12); product contract: jackin-accounts-usage-specification.md; operator gates: plans/multi-account/GOAL.md and TESTING.md.

## Status rules

- PASS applies only to a named requirement proved on the exact candidate SHA with command/scenario, environment, output/artifact and owner recorded.
- Historical results and uncommitted merge-tree results are informative only. They do not prove the final candidate.
- UNVERIFIED means no qualifying evidence is recorded. BLOCKED means a concrete code/security/verification issue prevents acceptance. Missing live access is not “unsupported.”
- Every final row must name exact commit, environment, command or manual scenario, result/artifact path and evidence owner. Keep credentials local and redact values.

## Snapshot and recorded evidence

| Item | Recorded fact | Status / limitation |
|---|---|---|
| PR | #1002, feat/multi-account-support; inspected remote head 2a318440ce2e02a15a76530812f375ee4998a0f3; base 41796158b1e45535ae4e74d5ff048cb5bb4e0488. | Historical inspected PR snapshot. Main fce94cea is now integrated by pushed commit a4930735; this is not the final PR candidate. |
| Diff/history | 446 files, +64,813/-5,785; 146 commits ahead of inspected base. | Broad surface requires final independent review. |
| Reviews | No formal submitted review; no inline review threads. One bot issue comment reports review usage limit reached. | Independent review remains required: H10. |
| PR checks at inspected head | Policy failed (run 35521088323); DCO passed. | Diagnose and rerun policy on final candidate. |
| Focused main-sync runs | See plans/multi-account/evidence/2026-09-21-main-sync.md. mise install passed (36 pinned tools); telemetry 65 tests; diagnostics 96 tests; combined core/telemetry/diagnostics 293 tests; protocol 120 passed/1 ignored; config 458 tests. | These ran on an uncommitted merge working tree based on feature 2a318440 + main fce94cea. Package-scoped; not post-merge candidate proof. |
| Host for those runs | Apple Silicon arm64, macOS 27.0, Rust 1.97.1, nextest 0.9.140. | TESTING.md requires macOS 26 + OrbStack for the mandatory usage-broker lane. This host does not satisfy the exact OS gate. |
| Required live/native/container gates | No qualifying final-candidate evidence recorded for cargo xtask ci --e2e, the live provider matrix, real two-Claude/one-Codex container/TUI scenario, desktop/native merge, or all GOAL gates. | UNVERIFIED. |
| Current source/UI gaps | Console checkpoint b196b71 retains the canonical projection, but its renderer still displays windows only and its tests require DTO migration; Capsule still renders the older focused usage view without metric groups. See crates/jackin-console/src/tui/screens/usage.rs and crates/jackin-capsule/src. | Projection/UI parity remains open; source inspection is not a test result. |
| Current security/runtime review findings | Review owners report an auth leaf-symlink/CAS publication issue and a broker publication lease acquired only after discovery, permitting stale catalog activation. | BLOCKED pending owner reproductions, fixes and regression evidence. Historical fixes do not prove these findings resolved. |
| Provider inventory | plans/multi-account/catalog-to-support-ledger.md is the inventory source; the current audit reconciles the existing OpenRouter collector against missing production dispatch and capability coverage. | Reconcile inventory against code/current official sources. Adapter existence is not live authorization or field proof. |
| Historical evidence | Older ledger/HANDOFF claims, including E2E/tracer results, predate this candidate. | Historical only; never count as current PASS without rerun at final SHA. |

The focused proof owner is the orchestrator for the merge-tree evidence artifact above. Replace role names with actual runner/reviewer and exact SHA in final entries.

## Product acceptance crosswalk

All journeys below are required by the product specification. Current status for each: UNVERIFIED on final candidate; none is promoted to PASS by focused merge-tree package runs.

| ID | Required observable behavior | Required evidence and owner |
|---|---|---|
| P01 | First start discovers/imports supported existing logins; one damaged source does not block healthy providers. | Fresh-profile bootstrap, source fixtures, Settings/Usage inventory. Owner: accounts + verification. |
| P02 | Later Settings Scan discovers selected new sources, preserves labels/defaults/edits; no-addition and partial-failure outcomes are explicit. | Interactive/command integration with pre/post config snapshots and failure injection. Owner: accounts + UI. |
| P03 | Add subscription/custom profile/API key/env/1Password reference; validate inference and usage permission separately; secrets remain private. | Form flow, validation/error redaction, staging and secret-canary tests. Owner: accounts + credential review. |
| P04 | Usage opens with inventory/last-good timestamps, refreshes asynchronously and periodically; detail shows available provider-specific data and truthful unavailable states. | Broker/UI integration, refresh/cancel/partial-failure behavior and detail fixtures. Owner: usage + UI. |
| P05 | Global defaults, workspace defaults/authorization and one-launch override have deterministic precedence. | Resolver tests and CLI/Console equivalence scenarios. Owner: accounts + runtime. |
| P06 | One container admits precisely Claude A, Claude B and Codex C; real TUIs show correct identities; D is inaccessible. | Real target-Mac OrbStack run, process identity, mount/staging inventory and negative canaries. Owner: runtime + verification. |
| P07 | One Kimi identity routes through supported clients without fabricated duplicate quota; exact OpenRouter model ID persists and never silently changes. | Source-backed capability rows, saved/reloaded config, launch/restore and provider fixture/live comparison. Owner: providers + accounts. |
| P08 | Tabs/sessions/restore preserve actual provider/account/model identity through splits, reconnect and account changes. | Session lifecycle, restore/reconnect and manifest-revision tests plus target-Mac smoke. Owner: runtime + UI. |
| P09 | Full requested agent/service catalog and all additional entries in client stores are represented; eligible routes/usage sources are proven or explicitly blocked with evidence. | Reconciled catalog-to-support ledger, current official sources, installed client/version, launch/usage proof. Owner: providers. |
| P10 | Native/shared protocol surfaces remain consistent and final work is independently reviewed and accurately handed off. | Native bridge/desktop gates, reviewer record and exact-SHA evidence summary. Owner: CI + verification + independent reviewer. |

Requested coverage includes Claude Code, Codex, Amp, Antigravity, Kimi Code, Z.AI, Muse Code, Cursor Agent, Grok Build, OpenRouter, omp, Hermes TUI, OpenCode, Gemini CLI as separate client/discovery source, and MiniMax. Z.AI/OpenRouter are billing/routing services through supported clients, not standalone TUI binaries. This list is not a filter: inventory every provider in registered stores. No live account/provider result is asserted here.

## Full A01–H12 checklist crosswalk

Definitions are in jackin-implementation-and-verification.md §4. Every ID below remains UNVERIFIED against the final candidate unless a row is completed with qualifying evidence. References identify requirements; they do not prove implementation.

| Requirement IDs | Contract area / obligation family | Current result and required evidence owner |
|---|---|---|
| A01–A06 | Default bootstrap; empty-config initialization; interrupted-write retry/idempotence; removals stay removed; later scans preserve choices; no-add/partial-source outcomes. | UNVERIFIED. Fresh/install-state matrix, retry/concurrency/partial-source tests, settings snapshot. Owner: accounts. |
| A07–A12 | Distinct source outcomes; bounded/malformed/unreadable/symlink/path safety; supported dotfiles/Amp roots; static-only zsh import; never execute shell/login/model prompts; wrappers deduplicate while retaining execution preferences. | UNVERIFIED. Fixture/process canaries, filesystem boundary tests, byte checks. Owner: accounts + credential review. |
| A13–A17 | Kimi schema/layout compatibility; OpenCode/omp/Hermes provider enumeration; source idempotence and scope-aware keys; stable IDs across mutations; subject change invalidates old identity/cache. | UNVERIFIED. Versioned store fixtures and identity/cache lifecycle tests. Owner: accounts + providers. |
| A18–A23 | macOS keychain lock/absence; environment references; removal preserves host data/reports impacts; pre-sentinel upgrade safety; concurrent scans merge; scanning has no shell/helper writes. | UNVERIFIED. Native keychain cases, migration/concurrency tests, source-tree and shell canaries. Owner: accounts + verification. |
| B01–B06 | Add/edit credential modes; separate inference vs usage permission; secret redaction; positive agent/provider capability gates before transmission; Kimi route identity; official route/protocol restrictions. | UNVERIFIED. Settings integration, secret canaries, negative preflight and sourced route matrix. Owner: accounts + providers + credential review. |
| B07–B12 | MiniMax Token Plan/PAYG distinction; exact OpenRouter model persistence and rejection; shared quota across model presets; disabled account behavior; billing credential scope/separation. | UNVERIFIED. Config round-trip, route validation, permission-scope tests, adapter fixtures. Owner: providers + accounts. |
| B13–B16 | Keyboard/focus/cancel/persistence; scans preserve dirty edits and safely merge/conflict; uncatalogued explicit model remains unverified unless authoritatively rejected; complete no-fixed-list provider inventory. | UNVERIFIED. UI tests, concurrent edit scenarios, model catalog tests and reconciled inventory. Owner: UI + accounts + providers. |
| C01–C08 | Global/workspace/role/one-launch precedence; deterministic picker only when needed; workspace authorization distinct from defaults; explicit missing/empty selection semantics; never ambient fallback. | UNVERIFIED. Resolver contract tests and CLI/Console parity. Owner: accounts + runtime. |
| C09–C14 | Exactly 2 Claude + 1 Codex admission; excluded D absent; new-launch entrypoint parity; defaults cannot mutate running manifest; tabs select only admitted configs; account-set changes use explicit update/restart/new-container path. | UNVERIFIED. Manifest assertions, real launch path and container integration. Owner: runtime + verification. |
| C15–C18 | Restore detects account/credential revision; invalid inherited defaults filtered atomically; unrelated D edits do not invalidate A/B/C reuse; disable/removal denies new grants without claiming revocation of materialized credentials. | UNVERIFIED. Restore/reuse, authorization and active-instance lifecycle tests. Owner: runtime + credential review. |
| D01–D07 | Per-instance Claude/Codex contexts; Amp XDG roots and refreshable auth; OpenCode XDG auth isolation; selected-only JSON/SQLite staging; omp broker is not auth boundary; Hermes state/refresh-token ownership. | UNVERIFIED; security-sensitive. Process identity/canary tests in actual capsule; inspect mount and staged-store inventory. Owner: runtime + credential review. |
| D08–D12 | Antigravity private keyring/target Linux mode; Cursor actual state isolation; Muse HOME/handshake and API-key sanitization; Grok subscription/API precedence; ambient auth/model/env cannot override selected account. | UNVERIFIED. Target-runtime proof or explicit capability blocker; negative ambient-override tests. Owner: providers + runtime. |
| D13–D18 | No secrets in argv/labels/image/inspect/errors; least-auth staging/no host-home/keychain mount; one OAuth refresh writer; scoped revocation/cache invalidation; no late resurrection; Linux amd64/arm64 installer/version/digest evidence. | BLOCKED pending auth leaf-symlink/CAS finding resolution; otherwise UNVERIFIED. Canary, race/CAS, expiry and architecture tests. Owner: credential review + runtime + CI. |
| D19–D22 | Each requested TUI interactive and cleans up; no real secrets in fixtures/docs/reports; copied refresh grants coordinate by lineage without merging merely shared billing identity; explicit missing/expired profiles fail closed. | UNVERIFIED. Actual TUIs, sanitized-fixture scan, refresh-lineage races and no-fallback tests. Owner: runtime + providers + credential review. |
| E01–E04 | Correct account labels; session metadata survives lifecycle; custom titles retain account identity; pane Usage selects actual provider/account including routed clients. | UNVERIFIED. Console/Capsule session lifecycle and route-selection integration. Owner: UI + usage. |
| E05–E08 | Capsule queries only admitted capabilities; usage availability does not control launch; selected accounts remain separately addressed; unused but readable accounts can be monitored. | BLOCKED by projection/consumer gaps and broker lease review finding; targeted capability isolation, cache-key and launch-independence tests required. Owner: usage + runtime + UI. |
| F01–F10 | Concurrent provider windows; duration semantics; percent-used/remaining DTO fidelity; denominator-less balance; distinct states; numeric edge cases; reset/expiry/renewal separation; no synthetic reset replenishment; API totals not subscription allowance; no double-counted shared pools. | UNVERIFIED. Independent semantic fixtures for each DTO and Console/Capsule mapping. Owner: providers + usage + UI. |
| F11–F20 | Claude, Codex, Amp, Antigravity, Gemini, Kimi, Z.AI, MiniMax, Cursor and Grok field/identity/error/period fixtures as specified. | UNVERIFIED. Reviewed fixtures with independent expected values and authoritative source/live version notes. Owner: providers. |
| F21–F29 | Muse; OpenRouter; OpenCode Go; omp/Hermes attribution; partial enrichment; freshness and scope; cross-account cache/history isolation; deduplication; explicit named edge-case scopes. | UNVERIFIED. Reconcile support ledger, fix stale OpenRouter row, add semantic fixtures and identity/scope isolation tests. Owner: providers + usage. |
| G01–G05 | Immediate inventory; freshness/no duplicate refresh; periodic refresh; idle/wake policy; manual refresh joins work and shares Retry-After. | BLOCKED pending broker lease finding review; otherwise UNVERIFIED. Deterministic broker integration and process timing evidence. Owner: usage. |
| G06–G11 | 2/20-client single-flight; distinct account concurrency; screen subscription lifecycle; timeout/cancel ownership; 401/403/429/5xx/malformed/offline recovery; broker owner-loss/restart/corrupt-state recovery. | BLOCKED pending lease issue resolution; mandatory usage_broker_e2e also unverified. Owner: usage + verification. |
| G12–G18 | Stable selection on account edits; every UI state; terminal size/Unicode/focus; 50-account latency; no provider calls from render adapters; Console/CLI/Capsule/native parity. | UNVERIFIED; current Console drops metric_groups and Capsule has no consumer. UI mapping/projection and broker call-inventory tests required. Owner: UI + usage + CI. |
| H01–H04 | Safe config conversion; atomic/idempotent schema migration and obsolete path removal; protocol mismatch error; all affected unit/integration/format/lint/docs/snapshot gates. | UNVERIFIED on final candidate. Migration fixtures plus GOAL commands after merge commit. Owner: accounts + CI. |
| H05–H09 | Mandatory Apple Silicon macOS 26 + OrbStack E2E with matched tests/JUnit; live account identity/fields; real central 3-account container; every available actual client TUI; provider field comparison to native/dashboard with bounded timestamps. | UNVERIFIED. Available host is macOS 27 and cannot satisfy exact OS gate. Run on macOS 26 target and record redacted artifacts. Owner: verification + providers. |
| H10–H12 | Independent evidence review; precise source-backed capability limitations; handoff names exact SHA, commands, environment, artifacts, live coverage and blockers. | UNVERIFIED. No formal review at inspected PR head; final ledger/handoff must be updated after gates. Owner: independent reviewer + orchestrator. |

## Mandatory final gates and dependency order

Do not call the branch complete until applicable GOAL gates run on the final merged candidate and outcomes are recorded. Required GOAL commands:

~~~sh
test "$(git branch --show-current)" = "feat/multi-account-support"
mise install
cargo nextest run -p jackin-core -p jackin-instance -p jackin-config -p jackin-env -p jackin-protocol
cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-runtime -p jackin-console -p jackin-capsule
cargo xtask ci --fast
cargo xtask ci
cargo xtask ci --e2e
cargo xtask roadmap audit
cargo xtask docs repo-links
cargo xtask research check
mise run desktop-ci
mise run desktop-merge
~~~

TESTING.md additionally requires the usage-broker E2E on Apple Silicon macOS 26 with OrbStack, usage_broker_e2e matched tests greater than zero, and JUnit under target/nextest/docker-e2e/. Native lanes require an actual logged-in Mac. Every manual jackin invocation uses --debug.

Dependency graph:
1. Resolve main merge and generated CI state; produce one clean final candidate SHA.
2. Resolve auth publication/symlink/CAS and broker pre-discovery lease findings; add regressions. These block credential and broker confidence.
3. Complete per-agent account identity, authorization/admission, selected-only staging and restore paths; these block the central container/live scenario.
4. Preserve canonical metric groups/freshness/account identity through Console and Capsule; use broker-owned asynchronous periodic refresh; these block usage UI and E05/G01–G18.
5. Reconcile every provider-store entry, support matrix and source-backed field claim; add semantic fixtures before authenticated live checks.
6. Run full unit/integration/CI/docs gates on exact commit, then required macOS 26 OrbStack E2E, native desktop gates and redacted live/client/container matrix.
7. Independent reviewer checks implementation and evidence; update H10–H12 with exact command, environment, result, artifact, owner and unresolved blockers.

Historical proof references remain leads only. The sole durable current-run record is plans/multi-account/evidence/2026-09-21-main-sync.md with its explicit merge-tree caveat.

