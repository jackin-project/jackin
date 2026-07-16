# Stage 4 parity and donor retirement evidence

## Consumer graph

- Pinned TermRock revision: `b1ea42a3febd710e8b663ce6f9fe3406f51add79`.
- `rg -l 'jackin_tui' --glob '*.rs'` returned no files after donor deletion.
- `cargo metadata --format-version 1 --no-deps` completed with no `jackin-tui` or `jackin-tui-lookbook` packages.
- Focused compilation passed for `jackin-capsule`, `jackin-console`, `jackin-launch-tui`, and root `jackin` after deletion.

## Preserved contracts

- TermRock owns neutral palette, display-column geometry, ANSI parsing, typed key dispatch, focus/hover, scrolling, panels, modal lifecycle, tabs, hints, dialogs, pickers, text input, diffs, and runtime contracts.
- Product wording, validation, terminal ownership, Tokio task plumbing, container/debug information, status/footer composition, branded headers, raw ANSI overlays, URL policy, and Capsule bottom chrome remain in jackin❯ crates.
- Capsule's raw-byte decoder boundary returns TermRock logical chords; its existing byte corpus and all 788 library tests passed after migration.
- Launch's 79 library tests and runtime's 608 library tests passed during their migration slices.

## Deferred aggregate gates

Per operator direction, CI/CD observation and aggregate repository gates are deferred until all Stage 5 implementation, documentation, release, and governance work is complete. No CI/CD result is claimed here.
