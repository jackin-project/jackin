//! jackin-runtime-launch-plan: launch plan diagnostics emission.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`launch_plan::emit_launch_plan`] — plan selection emit.
//!
//! Owns the `LaunchPlan` vocabulary and the diagnostic stage emits
//! for plan selection, prewarm, image materialization, and rejected
//! plans. Split out of `jackin-runtime` (S7 split 56, first
//! `launch/` subunit); the old `launch::LaunchPlan` / `launch::emit_*`
//! paths keep working through a re-export in `launch.rs`.

pub mod launch_plan;
