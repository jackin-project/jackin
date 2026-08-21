# Plan 004: Complete the eight-provider quota contracts

## Status
DONE

## Why this matters
Shared surfaces are only truthful when each adapter has proven identity, windows, and failure semantics.

## Preconditions — run before anything else
Plans 001–003 DONE; read provider spec and research 07; obtain only operator-approved fixture captures when needed.

## Spec contract
Provider quotas: supported matrix, limits only, window identity/order, truthful derived states.

## Must NOT
N2, N8, N11-N12. Never log/read/store secret values in fixtures or IDs.

## Inputs to provide
Immutable upstream contracts, sanitized provider fixtures, current eight adapter modules.

## Starting state
Classifier errors and missing Grok REST/OpenCode/Codex cap contracts remain; some provider fields are unproven.

## Commands you will need
Provider-focused `rtk cargo test -p jackin-usage <provider>`; canonical projection tests; fmt/clippy.

## Suggested executor toolkit
Existing HTTP/CLI adapters, mock servers/processes, typed sanitized errors, table-driven fixtures.

## Scope
All eight host providers, trusted credential resolvers, stable/provisional identity, windows, plan/lifecycle errors, semantic fixtures. No browser lane.

## Git workflow
Current branch/PR only; one signed/pushed provider checkpoint when independently green.

## Steps
### Step 1: Fix semantic classifiers
Codex duration-first with constrained slot fallback; Z.AI recognized duration with no positional fallback; preserve provider detail order.
### Step 2: Replace Grok unsupported fallback
Use official CLI-proxy REST contract; ACP may remain facade; remove direct grpc-web scan from supported production path.
### Step 3: Add OpenCode Go
Read `opencode-go` API credential through existing trust boundary; parse rolling/weekly/monthly and typed 401/403/malformed states; remain unresolved without non-secret identity.
### Step 4: Complete supported windows
Codex `individual_limit`; Claude proven extra families; best-effort Z.AI plan; only provider-proven MiniMax caps. Do not fabricate F22 extraction.
### Step 5: Gate derived semantics
Provider-evidenced Not started and confidence-tested rich current run-out only.

## Test plan
Run every fixture enumerated in research 07, sanitized-error tests, no-secret snapshots, provider order/parity, and outbound-route inventory.

## Done criteria
Eight adapters have explicit supported/unresolved contract; Q1-Q3 fixtures pass; no unsupported transport or guessed identity/window is production truth.

## STOP conditions
Only browser/scraping credential path exists; upstream payload lacks claimed field; live capture requires unapproved secret access.

## Maintenance notes
Pin upstream evidence revisions and update fixtures with provider contract changes.

## Completion evidence

- Codex classifies primary/secondary windows by exact provider duration first,
  uses constrained slot fallback only for unknown or missing durations, and
  extracts top-level and `spend_control.individual_limit` monthly money caps.
- Z.AI accepts `CREDIT_LIMIT` and legacy `TOKENS_LIMIT`, maps only recognized
  explicit durations to Session/Weekly, retains provider order, and leaves
  malformed or unknown durations unclassified.
- Grok production refresh uses the supported CLI-proxy REST billing contract
  with optional settings enrichment; ACP remains a bounded fallback and the old
  grpc-web scan is no longer called by production refresh or discovery.
- OpenCode Go reads only the `opencode-go` API entry from the trusted profile
  path, parses rolling/weekly/monthly limits with typed 401/403/malformed states,
  and keeps identity provisional because no durable non-secret identifier exists.
- Provider-facing labels are provider-only (`OpenAI`, `Anthropic`, `Amp`, `xAI`,
  `Z.AI`, `Kimi`, `MiniMax`, `OpenCode`); runtime names remain internal aliases.
- Focused fixture proof covers Codex duration/cap, Z.AI classification, OpenCode
  auth/windows, Grok REST shapes, host discovery, and existing provider suites;
  `jackin-usage` clippy passes with `-D warnings`. The broker lifecycle test now
  runs from `jackin-runtime`, the owning tier for environment-backed service
  resolution; the lower-tier `jackin-usage` dependency on `jackin-env` is removed.
