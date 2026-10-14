# Plan 003: Build the durable single-authority broker

## Status
DONE

## Why this matters
Single-flight inside an activating process does not survive owner exit or prevent consumer bypasses.

## Preconditions — run before anything else
Plans 001–002 DONE; read broker-refresh spec and research 06; isolate test state under workspace-owned paths.

## Spec contract
Broker: durable authority, joined work, adaptive cadence, recoverable persistence.

## Must NOT
N3, N4. No silent launchd or host configuration writes.

## Inputs to provide
V1 protocol, current coordinator/broker/store, fake clock/executor, process test harness.

## Starting state
Broker thread belongs to first caller, PID-only election exists, consumers retain bypass/cache controls.

## Commands you will need
`rtk cargo test -p jackin-runtime --test broker_service_lifecycle -- --test-threads=1`; coordinator/broker suites; fmt/clippy.

## Suggested executor toolkit
Independent broker executable, mode-0600 Unix socket, atomic lease/state, process-level integration tests.

## Scope
Demand activation spike and implementation; handshake; service lifecycle; catalog revision; deadlines; joins; cancellation; crash recovery; protocol operations.

## Git workflow
Current branch/PR only; push each signed commit. Do not install host services.

## Steps
### Step 1: Prove activation direction
Test concurrent cold start, activator exit, PID reuse, incompatible healthy broker, idle restart, exact-generation joins. If invariant fails, stop and document resident-service reslice.
### Step 2: Split client and executor lifetime
Clients activate/connect only; independent service loads state, binds authenticated endpoint, publishes readiness.
### Step 3: Implement projection operations
CurrentProjection, RequestRefresh, and JoinPublication with catalog/generation IDs and relay allowlists.
### Step 4: Centralize policy
Implement fake-clock 2/5/15/30 cadence; 10-minute idle exit; 30-second activation
lease renewed every 10 seconds; 30-second provider timeout; and 30-second-to-15-minute
exponential backoff with deterministic full jitter. Later provider rate-limit/retry
deadlines win. Implement force constraints, cancellation isolation, no follow-up queue.
### Step 5: Harden persistence/recovery
Use one atomic V1 envelope for projection, aliases, catalog revision, deadlines, and
incarnation. Add corrupt-state quarantine, owner-lost recovery, immutable last-good,
and a 1 MiB length-delimited frame; F25/40 accounts must stay below 75%.

## Test plan
Adversarial multi-process suite plus legacy coordinator/broker tests, transport permissions, crash fault injection, zero direct executor construction by clients.

## Done criteria
Owner exit survives; four concurrent clients see one generation; retry/cadence restored after restart; static audit has one provider authority.

## STOP conditions
In-process fallback needed; endpoint permits another user; crash can publish mixed state; test writes outside approved workspace paths.

## Maintenance notes
Broker protocol/persistence versions and policy constants live in one module with fake-clock tests.

## Completion evidence

- Added the shipped sibling `jackin-usage-broker` executable under `jackin-runtime`,
  the owning orchestration tier. Process activation uses a
  per-user mode-0600 Unix socket and JSON lease with protocol/build identity and epoch
  expiry; PID is not authority. The service survives the activating client and exits only
  after the configured ten-minute idle interval with no active coordinator work.
- Centralized cadence and retry policy in `coordinator::policy`: 2/5/15/30-minute
  activity cadence, 30-second provider deadline, ten-minute idle lifetime, 30-second
  lease renewed every ten seconds, deterministic bounded full jitter from 30 seconds to
  15 minutes, and provider retry deadlines winning when later.
- Added additive projection operations (`CurrentProjection`, `RequestRefresh`, and
  `JoinPublication`) and a durable atomic projection envelope containing publication,
  aliases, catalog revision, deadlines, and broker incarnation. Corrupt envelopes are
  quarantined before rendering or provider work.
- Production CLI, runtime relay assembly, and desktop bridge activation now use the
  process client; the executor-backed helper remains a test/runtime-only compatibility
  seam for existing adversarial tests.
- Proof: `broker_service_lifecycle` (four concurrent cold activators, one publication,
  activator exit survival); coordinator 14; broker 8; policy 2; projection-state 6;
  protocol usage-broker 9; `cargo clippy -p jackin-usage --all-targets --all-features
  --locked -- -D warnings`; dependent runtime/FFI/CLI `cargo check`; and `cargo fmt`.
