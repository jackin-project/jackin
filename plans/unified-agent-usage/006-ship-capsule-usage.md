# Plan 006: Ship resolved-agent Capsule Usage

## Status
DONE

## Why this matters
Capsule preview must represent launch reality before a session without becoming launch policy.

## Preconditions — run before anything else
Plans 001–004 DONE; read Capsule spec, existing modal tests/snapshots, runtime relay and launch resolution.

## Spec contract
Capsule resolved membership, modal grammar, independent lifecycle/quota state, truthful empty/failures.

## Screen contract
S11–S13 including Overview, resolved-agent tabs, conditional account tabs, narrow/focus/scroll/empty.

## Must NOT
N1, N3, N5-N8, N10.

## Inputs to provide
Resolved launch inventory, relay-scoped broker projection, existing Usage dialog/widget/snapshots.

## Starting state
Current fixed/provider tabs and queued refresh/Claude diagnostic bypass are session-centered.

## Commands you will need
`rtk cargo test -p jackin-runtime usage_relay::resolved_launch_inventory -- --test-threads=1`; `rtk cargo test -p jackin-capsule usage_projection -- --test-threads=1`; smoke/render gates.

## Suggested executor toolkit
Existing dialog state machine, shared quota formatter/widget, typed relay capability.

## Scope
Resolved inventory projection, lifecycle modeling, Overview/agent/account tabs, refresh join, bypass removal, snapshots/docs.

## Git workflow
Current branch/PR only; signed commits pushed immediately.

## Steps
### Step 1: Project resolved launch inventory
Map each fully resolved agent and forwarded canonical account; no global/fixed/capability-only rows.
### Step 2: Separate lifecycle dimensions
Carry `agent_uninitialized`, capability, freshness, and provider errors independently; initialization clears lifecycle only.
### Step 3: Retarget modal navigation
Overview plus agent tabs; account sub-tabs only for multiple accounts; preserve focused destination where valid.
### Step 4: Replace refresh paths
Remove direct Claude diagnostic and pending post-flight queue; submit/join broker intent once.
### Step 5: Render full state matrix
Account pairs on Overview, provider-order detail windows, zero-agent exact copy/no Retry, failures, narrow, scroll and focus reversal.

## Test plan
Relay allowlist/membership; agent/account dedup; uninitialized→initialized; quota never gates launch; render snapshots; no direct provider import/call.

## Done criteria
Named targets pass; all resolved accounts appear once per required agent context; no bypass; launch behavior unchanged by quota.

## Execution evidence — 2026-08-21

- Capsule empty inventory now renders the exact Rust-owned
  `No agents configured for this Capsule.` message and removes the refresh
  action from the footer; quota never gates launch.
- Relay membership has an explicit deduplicating
  `resolved_launch_usage_inventory` projection and regression test. It derives
  only from resolved `CapsuleConfig.agents`; global discovery/capabilities do
  not create Capsule rows.
- The direct `jackin-capsule usage claude-cli` diagnostic bypass was removed;
  Capsule usage remains broker/relay-backed through `accounts` and `verify`.
- Focused proof: `cargo test -p jackin-runtime resolved_launch_inventory
  --offline -- --test-threads=1` (1 passed), `cargo test -p jackin-capsule
  usage_projection --offline -- --test-threads=1` (1 passed), capsule/runtime
  clippy with `-D warnings`, and formatting.

## STOP conditions
Resolved launch config is unavailable at projection boundary; quota begins influencing launch; lifecycle state is collapsed.

## Maintenance notes
Any future Capsule membership source change requires explicit roadmap decision and fixtures.
