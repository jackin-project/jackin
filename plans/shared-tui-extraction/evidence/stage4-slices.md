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

CI/CD observation is deferred until the final aggregate verification phase.
