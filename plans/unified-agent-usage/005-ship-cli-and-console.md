# Plan 005: Ship the simple CLI and native Console Usage route

## Status
DONE

## Why this matters
Headless automation needs stable plain output; operators need a Console view that belongs to existing jackin❯ grammar.

## Preconditions — run before anything else
Plans 001–004 DONE; read CLI/Console specs and TUI docs; inspect existing CLI/cache bypasses and Console snapshots.

## Spec contract
CLI requirements and Console Overview/Account Detail/interaction contracts.

## Screen contract
S1–S6, S9–S10 exactly as specs; top-left `jackin❯ · usage`, body split, Capsule meters, simple CLI.

## Must NOT
N1-N3, N8-N10, N13.

## Inputs to provide
Canonical projection client, CLI fixture matrix, Console TermRock components, Capsule meter formatter.

## Starting state
Instance-oriented CLI/cache exists; bare host command and Console Usage route do not.

## Commands you will need
`rtk cargo test -p jackin cli::usage::canonical_overview -- --test-threads=1`; `rtk cargo test -p jackin-console usage -- --test-threads=1`; render conformance; fmt/clippy.

## Suggested executor toolkit
Shared Rust projection formatter, existing Console brand header/split/focus/footer, Capsule quota widget extraction.

## Scope
Bare human/JSON command, retained instance accounts/verify, removal of bypass switches/store; Console route/list/detail/states/input/snapshots/docs.

## Git workflow
Current branch/PR only. Separate signed/pushed CLI and Console commits allowed; both remain in #898.

## Steps
### Step 1: Implement bare command adapters
Final-only bounded broker read; simple grouped human output; exact V1 JSON and exit/stdout/stderr behavior.
### Step 2: Migrate instance diagnostics
Retain accounts/verify intent through resolved config and broker; delete/redefine alternate cache/snapshot/no-refresh/sync paths.
### Step 3: Add top-level Console route
Use shipped brand header/frame/master-detail/footer; typed canonical destinations and persistent removal notice.
### Step 4: Compose Capsule-parity limits
Extract/share semantic meter geometry and detail rows while preserving Console panels/focus; compact fallback below breakpoint.
### Step 5: Complete state and input matrix
Loading, refresh-last-good, empty, stale, partial/global error; keyboard/mouse/focus/scroll/footer and Settings affordance.

## Test plan
CLI golden JSON/human across TTY/plaintext/width/state/exit; Console render matrix 80×24/narrow/wide/focus/scroll/removal; no-provider-call audit.

## Done criteria
Both named test targets pass nonzero tests; semantic fixtures match; CLI remains simple; Console matches repository snapshots/style.

## Execution evidence — 2026-08-21

- Bare `jackin usage` reads the broker-owned canonical V1 projection and
  renders compact provider/account/window output or exact projection JSON.
- Existing instance `accounts`/`verify` parsing remains intact; no new direct
  provider or cache authority was added.
- `jackin console` now opens Usage with `u` from the workspace list, uses the
  shipped `jackin❯ · usage` frame, canonical account rows, account detail, and
  full-width Capsule-style meters. Multi-account providers are grouped without
  duplicate provider destinations.
- Focused proof: `cargo test -p jackin-console --offline -- --test-threads=1`
  (1286 passed), `cargo test -p jackin cli::tests --offline -- --test-threads=1`,
  `cargo clippy -p jackin-console --all-targets --offline -- -D warnings`,
  `cargo clippy -p jackin --all-targets --offline -- -D warnings`,
  `cargo fmt --check`, and `mise run lint`.

## STOP conditions
Console requires invented global navigation; CLI needs direct fetch/cache authority; shared meter extraction changes Capsule behavior without proof.

## Maintenance notes
Update TUI docs for cross-cutting focus/navigation/compact behavior in this PR.
