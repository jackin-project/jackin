# Plan 001: Freeze the executable usage contract and baseline

## Status
DONE — 2026-08-21 (`test(usage): freeze unified baseline`)

## Why this matters
Later migrations need one measured baseline and one test vocabulary, not prose-only gates.

## Preconditions — run before anything else
- Confirm `chore/roadmap-unified-agent-usage`, PR #898, and clean/understood worktree.
- Read `AGENTS.md`, `TESTING.md`, `ENGINEERING.md`, TUI docs, roadmap, coverage, specs, and research 01–08.

## Spec contract
All specs; especially parity-release architecture and surface-conformance requirements.

## Must NOT
N2, N3, N14. Do not create another branch/PR or change product decisions.

## Inputs to provide
`plans/unified-agent-usage/`, `research/agent-usage-platform/`, current fixture/test inventory.

## Starting state
Current tests cover legacy coordinator/broker/surfaces; canonical targets do not exist.

## Commands you will need
`rtk cargo xtask research check`; focused commands in research chapter 05; `rtk mise run fmt`; `rtk mise run lint`; `rtk mise run test`.

## Suggested executor toolkit
`rg`, existing fixture builders, Rust golden/snapshot harnesses, desktop scripts.

## Scope
Test/fixture scaffolding, documented V1 schema fixture, bypass inventory gate, exact command matrix. No production behavior yet.

## Git workflow
Stay on current branch/PR. Commit `test(usage): freeze unified baseline`, sign off, add Codex co-author, push.

## Steps
### Step 1: Record baseline and schema fixtures
Add canonical valid/invalid V1 envelopes and provider/account/window/state fixture matrix without secrets.
### Step 2: Create compile-safe contract harnesses
Create passing fixture/schema/bypass harnesses and reserve later target names in the
command manifest. Do not commit known-failing tests. Each behavior plan adds its
nonzero behavioral target atomically with the implementation it proves.
### Step 3: Automate bypass inventory
Turn the static provider-call/freshness-owner search into a governed test or xtask allowlist with every current bypass classified.
### Step 4: Freeze surface golden matrices
Name Console, CLI, Capsule, FFI, and desktop cases, dimensions, focus, accessibility, and error states.

## Test plan
Run research check, all new fixture parsers, bypass audit, format/lint, and the focused legacy baseline commands; record exact results.

## Done criteria
Baseline fixture/audit targets pass; later target names and owners are explicit;
schema fixtures parse; bypass audit detects an injected forbidden caller; docs name
exact commands.

## STOP conditions
Schema contradicts specs; a secret-derived value enters fixtures; test target reports zero tests; branch/PR differs.

## Maintenance notes
Update fixture schema and bypass allowlist only with the owning contract change.

## Completion evidence

- `contract_baseline`: 5 passed, zero failed.
- coordinator: 14; broker: 7; runtime relay: 8; Capsule usage: 48; CLI usage: 9;
  FFI bridge: 8 — all passed.
- `rtk mise run fmt`, `rtk mise run lint`, and `rtk mise run test`: PASS.
- research and roadmap audits: PASS.
- No production provider, broker, projection, CLI, TUI, FFI, or Swift behavior changed.
