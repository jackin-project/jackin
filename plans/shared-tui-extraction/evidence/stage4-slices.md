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
- `jackin-runtime` test-only prompt and layout fixtures now resolve through TermRock or launch-owned policy; its donor dependency was removed and all 608 library tests passed.

## Launch foundations

- Theme, display-width geometry, ANSI text parsing, typed key dispatch, rich hints, modal backdrop, and neutral dialog scroll state now resolve from TermRock.
- Launch-specific presentation output, warp animation, terminal-mode policy, bottom-chrome composition, and hint wording have explicit local owners in `jackin-launch-tui`.
- TermRock repaired the missing backend-neutral dialog-scroll, viewport, and dialog-layout contracts and removed stale donor-specific catalog artifacts forward; jackin❯ pins full revision `b1ea42a3febd710e8b663ce6f9fe3406f51add79` with the lockfile committed.
- TermRock restored the product-neutral parity implementations for confirmation, error, filtered-selection, text-entry, diff, and scroll-panel surfaces; its all-feature suite passed with 146 tests.
- Container/debug information remains product-owned in `jackin-launch-tui`, including its wording, row policy, hyperlink overlay, and clipboard targeting.
- `cargo test -p jackin-launch-tui --lib` passed with 79 tests. `rg -l 'jackin_tui' crates/jackin-launch-tui --glob '*.rs'` is empty and the crate no longer depends on `jackin-tui`.

CI/CD observation is deferred until the final aggregate verification phase.

## Console component families

- Console panels, dialogs, pickers, text entry, tabs, wrapped hints, diff, scroll, modal lifecycle, and geometry now resolve through TermRock revision `b1ea42a3febd710e8b663ce6f9fe3406f51add79`.
- Product-owned modal sizing, brand composition, and container/debug information moved into `jackin-console`; `cargo check -p jackin-console` passed.
- Tokio subscription plumbing, mouse-mode escape policy, and status-footer composition now live in `jackin-console`; root console services call that owner directly.
- `rg -l 'jackin_tui' crates/jackin-console --glob '*.rs'` is empty, the donor dependency is removed, and `cargo check -p jackin-console -p jackin` passed.
