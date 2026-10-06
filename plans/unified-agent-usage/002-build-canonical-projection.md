# Plan 002: Build canonical identity and projection V1

## Status
DONE

## Why this matters
Deduplication, order, selection, and parity require one surface-neutral Rust truth.

## Preconditions — run before anything else
Plan 001 DONE; read canonical-projection spec and research 06; verify cited symbols still exist.

## Spec contract
Canonical projection: one V1 projection, evidence identity, deterministic membership/order, Rust semantics, selection removal.

## Must NOT
N1, N4, N8-N10, N14.

## Inputs to provide
V1 fixtures, current protocol records, discovery/catalog/coordinator state, provider labels.

## Starting state
Account generations and desktop-shaped aggregate exist; authenticated labels/source IDs can influence identity.

## Commands you will need
`rtk cargo test -p jackin-usage canonical_projection -- --test-threads=1`; protocol tests; fmt/clippy.

## Suggested executor toolkit
Serde, existing coordinator state store, property tests for merge/order/collision.

## Scope
Protocol V1 records; canonical evidence/alias/merge; discovery membership; Rust labels/order/formatting; projection publication. No consumer UI.

## Git workflow
Current branch/PR only. Commit/push cohesive checkpoints; never rewrite published history without approval.

## Steps
### Step 1: Add secret-free V1 wire records
Implement schema version, projection/provider/account/window/freshness/issue/unresolved records and compatibility tests.
### Step 2: Replace ordinal/label identity
Implement typed evidence ladder, domain-separated IDs, collision failure, provisional unresolved capability, atomic alias transition.
### Step 3: Separate membership, merge, and ordering
Use current discovery only for membership; deterministic precedence; fixed provider orders; locale-stable account ranks; provider window ranks.
### Step 4: Publish immutable generations
Build atomic projection from committed account state and retain last-good on partial failure.
### Step 5: Add destination normalization helper
Account-only multi-provider destinations and explicit removal result/notice; no presentation selection in JSON.

## Test plan
Duplicate discovery, collision, alias replay/crash, empty/unresolved, stable order under severity changes, unknown schema, partial failure, golden JSON.

## Done criteria
Canonical target passes; all fixtures have one account per evidence identity; no source ordinal/secret/agent name in IDs or labels.

## STOP conditions
Provider lacks non-secret identity and implementation guesses one; cross-platform collation differs; V1 exceeds transport bound without measured redesign.

## Maintenance notes
Breaking V1 meaning needs major schema; additive fields require compatibility fixture.

## Completion evidence

- Additive V1 protocol records retain the account-generation wire during consumer migration; unknown majors, conflicting percentages, ranks, golden JSON, and the 40-account transport margin are tested.
- Canonical IDs use typed provider evidence and provider-only machine identities; legacy routing/source IDs remain operational but cannot become V1 identity.
- Current discovery owns membership; unresolved capabilities never become account rows. ICU4X 2.2 `und`, English, Turkish, and Vietnamese goldens freeze account ranks.
- Immutable reads retain one publication until content changes; stale last-good windows survive partial failure. Alias replay is idempotent and collisions fail before mutation.
- Typed destination normalization preserves valid account selection and returns removed selections to Overview with `Selected account is no longer available.`
- Proof: `canonical_projection` 5 passed; protocol `usage_broker` 8 passed; `jackin-usage` nextest 290 passed; `rtk mise run fmt`, `lint`, `test`, and `rtk cargo xtask ci --e2e` passed.
