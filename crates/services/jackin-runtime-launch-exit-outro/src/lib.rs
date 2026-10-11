//! jackin-runtime-launch-exit-outro: exit outro from an observed boundary.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`exit_outro::render_exit_observation`] — render the
//! two-screen exit outro (decelerating warp, then closing caption) once
//! the last container has left the construct.
//!
//! Observation-inversion split out of `jackin-runtime` (S7 split 95): the
//! hub keeps observing the universe exit boundary
//! (`jackin-runtime-universe`, T7) and hands the already-observed
//! `(running, ExitClaim, force_outro, data_dir)` to this leaf, which owns
//! the still-running summary plus the rich-terminal outro. The old
//! `jackin_runtime::runtime::launch::render_exit` path keeps working
//! through the hub wrapper.

pub mod exit_outro;
