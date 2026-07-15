# Stage 4 migration slices

## Pin and dependency inversion

- Initial revision `30d3c0cf18ddba07af3fbb872aaf48088cae7539` was pinned by full SHA and committed with `Cargo.lock`.
- Callers of path shortening now import `jackin_core::shorten_home` directly.
- Scroll ownership moved to `termrock::scroll`; OSC 22/52 emission sites use typed TermRock requests and consumer-side pure encoders.
- `crates/jackin-tui` contains no `jackin_core` references and no TermRock compatibility re-export facade.
- Focused package compilation passed for the donor, launch, runtime, console, root, and Capsule consumers.

## Runtime contracts

- Migration exposed a source-compatibility gap in the neutral runtime contract. TermRock repaired it forward in `771d0007c59d3be0d389f127dae94c5ab1b593ba`; jackin❯ repinned that full revision.
- `Dirty`, `UpdateResult`, `NoEffect`, `Subscription`, `SubscriptionPoll`, `Component`, `View`, `drive_frame`, and `drive_render` now resolve from `termrock::runtime`.
- Tokio receiver/spawn plumbing remains consumer-owned while its final console relocation proceeds.

## Launch foundations

- Theme, display-width geometry, ANSI text parsing, typed key dispatch, rich hints, modal backdrop, and neutral dialog scroll state now resolve from TermRock.
- Launch-specific presentation output, warp animation, terminal-mode policy, bottom-chrome composition, and hint wording have explicit local owners in `jackin-launch-tui`.
- TermRock repaired the missing backend-neutral dialog-scroll, viewport, and dialog-layout contracts and removed stale donor-specific catalog artifacts forward; jackin❯ pins full revision `6cd6da3531d8c964a51d2c2ac9a27e51e568a7fb` with the lockfile committed.
- `cargo test -p jackin-launch-tui --lib` passed with 79 tests after the scroll-state repin. Remaining donor references are confined to component families still being migrated.

CI/CD observation is deferred until the final aggregate verification phase.
