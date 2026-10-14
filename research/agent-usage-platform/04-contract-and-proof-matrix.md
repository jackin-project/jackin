# 04 — Contract and proof matrix

Vetted: 2026-08-20
Questions: Which contract details must be frozen before implementation, which directions remain evidence-compatible, and what exact proofs distinguish them?
Informs: unified-agent-usage
Method: synthesis of the three vetted chapters plus targeted verification of newly surfaced broker-catalog and release-proof paths

## Why a matrix is required

The codebase already contains several nearly canonical seams, but none is the complete settled contract. The desktop projection omits OpenCode and has no serialized schema version, host operations inconsistently reuse the distinct desktop order, identity can depend on source ordinals or visible labels, host CLI failure is all-or-nothing, and live broker attachment can discard an updated executor/catalog. Treating those implementations as the specification would preserve the bug classes this work is intended to remove. — `native/AGENTS.md:28-30`, `crates/jackin-usage/src/host.rs:75-98`, `crates/jackin-usage/src/host.rs:467-479`, `crates/jackin-usage/src/host.rs:802-835`, `crates/jackin/src/cli/usage.rs:228-265`, `crates/jackin-usage/src/host/broker.rs:544-558`, [codebase findings](01-codebase-architecture.md) (confidence: HIGH)

The reference projects prove useful mechanisms but not their combination: terminal OpenUsage proves a central socket daemon, CodexBar proves in-process coalescing, native OpenUsage proves shared serializers, and TermRock proves deterministic render primitives. None proves the exact eight-provider host graph, seven-provider desktop filter, cross-process generation joining, or anonymous-account identity required here. — [reference findings](03-reference-implementations.md) (confidence: HIGH)

## Canonical projection contract candidates

The following candidate is the minimum complete V1 direction surfaced by the evidence. It is not selected by this research artifact; planning must freeze or replace every row before source changes begin.

| Area | Candidate V1 rule | Evidence pressure and trade-off |
|---|---|---|
| Envelope | `schema_version`, immutable `projection_id`, `generated_at`, discovery revision, broker instance/generation, ordered providers, structured global issues | Existing desktop generation is not a schema version and generic CLI JSON wraps different payloads. More metadata makes freshness and compatibility explicit but increases fixture surface. — `crates/jackin-usage/src/host.rs:467-479`, `crates/jackin/src/cli/format.rs:14-41` |
| Provider | Stable surface ID, display label, contract-specific order, membership state, refresh/freshness state, accounts, structured provider issues | Host and desktop have distinct settled orders, while global abort loses partial-provider output. Tagged state prevents text parsing but freezes public enum semantics. — `native/AGENTS.md:28-30`, `crates/jackin-usage/src/host.rs:75-98`, `crates/jackin/src/cli/usage.rs:228-265` |
| Canonical account | Provider-scoped canonical ID, identity evidence kind, display label, optional plan/status, provenance count, lifecycle, freshness, ordered windows, structured account issue | Current descriptors summarize one limit and can merge by visible labels. Carrying evidence state enables safe deduplication but exposes a larger internal-to-wire contract. — `crates/jackin-usage/src/host/accounts.rs:19-57`, `crates/jackin-usage/src/host/accounts.rs:104-134` |
| Quota window | Stable row ID, Rust-owned label/value/reset strings, semantic remaining/used kind, optional bounded numeric meter, optional reset instant, quota state | FFI currently preserves Rust strings, while flat CLI repeats account identity for each window. Stable nested windows prevent duplicate account rows and support deterministic selection. — `native/Sources/JackinUsageBridge/OverviewInventory.swift:6-58`, `crates/jackin/src/cli/usage.rs:403-424` |
| Issues | Stable code, scope, recoverability, display message, optional retry instant; secret-bearing provider text never crosses the sanitized boundary | Current failures are split between a global string, diagnostics, account errors, and command aborts. Structured issues improve recovery and JSON consumers but require an explicit redaction registry. — `crates/jackin-usage/src/host.rs:1207-1303`, `crates/jackin-usage/src/host/accounts.rs:104-134`, `crates/jackin/src/cli/usage.rs:228-265` |
| Evolution | Consumers reject an unknown major version; V1 permits only documented optional field additions; removal, type/meaning change, ordering change, or enum-case reinterpretation increments the major fixture set | The current generic envelope says `v1` without versioning the projection it carries. Strict major handling avoids silent semantic drift at the cost of coordinated adapter updates. — `crates/jackin/src/cli/format.rs:14-41`, `crates/jackin-usage/src/host.rs:467-479` |

### Identity and merge decision matrix

| Evidence | Candidate behavior | Required proof |
|---|---|---|
| Provider-supplied immutable non-secret account ID | Canonical key is a domain-separated hash of provider plus normalized ID; equal exact IDs merge | Same account through all discovery scopes yields one row; different providers never merge. |
| Provider-supplied stable non-secret handle but no immutable ID | Keep typed handle evidence; merge only under provider-specific normalization documented by that adapter | Case, Unicode, tenant/domain, and duplicate-label fixtures prove no accidental alias. |
| Configuration provenance before authentication | Create an ephemeral typed capability identity, never a durable canonical account ID | Source reordering and process restart do not change a displayed canonical row because no canonical row exists until stronger evidence arrives. |
| Multiple unresolved capabilities | Roll them into one provider-level `resolving configurations` state rather than displaying guessed account rows | Every current configuration remains counted/diagnosable without showing duplicate or falsely merged accounts. |
| Stronger identity arrives | Atomically add an alias from provisional capability to canonical ID, merge snapshots by explicit freshness precedence, move selection, and retire provisional state | Crash-at-each-step and replay tests yield one canonical row, one durable state file, and stable selection. |
| No stable identity after a provider attempt | Remain explicitly unresolved; do not merge by label, source ordinal, or token material | Surface remains available with an explanation; no secret, digest of secret, source path, or ordinal reaches projection output. |
| Provider ID and stable handle contain identical bytes | Domain-separate provider, evidence kind, normalization version, and normalized value before deriving any key | Equal bytes under different evidence kinds cannot alias accidentally; an adapter-supplied alias is required to merge them. |
| Normalization changes | Persist the normalization algorithm version with identity evidence; never reinterpret an old key silently | Upgrade fixtures prove either an explicit alias transaction or a clean unresolved transition without duplicate canonical rows. |
| Derived-key collision with unequal full evidence | Treat the key as an index, compare the complete typed non-secret evidence, and fail closed on inequality | Forced-collision fixture performs no merge, provider call, or overwrite and emits a typed identity issue. |
| Equal visible labels with unequal evidence | Never merge by the label | Case-only, Unicode-normalization, tenant, and duplicate-label fixtures remain distinct after authentication. |
| Equal canonical evidence with conflicting display fields | Merge identity, then apply a separately frozen field-precedence table for label, plan/status, timestamps, errors, provenance, and windows | Permuted discovery order and equal-timestamp fixtures produce byte-identical projections. |
| Equal window stable IDs with conflicting payloads | Apply the frozen generation/freshness precedence or emit a typed collision; never depend on iteration order | Permuted input and replay fixtures preserve deterministic values and window order. |

The settled unresolved-capability outcome sacrifices pre-authenticated account labels instead of fabricating deduplication: unresolved configuration state stays outside canonical account rows. The representation and later alias mechanism remain planning choices. Displaying one row per configuration would violate the settled no-duplicate-account experience, while merging by visible label can silently conflate distinct accounts. Provider-specific evidence must decide whether and when a capability can graduate to canonical identity. — [roadmap disposition](../../roadmap/unified-agent-usage/README.md#open-research-questions), `crates/jackin-usage/src/host/discovery.rs:811-854`, `crates/jackin-usage/src/host/accounts.rs:19-57`, [identity unknowns](03-reference-implementations.md#open-unknowns) (confidence: HIGH for settled outcome and current risks; representation remains a planning direction)

### Deterministic ordering and partial output

The settled host order is Claude, Codex, Amp, Grok, Kimi, OpenCode, Z.AI, then MiniMax. The frozen desktop order is Codex, Claude, Amp, Grok, Z.AI, Kimi, then MiniMax; it excludes OpenCode but intentionally does not preserve host-relative order. Planning must encode both contracts explicitly and prevent host operations from reusing the desktop array. Providers can contain canonical accounts sorted by an explicit Rust order key, then locale-independent normalized display label, then canonical ID; accounts can contain windows in provider-defined Rust order, then stable row ID. Presentation adapters must not sort. A provider failure produces its provider record with last-good accounts when available plus structured issues; usable providers remain present and the human command exits zero. A global nonzero result occurs only when invocation is invalid or no usable projection can be produced. These constraints and candidate account/window sorts require frozen cross-surface fixtures. — `native/AGENTS.md:28-30`, [roadmap decisions](../../roadmap/unified-agent-usage/README.md#decisions), `crates/jackin-usage/src/host.rs:75-98`, `crates/jackin-usage/src/host/accounts.rs:158-188` (confidence: HIGH for settled host/desktop orders and partial-output constraint; account/window sorts remain planning directions)

For the command contract, a usable projection is a schema-valid broker envelope
whose current-membership evaluation completed and that can truthfully describe
the inventory, even when the inventory is empty or contains only unresolved
configuration state. Empty inventory and unresolved-only inventory therefore
render explicit human/JSON states and exit zero. Any current or stale last-good
quota row also makes a partial projection usable and exits zero with structured
issues. If every current member failed and no last-good row exists, human output
still renders the failures and JSON still emits the schema-valid issue envelope,
but the command exits nonzero. Invalid invocation, broker transport failure,
schema mismatch, or projection-construction failure also exits nonzero. This
definition is a normative planning input derived from the settled empty,
partial-failure, and exit decisions rather than current CLI behavior. — [roadmap
decisions](../../roadmap/unified-agent-usage/README.md#decisions), [CLI screen
contract](../../roadmap/unified-agent-usage/README.md#cli-usage-output)

### Capsule membership and lifecycle contract

Capsule presentation membership is not the host inventory or the fixed provider
catalog. It is exactly the agent set in the current fully resolved instance
launch configuration. Global discovery, unresolved configuration, historical
membership, or a usage capability alone cannot add a row. A resolved usage
capability may enrich an eligible agent with canonical-account quota previews;
without one, the eligible agent remains visible with an explicit no-preview
explanation.

An eligible agent has lifecycle `initialized` only after at least one session has
started in that Capsule. Until then, the row carries the typed issue code
`agent_uninitialized`. That issue is an agent-scoped lifecycle error, not a
provider-refresh failure. A successful quota preview may coexist with it but
cannot clear, downgrade, or replace it. Starting the first session clears only
the lifecycle error; it does not rewrite quota freshness or provider issues.
Neither lifecycle nor quota state authorizes or blocks launch.

Required proof fixtures cover: fixed/global provider excluded; unresolved agent
excluded; capability-only provider excluded; resolved agent without capability;
resolved uninitialized agent with preview; initialized agent; multiple canonical
accounts; launch-config add/remove/reorder; selection retained across first
session; and simultaneous lifecycle plus stale/refresh failure. Every surface
adapter must preserve the structured issue code and Rust-owned preview strings.
— [roadmap decisions](../../roadmap/unified-agent-usage/README.md#decisions),
`crates/jackin-runtime/src/usage_relay.rs:189-215`,
`crates/jackin-runtime/src/usage_relay.rs:385-419`,
`crates/jackin-usage/src/usage/view.rs:470-489`

## Broker policy matrix

Planning must choose exact mechanisms and numeric values where the evidence does
not. It must not inherit either accidentally from current implementation.

| Operation or transition | Evidence-backed outcome | Candidate mechanism and planning choice/proof |
|---|---|---|
| Current read | One authority serves current or last-good state without unnecessary provider work. | Broker transport is the leading candidate; freeze maximum read latency and pre-first-snapshot state. |
| Ambient refresh | One canonical capability has at most one active generation and all concurrent subscribers observe it. | Broker-owned due-time policy; freeze success TTL and whether adapters may declare longer TTLs. |
| Forced refresh | An active generation is joined and duplicate post-flight work is not queued. | One broker refresh operation; freeze manual refresh floor and abuse limit. |
| Admission and queueing | Bounded overload cannot create a second authority, starve one provider indefinitely, or turn rejected work into hidden retries. | Freeze connection/generation queue capacities, full-queue result, retry ownership, and per-provider fairness; current baselines are 128 connections and 256 generations. — `crates/jackin-usage/src/host/broker.rs:126-133`, `crates/jackin-usage/src/coordinator.rs:60-80` |
| Provider concurrency | Parallelism remains bounded while distinct accounts make progress and one blocked provider cannot consume every worker. | Freeze global and per-provider limits, scheduling order, and starvation proof; current baseline is four provider workers. — `crates/jackin-usage/src/coordinator.rs:60-80` |
| No-refresh | Read-only intent cannot become a second freshness authority or bypass retry state. | Broker current-state operation is the leading candidate; freeze missing-state result. |
| Discovery update | A live authority reflects current membership and cannot retain the first caller's stale catalog/resolver. | Candidate: versioned register/update/revoke transaction. Freeze catalog-revision source, conflict behavior, and alternate mechanism if the candidate is rejected. |
| Membership revocation | A removed current-discovery member disappears from every live projection without reviving through history. | Freeze cancellation of queued/in-flight work, durable last-good retention or deletion, alias cleanup, and re-add behavior. |
| Credential rotation | A live authority applies the current credential binding without parallel resolver ownership. | Candidate: binding update coupled to catalog revision. Freeze old in-flight generation completion/cancellation behavior. |
| Success | Last-good state and next eligibility survive caller exit and broker restart consistently. | Candidate: one atomic durable transaction. Current TTL is five minutes; retain or replace explicitly. |
| Rate limit | Last-good survives and provider retry time prevents hammering. | Honor provider retry instant; candidate local sequence is the current 5, 10, 20, then 30 minutes. Retain or replace explicitly. |
| Rate-limit recovery | A successful generation and materially newer provider deadline cannot leave obsolete exponential state active. | Freeze failure-count reset, provider-deadline replacement, clock-skew handling, and replay proof. |
| Other transient failure | Repeated callers cannot create an unbounded retry loop. | Candidate bounded exponential backoff; freeze initial delay, multiplier, cap, jitter, and reset condition. |
| Authentication/permission failure | Last-good survives and polling does not repeatedly prompt or fail until relevant state changes. | Candidate invalidation-driven retry plus explicit retry; freeze invalidation events and recovery copy. |
| Cancellation | One subscriber leaving cannot start replacement work; provider work cannot block the broker indefinitely. | Candidate waiter removal plus abortable provider deadline. Freeze hard timeout and last-subscriber cancellation policy. |
| Leader ownership | PID reuse or stale owner metadata cannot suppress safe takeover or admit two authorities. | Candidates include instance token, process-birth evidence, and renewable lease. Freeze mechanism, renewal, and stale threshold through adversarial tests. |
| Persistence/crash | Last-good, retry policy, catalog identity, and interrupted-generation recovery are transactional and replay deterministically. | Candidates include atomic files or one transactional store. Freeze storage, schema/version rule, fsync boundary, and recovery behavior. |
| Last-good retention | Stale data remains explicitly aged and useful without becoming permanent current membership. | Freeze maximum age or explicit no-expiry policy, cleanup trigger, disk bound, revoked-member handling, and unavailable transition. |
| Protocol mismatch | Incompatible clients perform no provider call and cannot silently replace a healthy authority. | Candidate typed incompatibility plus explicit upgrade/shutdown negotiation. Freeze executable lookup and supervision behavior. |

Current behavior supplies useful baseline values but not a complete contract: success cooldown defaults to five minutes, only rate-limit failures receive local exponential backoff, the 20-second threshold is post-hoc rather than abortive, and no wire cancellation exists. — `crates/jackin-usage/src/coordinator.rs:24-26`, `crates/jackin-usage/src/coordinator.rs:60-80`, `crates/jackin-usage/src/coordinator.rs:382-430`, `crates/jackin-usage/src/coordinator.rs:623-704`, `crates/jackin-protocol/src/usage_broker.rs:107-159` (confidence: HIGH)

## Broker direction proof

Before choosing resident or demand-activated lifetime, run the same isolated spike against both candidates:

1. Start 20 clients concurrently against no endpoint; assert one authority instance, one logical catalog revision, and one provider call per canonical capability.
2. Exit the activating client during an in-flight refresh; assert the broker and generation survive and every waiter receives the same terminal generation.
3. Add, remove, and rotate a configuration while the authority lives; assert the selected catalog-update mechanism changes authoritative membership/bindings and the first owner's resolver is not retained accidentally.
4. Reuse a stale guard PID, crash during queued/updating persistence, restart, and assert safe lease takeover plus one recovery generation.
5. Connect old/new protocol clients; assert mismatch performs zero provider calls and cannot replace a healthy incompatible service silently.
6. Cancel one and then all subscribers; assert no duplicate replacement generation and the selected provider-cancellation policy.
7. Run CLI, console, Capsule relay, and desktop together; assert shared generations, deadlines, backoff, and last-good projection.

The resident direction additionally proves explicit startup/shutdown and idle resource behavior. The demand-activated direction additionally proves concurrent activation and endpoint readiness. If both satisfy every invariant, detailed planning owns the final selection and records the comparative results, lifecycle-state surface, startup behavior, and host-registration requirement; this research artifact does not choose. Installing a persistent host service during development is not implicit permission for a host write; tests must use isolated workspace-owned state unless the operator explicitly opts into installation. — [broker candidate directions](README.md#candidate-broker-directions), `crates/jackin-usage/src/host/broker/tests.rs:152-260` (confidence: HIGH for current test gap; candidate proof is prescriptive)

## Native empirical acceptance tasks

The final running app must complete, not merely render, these tasks:

- Keyboard-open each provider status item, move through popover controls, switch canonical account, refresh, open Usage, and verify exact provider/account handoff without trapped or lost focus.
- On a minimum topology of built-in 2×, external 1×, and external 2× displays, with per-display menu bars where the system permits, click each AppKit `NSStatusItem` and verify its `NSPopover` anchors to that item; open the unique Usage window, move/resize it, close/reopen, disconnect a display, and verify safe visible placement plus restored selection/sidebar state.
- With VoiceOver, identify provider, account, plan, value, reset, freshness/error, and Retry; move from status item through popover into Usage and Settings; confirm refresh completion and asynchronously replaced rows are understandable without duplicate announcements.
- With Full Keyboard Access, complete every safe action; with Increase Contrast, Reduce Transparency, Reduce Motion, and Differentiate Without Color, preserve row relationships, focus, non-color state, opaque fallback, and motion policy.
- After human structural selection, freeze the draft design fixture catalog against the already-frozen desktop order; then at 760 × 500, 920 × 620, and 1200 × 760 complete every fixture, including 2× English, mixed right-to-left/LTR, CJK, German, 40 accounts, inactive/key windows, scrollbar variants, accent colors, icon sizes, wallpapers, display scales, and color profiles. — `native/AGENTS.md:28-30`, `native/Design/UnifiedAgentUsage/Fixtures.md:3-20`, `native/Design/UnifiedAgentUsage/Fixtures.md:203-212`, `native/Design/UnifiedAgentUsage/Fixtures.md:250-330`, `native/Design/UnifiedAgentUsage/ExperienceBrief.md:229-243`, `native/Design/UnifiedAgentUsage/BaselineVisualQA.md:71-82`

Apple does not define the physical-display placement behavior required for the current AppKit `NSStatusItem` plus `NSPopover` architecture, a dedicated stale-data component, or a quota-table announcement API; reference apps do not prove end-to-end VoiceOver for this workflow. These are empirical gates, not assumptions. — `native/Sources/JackinDesktop/DesktopAppDelegate.swift:9-34`, [Apple open unknowns](02-apple-native-design.md#open-unknowns), [reference open unknowns](03-reference-implementations.md#open-unknowns) (confidence: HIGH)

## Distribution proof chain

The release gate must bind one public artifact digest through every step:

1. Build the release app, Developer ID-sign it with hardened runtime and timestamp, verify nested signatures, create a temporary ZIP for notarization submission, wait for acceptance, and staple the signed app.
2. Create the final immutable public ZIP from that stapled app, compute exactly one SHA-256 over the final ZIP bytes, publish those unchanged bytes at a versioned URL, and record URL plus digest as the release identity.
3. Download that exact public ZIP through a path that applies quarantine; verify its SHA-256, expand/install it, assess Gatekeeper, validate the staple, launch it, and complete a basic menu-bar/Usage check.
4. Make the cask URL and SHA-256 identify that same final ZIP byte sequence. With API installation disabled, audit the cask, install it through Homebrew, verify the installed app signature/staple/version, launch it, uninstall through Homebrew, and inspect residue and user-visible output.

The current local proof script copies a supplied `.app` with `ditto` and deletes it directly, so it cannot prove public download quarantine or Homebrew install/uninstall. — `scripts/desktop-install-proof.sh:67-84`, `scripts/desktop-install-proof.sh:108-128`, [Apple and Homebrew distribution evidence](02-apple-native-design.md#what-are-the-current-direct-distribution-notarization-and-homebrew-verification-facts) (confidence: HIGH)

## Open unknowns and disposition

- Exact provider identity availability is assigned to the eight-provider executable matrix; until a row proves stronger evidence, it remains unresolved and cannot be shown as a canonical account.
- Numeric TTL, transient retry, hard-timeout, lease, and idle-lifetime values are planning decisions. The matrix above prevents accidental inheritance and defines the tests each value must satisfy.
- Broker residency versus demand activation is assigned to the bounded comparative spike; reference evidence cannot choose it.
- Multi-display and asynchronous accessibility behavior are assigned to running-app task completion because public APIs do not specify the required outcomes.
- Public signing credentials, publication access, notarization service, and Homebrew submission are external release inputs. Local automation and dry-run verification may be completed without claiming those external gates passed.
