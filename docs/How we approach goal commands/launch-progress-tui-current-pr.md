# Launch Progress TUI Current PR Goal

Use this prompt for the current Launch Progress TUI implementation PR.

Current PR/branch:

- `docs/launch-progress-run-diagnostics`
- https://github.com/jackin-project/jackin/pull/new/docs/launch-progress-run-diagnostics

Source of truth:

- `docs/src/content/docs/reference/roadmap/launch-progress-tui.mdx`
- `docs/src/content/docs/reference/roadmap/snapshot-tests-tui.mdx`
- `docs/src/content/docs/reference/roadmap.mdx`

```text
/goal Implement the Launch Progress TUI end to end in the current PR/branch `docs/launch-progress-run-diagnostics`.

Read the Launch Progress TUI roadmap first and treat it as the product and
technical source of truth. Use the snapshot-test roadmap only for render-test
direction when the rich ratatui renderer needs a test harness. Do not repeat the
roadmap in the PR; implement it and update it only when code reveals a missing
or contradictory decision.

Ship the complete feature in one large pull request, following the roadmap
phases internally in order: boundary rain gating, evented launch progress,
durable run diagnostics, rich TUI renderer, console integration, and only then
startup parallelization.

Implement the settled decisions captured in the roadmap: run IDs for every
`jackin ...` command, JSONL run diagnostics, compact non-debug artifacts,
detailed debug artifacts only when `--debug` is enabled, no diagnostics command
in this PR, `--no-rain`, `--no-tui`, `JACKIN_NO_MOTION=1`, conservative rich
terminal detection, structured stages, compact/non-interactive/test renderers,
the ratatui rich cockpit, console-triggered launch reuse, first/last-container
rain gating, and no raw logs or debug firehose in the operator-facing launch
surface.

Keep host-side writes limited to jackin-owned diagnostics/prune state described
in the roadmap. Do not mutate host Git, shell, Docker, `gh`, agent home, or
terminal configuration as part of this work.

Update operator and contributor docs in the same PR as behavior lands:
`jackin load`, `jackin console`, TUI design decisions, codebase map, and any
launch/runtime internals page touched by the implementation. Update roadmap
status and overview if the item becomes partially implemented or fully shipped.

Before marking ready, run relevant Rust launch/TUI/diagnostics tests, clippy if
feasible for touched code, and docs checks from `docs/`: `bun run build`,
`bun run check:repo-links`, `bunx tsc --noEmit`, and `bun test`.

Do not add `CHANGELOG.md` entries before first release.
```
