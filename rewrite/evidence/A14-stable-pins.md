# A14 stable pins verification

Date: 2026-11-09.

## Product changes

- Codex is pinned to 0.167.0 with the researched arm64 and amd64 hashes. `compatibility.lock.toml` records release date 2026-11-06.
- Kimi Code is pinned to 2.5.2 and omp is pinned to 18.18.3 with their researched hashes.
- T01 uses `gpt-6.1-sol`. T05 uses `gemini-4-argon-medium` and remains unavailable for its desktop-keyring reason.
- T15 and T19 use `MiniMax-M3` and remain unavailable with reason `uppercase MiniMax-M3 live proof pending`.
- T11 retains `minimax-m3` and remains supported. T21 uses `claude-haiku-5-5`. T22 retains `gpt-5.1-codex`.
- `plan_multi` now shares the existing instance validator with `plan_launch`. Unavailable tuples return `PlanError::TupleNotSupported` through both planning interfaces.

## Red and green evidence

Before the product changes, the new public checks failed on the old values. Codex reported 0.166.0 instead of 0.167.0. T01 reported `gpt-5.1-codex`, T15 reported lowercase `minimax-m3` and `Supported`, T19 accepted uppercase validation without the required unavailable state, the T21 renderers accepted `claude-haiku-4-5`, and the role manifest accepted the removed T01 model.

The literal mutation pass produced assertion failures for 58 source mutations. The pass covered every URL template, every target triple, every changed version and hash, every unchanged release row, both spec and tuple model declarations, and the T15/T19 states and reasons. Three changed URL templates also failed through production `ImagePlan::final_builds` assertions. The restored catalog and build-plan checks passed.

Removing the shared validator from `plan_multi` made the T15, T17, and T19 planner-refusal tests fail with `expected unavailable tuple refusal`. Restoring the validator made all three pass.

Targeted green runs passed 175 core tests, 608 session tests, and 17 image build-plan tests. The later T15 planner check passed separately. Moving the literal matrix into a constant did not change its assertions. The final `agent_table` run passed 7 tests, and core Clippy passed with warnings denied.

## G-FAST

Command: `mise exec -- mbx +1.99.1 xtask ci --fast`

Result: exit 0. Nextest ran 2168 tests, with 2168 passed and 0 skipped. The run also passed fmt, clippy, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse. The fast gate skipped the docs build as defined by the repository gate.

## Live-proof requirements

No live agent request or live network check was run for this unit.

- Codex 0.167.0 still needs the release re-proof for T01 and T17 through T20.
- Kimi Code 2.5.2 still needs the T08 release re-proof.
- omp 18.18.3 still needs the T09 through T12 release re-proofs and any available T21/T22 credential lanes.
- T15 and T19 must receive fresh live replies with exact uppercase `MiniMax-M3` before either tuple returns to `Supported`.
- T05 remains unavailable for its existing desktop-keyring reason. No live model response is claimed.
- T21 and T22 remain unavailable without API keys.
