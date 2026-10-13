# Task 004 source-unit approval

Reviewer: subagent `01a135ed-e345-7462-a415-ed02121fe0d1`, not the implementer.
Verdict context: 2026-10-13.

## Decision

APPROVE source-unit adoption of the P tree
`fba69d60c2373156dc0dcecee8e085c83776966f`.

P is a fast-forward of M: 0 behind, 366 ahead. This record approves
adopting that tree as source units. It is not approval to merge those
366 commits. It is not approval to merge
`cddfead7e2c7b767d93cc82f68016277f020d555`.

## Contracts already on P that must survive

- `should_exit_idle`: idle elapsed AND coordinator idle AND no active
  monitor AND no authenticated foreground guard.
- `USAGE_MONITOR_SCHEMA_VERSION = 4`.
- `USAGE_BROKER_PROTOCOL_VERSION = "v8"`.
- Load refuses versions other than 1, 2|3, or 4 before save.
- `claim_existing_lease` does not treat 401/403/429 text as authority.

## Rejected

- U schema 6 and wire v9 as the selected numbers.
- U `migrate_v4` / `migrate_v5`.
- U `known_foreign_lease_is_reclaimable` v8|v9 allowlist.
- T `9ef617d014df15c6d967501bcbbdcb3689ed453c` as a base.
- Stash untracked deletion parents.
- The hook line `` `let _hook_proof = 41 + 1;` ``.
- The `fea3ade` conflicted tree.
- An in-process sidecar fallback.
- Claiming U live acceptance.

## Required adaptation

One adaptation is required: add `update_broker_used_only`.

Call it from the broker-window update path when a used percentage is
present and reset is absent, including zero used derived from remaining.

- Update only `used`.
- Clear `paired`.
- Do not write a reset epoch.
- Do not copy `effective_reset` or `latest_reset_epochs` onto that used
  field.
- Do not change the stored reset.
- Do not add schema-6 watermark fields.
