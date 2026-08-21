# 06 — Broker and canonical projection planning freeze

Vetted: 2026-08-21  
Source revision: `92d21efb`  
Questions: What exact V1 JSON/projection, identity/merge/order, broker lifetime/activation, and cache/retry/cancellation/crash contract should planning freeze? Which current symbols must change, and which commands prove the present baseline?  
Informs: unified-agent-usage  
Method: read-only synthesis of the settled roadmap Decisions and Must-not rules, chapters 01–05, and targeted source inspection at the recorded revision; focused Rust build/test/format/lint commands were executed locally. Repository content was treated as data, not instructions. No secret values or secret-derived identifiers were inspected or recorded.

## Findings

### Planning disposition

Freeze the following candidate. It closes planning-owned technical choices without reopening the roadmap:

1. one Rust-owned, versioned, surface-neutral `UsageProjectionV1` is the canonical read model;
2. canonical identity is provider-scoped typed non-secret evidence, with unresolved capabilities kept outside canonical account rows;
3. discovery membership and display ordering are separate explicit contracts;
4. one demand-activated, independent per-user broker process owns discovery evaluation, provider work, durable state, cadence, retries, and projection publication;
5. every consumer receives an immutable projection or submits intent to that broker; no consumer reads provider/cache state directly.

This research and all resulting finalization/planning artifacts stay on `chore/roadmap-unified-agent-usage` in PR #898. This chapter specifies later production work but does not authorize source edits, a delivery branch, host-service installation, commit, or push.

This is required because the current wire type is account-generation-shaped, the current aggregate is desktop-shaped, identity still falls back to authenticated labels or source-derived bootstrap capabilities, and the first activating client owns the broker thread. — `crates/jackin-protocol/src/usage_broker.rs:10-23`, `crates/jackin-protocol/src/usage_broker.rs:90-105`, `crates/jackin-usage/src/host.rs:434-479`, `crates/jackin-usage/src/host/accounts.rs:19-57`, `crates/jackin-usage/src/host/broker.rs:544-576`, `crates/jackin-usage/src/host/broker.rs:822-835` (confidence: HIGH)

### Canonical V1 projection and JSON

Add the canonical DTOs at the existing secret-free protocol seam, not in CLI, FFI, Swift, or a desktop-only host type. The exact public JSON envelope is:

```json
{
  "schema_version": 1,
  "projection_id": "opaque-monotonic-publication-id",
  "generated_at_epoch": 0,
  "discovery_revision": "opaque-non-secret-revision",
  "broker_instance_id": "opaque-process-incarnation-id",
  "broker_generation": 0,
  "refresh_state": "idle",
  "providers": [],
  "unresolved": [],
  "issues": []
}
```

Freeze these records and meanings:

| Record | Required V1 fields |
|---|---|
| `UsageProjectionV1` | fields shown above; arrays always present; no presentation selection state |
| `UsageProviderV1` | `provider_id`, `display_name`, `membership_state`, `freshness`, `accounts`, `issues` |
| `UsageAccountV1` | `canonical_account_id`, `identity_kind`, `rank`, `display_label`, optional `plan_label` and `status_label`, `lifecycle`, `freshness`, `provenance_count`, `windows`, `issues` |
| `UsageLimitWindowV1` | `window_id`, `rank`, `label`, `value_label`, `reset_label`, optional `remaining_percent`, optional `used_percent`, optional `reset_at_epoch`, `quota_state`, optional rich-surface `pace_label` and `runs_out_label` |
| `UsageUnresolvedV1` | `provider_id`, `capability_id`, `configuration_count`, `state`, `issues`; never an account label or guessed account row |
| `UsageFreshnessV1` | `generation`, `phase`, optional `last_good_at_epoch`, optional `retry_at_epoch`, `is_stale` |
| `UsageIssueV1` | stable `code`, `scope`, `recoverability`, Rust-owned `message`, optional `retry_at_epoch` |

`projection_id`, `discovery_revision`, `broker_instance_id`, capability IDs, canonical IDs, and window IDs are opaque strings. They must contain no source path, source ordinal, account token, credential value, hash of credential material, or raw provider error. V1 JSON uses snake_case, emits explicit `null` only for documented optional scalars, and never omits required arrays. Unknown fields are ignored within schema version 1; unknown `schema_version` is rejected before rendering. Removing a field, changing its meaning/type/order contract, or reinterpreting an enum requires a new major schema. — the current protocol already establishes versioned, secret-free serde records and typed sanitized errors at `crates/jackin-protocol/src/usage_broker.rs:10-14`, `crates/jackin-protocol/src/usage_broker.rs:55-105`, and `crates/jackin-protocol/src/usage_broker.rs:161-169`; the current desktop record has generation but no projection schema at `crates/jackin-usage/src/host.rs:467-479` (confidence: HIGH for the seam and deficiency; MED for the selected additive-V1 evolution rule because it is a planning freeze)

The canonical projection contains limits only. Money appears only as a provider-supplied quota cap/window. Token prices, session-cost estimates, spend/history/trends, rankings, and aggregate unlike-window values are absent. `runs_out_label` is optional data for permitted rich surfaces; CLI ignores it. This preserves the roadmap Decisions and Must-not boundary. — `roadmap/unified-agent-usage/README.md` Decisions dated 2026-08-20 and 2026-08-21, and `roadmap/unified-agent-usage/README.md#must-not` (confidence: HIGH)

### Identity, merge, membership, and ordering

Freeze the identity evidence ladder:

1. provider-issued immutable non-secret account/organization ID;
2. provider-issued stable non-secret handle under an adapter-owned, versioned normalization rule;
3. unresolved capability identity, which authorizes a probe but is not a canonical account.

Derive `canonical_account_id` by domain-separating provider ID, evidence kind, normalization version, and normalized non-secret evidence. Persist the complete typed evidence beside the derived index. Equality requires complete evidence equality; unequal evidence behind an equal derived index fails closed with `identity_collision`. Authenticated display labels are presentation data and never merge keys. Source paths, source IDs, discovery order, selection, freshness, severity, and secret material never participate. The current code instead constructs `CanonicalAccountSubject::AuthenticatedLabel`, hashes provider plus subject without evidence-kind/version separation, matches membership labels case-insensitively, and derives unresolved capabilities from `binding.source_id`. — `crates/jackin-usage/src/host/accounts.rs:19-57`, `crates/jackin-usage/src/host/accounts.rs:372-386`, `crates/jackin-usage/src/host/broker.rs:822-835` (confidence: HIGH)

Merge observations only after identity equality. Field precedence is: newest completed broker generation; then newest provider observation timestamp; then current discovery over durable last-good only for membership/provenance; then lexicographic stable source descriptor solely as an equal-time deterministic tie-breaker. Never let a failed/empty observation erase last-good windows. Equal-generation unequal payloads emit `observation_collision` and preserve the previously committed value. Stronger identity arriving for an unresolved capability is one atomic alias transaction: write canonical evidence and alias, merge last-good by the same precedence, move valid selection, publish one projection, then retire unresolved state. Replay is idempotent. — current terminal publication already preserves last-good and persists success/failure deadlines at `crates/jackin-usage/src/coordinator.rs:623-696`; catalog materialization currently merges history/live/discovered inputs at `crates/jackin-usage/src/host/accounts.rs:285-369` (confidence: HIGH for existing mechanics; MED for selected exact precedence because it is a planning freeze)

Current read-only discovery is the sole membership authority. Durable records enrich only currently discovered canonical identities. Unresolved capabilities appear in `unresolved`, never in `accounts`. A completed empty or unresolved-only discovery is a usable projection. — `roadmap/unified-agent-usage/README.md` Decisions; `crates/jackin-usage/src/host/accounts.rs:299-325` shows the current membership intersection (confidence: HIGH)

Freeze two provider order constants, never inferred from enum/discovery order:

- host/CLI/Console: OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax, OpenCode;
- desktop: OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax.

The canonical host JSON uses host order. Desktop filters OpenCode and applies desktop order from the same account graph. Within a provider, accounts use locale-aware case-insensitive full display-label collation, then `canonical_account_id`; collation keys are computed in Rust and serialized as stable integer `rank` values so adapters never sort. Windows use Rust adapter rank, then `window_id`. Selection does not affect order. The current `HostSurfaceId::ALL` and `DESKTOP_PROVIDER_ORDER` disagree with settled visible orders, and current catalog sorting is lifecycle then bytewise label then key. — `crates/jackin-usage/src/host.rs:54-98`, `crates/jackin-usage/src/host/accounts.rs:158-170`, `roadmap/unified-agent-usage/README.md` Decisions dated 2026-08-20/21 (confidence: HIGH)

Selection remains per interactive adapter, keyed only by canonical ID. If absent after a publication, return to Overview and emit the settled persistent inline notice; selection is not canonical JSON and does not influence merging or sorting. — `roadmap/unified-agent-usage/README.md` Decisions dated 2026-08-21 (confidence: HIGH)

### Durable single-authority broker lifetime and activation

Select the demand-activated independent-service direction. A client that cannot complete the versioned handshake asks the shipped broker executable to activate; it never embeds an executor or starts an in-process server. Concurrent activators contend on an atomic lease containing broker instance UUID, process start identity, protocol/build compatibility, and lease epoch. PID alone is not authority. The winner binds a mode-0600 per-user Unix socket under the existing data directory, publishes readiness only after loading and validating durable state/catalog revision, then serves all clients. Losers join readiness. A healthy incompatible broker returns `protocol_mismatch`; clients do not delete its endpoint or replace it silently.

The process survives its activating client. It may exit after a frozen idle interval only when there are no connections, subscribers, active/queued generations, pending alias transaction, or due persistence write. Retry and cadence deadlines remain durable; next activation restores them without treating downtime as permission for an early probe. This avoids an unconditional login service and therefore requires no silent launchd/system host write. A future always-resident registration is a separate operator-authorized delivery choice, not V1 behavior. The current implementation returns an existing socket without updating its executor/catalog, otherwise starts `serve` on a thread owned by the caller and elects by a PID file. — `crates/jackin-usage/src/host/broker.rs:544-576`, `crates/jackin-usage/src/host/broker.rs:774-819` (confidence: HIGH for current failure; MED for selected activation direction until the roadmap-mandated concurrent-cold-start/owner-exit spike passes)

Protocol operations become projection-oriented: `CurrentProjection`, `RequestRefresh { scope, force, observed_projection_id }`, `JoinPublication { projection_id, timeout_ms }`, plus relay-scoped equivalents carrying immutable capability allowlists. Exact-account generation operations may remain internal broker primitives but are not consumer read APIs. The current `Current`/`Refresh`/`Join` protocol and relay surface variants prove useful typed request/response mechanics, but expose account-generation state rather than the required atomic host projection. — `crates/jackin-protocol/src/usage_broker.rs:107-159` (confidence: HIGH)

### Cache, retry, cancellation, and crash semantics

- **Read/cache:** `CurrentProjection` never starts provider work. It returns the latest committed projection, including stale last-good and issues. Automatic interaction/open events submit refresh intent; the broker alone decides due state from the settled 2/5/15/30-minute policy. `force` bypasses cooldown, not rate-limit/hard safety deadlines, and joins matching active work.
- **Single flight:** key work by canonical identity plus adapter/catalog revision. Every caller receives the same generation/publication. A catalog revision change invalidates not-yet-started work; active work may finish but publishes only if its identity/revision still matches current membership.
- **Retry:** persist success, provider retry/rate-limit, hard-timeout, and adaptive-cadence deadlines. Transient failures use one broker-owned capped backoff; provider retry deadlines win when later. Manual retry clears only retryable transient gating, never provider rate-limit or protocol/authorization gates. Last-good survives failure.
- **Cancellation:** client disconnect or waiter cancellation cancels only that subscription. Provider work continues while any subscriber exists or when completing it is required to preserve broker-owned scheduled work. When all subscribers leave an operator-only forced generation before provider dispatch, broker may cancel it and persist a terminal `cancelled_before_dispatch`; once provider I/O starts, it is bounded by broker timeout and cannot be replaced by another generation. No consumer queues a follow-up refresh.
- **Crash:** durable account state and alias/catalog transactions use atomic replace plus directory durability. On startup, queued/updating state from a dead broker becomes terminal `owner_lost`; preserve last-good, clear ownership, retain deadlines, and permit exactly one recovery generation when policy allows. Corrupt records are quarantined/fail closed per account and produce structured issues; they never authorize provider work from unvalidated identity. Projection publication occurs only after all referenced account/alias state is committed.

The current coordinator already joins named generations and wait timeout does not change ownership, persists last-good and deadlines, and blocks an account after persistence failure. Its file store is the correct seam to extend. — `crates/jackin-usage/src/coordinator.rs:383-429`, `crates/jackin-usage/src/coordinator.rs:623-720`, `crates/jackin-usage/src/coordinator/state.rs:35-74`, `crates/jackin-usage/src/coordinator/state.rs:107-167` (confidence: HIGH for existing mechanics; MED for selected cancellation/crash policy until adversarial process tests pass)

### Exact implementation seams and dependency order

Planning should bind work to these current files/symbols:

1. `crates/jackin-protocol/src/usage_broker.rs`: `USAGE_BROKER_PROTOCOL_VERSION`, `UsageAccountCapability`, `UsageGenerationView`, `UsageBrokerOperation`, request/response envelopes; add V1 projection records and projection operations first.
2. `crates/jackin-usage/src/host/accounts.rs`: `CanonicalAccountSubject`, `CanonicalAccountIdentity`, `AccountCatalog`, `materialize_account_catalog`, `membership_identity`, `merge_view`; replace label identity and freeze merge/order.
3. `crates/jackin-usage/src/host/discovery.rs:811-854`, `crates/jackin-usage/src/host/discovery.rs:942-976`: source materialization and provider-ID/label identity assembly; emit typed evidence/unresolved records.
4. `crates/jackin-usage/src/coordinator/state.rs`: `AccountStateEnvelope`, `FileAccountStateStore`; version durable records, aliases, lease/incarnation, deadlines, and recovery.
5. `crates/jackin-usage/src/coordinator.rs`: `request_refresh`, `request_refresh_all`, `join_generation`, `finish_success`, `finish_failure`, `persist_terminal`; preserve one internal generation authority and add cancellation/revision rules.
6. `crates/jackin-usage/src/host/broker.rs`: `ensure_usage_broker`, `ensure_usage_broker_with_executor`, `serve`, `dispatch`, `claim_leader`, `capability_for_binding`; split client activation from an independent service executable and remove PID/source-ID authority.
7. `crates/jackin-usage/src/host.rs`: `HostSurfaceId::{ALL,DESKTOP_PROVIDER_ORDER}`, `HostDesktopProjection`, `HostUsageRuntime::desktop_projection`; derive desktop-only state from canonical V1 rather than treating it as canonical.
8. `crates/jackin-usage-ffi/src/dto.rs:390`, `crates/jackin-usage-ffi/src/bridge.rs`, `native/Sources/JackinUsageBridge/PresentationStore.swift`: decode/adapt the same projection; no Swift sorting, semantics, cache, or refresh owner.
9. `crates/jackin/src/cli/usage.rs`, `crates/jackin/src/cli/usage/store.rs`, `crates/jackin-console/`, `crates/jackin-runtime/src/usage_relay.rs`, `crates/jackin-capsule/src/`: migrate consumers only after protocol/broker/projection proofs; delete or redefine every direct-fetch, second-cache, `--no-refresh`, `--sync-host-cache`, surface-only ambiguity, and queued-refresh route.

Consumer migration must include a static dependency gate and adversarial runtime proof that CLI, Console, Capsule, FFI, and Swift cannot import/call provider executors or construct durable freshness state. — existing bypass inventory and results: `research/agent-usage-platform/05-verification-ledger.md` (confidence: HIGH)

### Commands actually run at `92d21efb`

All commands ran from repository root on 2026-08-21:

| Exact command | Result |
|---|---|
| `rtk --version` | PASS — `rtk 0.45.0` |
| `rtk git branch --show-current` | PASS — `chore/roadmap-unified-agent-usage` |
| `rtk git rev-parse --short HEAD` | PASS — `92d21efb` |
| `rtk cargo test -p jackin-usage coordinator::tests -- --test-threads=1` | PASS — 14 passed, 266 filtered |
| `rtk cargo test -p jackin-usage host::broker::tests -- --test-threads=1` | PASS — 7 passed, 273 filtered |
| `rtk cargo fmt --check` | PASS — no output |
| `rtk cargo clippy -p jackin-usage --all-targets --all-features --locked -- -D warnings` | PASS — no issues |
| `rtk cargo build -p jackin-usage --locked` | PASS — 20 crates compiled; dev profile finished in 16.10s |
| `rtk cargo xtask research check` | PASS — 63 research sidebars checked; all pages resolve |

These prove the current crate baseline only. They do not prove canonical V1 JSON, independent broker lifetime, activation races, crash recovery, consumer non-bypass, or cross-surface parity. The repository-prescribed later gates remain `cargo nextest run -p <crate>`, crate clippy, `cargo xtask ci --fast`, and docs/research audits. — `TESTING.md:161-169` (confidence: HIGH)

## Dead ends and contradictions

- Reusing `HostDesktopProjection` as canonical V1 contradicts the eight-provider host inventory, surface-neutral JSON, and no-presentation-state contract. It is explicitly desktop-shaped and contains selection/glance/status-bar data. — `crates/jackin-usage/src/host.rs:434-479` (confidence: HIGH)
- Keeping the current in-process broker and merely improving its PID file cannot satisfy owner-exit survival or catalog/executor replacement. — `crates/jackin-usage/src/host/broker.rs:544-576`, `crates/jackin-usage/src/host/broker.rs:774-819` (confidence: HIGH)
- Always-resident launchd registration would add host installation/startup policy and host writes not required to prove V1. It remains technically possible, but is not selected.
- Treating authenticated label, discovery source, ordinal, or configuration slot as durable identity contradicts no-duplicate/no-alias requirements. — `crates/jackin-usage/src/host/accounts.rs:37-57`, `crates/jackin-usage/src/host/broker.rs:822-835` (confidence: HIGH)
- Persisting unresolved capability rows as accounts resurrects guessed identity. Persisted history may enrich current canonical members only. — roadmap membership decision and Must-not rules (confidence: HIGH)
- Caller cancellation terminating shared provider work, or caller disconnect starting replacement work, contradicts single authority. Wait cancellation is subscription-only.
- Direct CLI/Console/Capsule/desktop provider calls, second caches, presentation retry queues, and raw snapshot bypasses contradict the settled broker rule. — `research/agent-usage-platform/05-verification-ledger.md` (confidence: HIGH)
- Cost/spend/history/trend payloads are outside V1 even if reference applications expose them. Limits-only remains fixed. — `roadmap/unified-agent-usage/README.md#must-not` (confidence: HIGH)

## Open unknowns

- The demand-activated service choice remains conditional on the bounded spike proving concurrent cold start, activator exit, PID reuse, incompatible healthy service behavior, idle exit/restart, and exact-generation joining. Failure of any invariant requires returning to the resident-service direction; it does not permit an in-process fallback.
- Freeze V1 policy constants in one tested broker module: 10-minute idle exit; 30-second activation lease renewed every 10 seconds; 30-second provider hard timeout; transient exponential backoff starting at 30 seconds and capped at 15 minutes with deterministic full jitter seeded by canonical ID plus failure generation. Provider retry/rate-limit deadlines override this when later. Fake-clock tests own every boundary.
- Exact provider identity availability remains an eight-provider executable matrix. Until a provider proves immutable ID or stable normalized handle, it stays unresolved and cannot render a canonical account.
- Rust locale-aware collation implementation and pinned Unicode/locale data version need selection. Cross-platform fixtures must prove byte-identical rank/order; falling back to platform Swift collation is forbidden.
- Freeze V1 transport at a 1 MiB length-delimited frame and require the F25/40-account maximum fixture below 75% of that bound. — `crates/jackin-protocol/src/usage_broker.rs:13-14`, `native/Design/UnifiedAgentUsage/ExperienceBrief.md:264` (confidence: HIGH for current bound/fixture; MED for selected margin)
- Freeze durable V1 state as one atomic envelope per publication containing projection, aliases, catalog revision, deadlines, and incarnation. If crash injection disproves it, stop and reslice to a maintained transactional store; unordered multi-file JSON is forbidden.
- Package the broker executable beside the host binary/application on every supported platform. Activation resolves that sibling artifact; tests use workspace-owned isolated paths. V1 performs no host-service registration or silent installation.
