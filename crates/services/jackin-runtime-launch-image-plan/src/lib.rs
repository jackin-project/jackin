//! jackin-runtime-launch-image-plan: launch image plan.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`image_plan::resolve_launch_image_plan`] — image decision.
//!
//! Resolves the role repo, reads its manifest, and decides which image a
//! launch would use — the exact work a launch does before it starts a
//! container, and nothing after it. Split out of `jackin-runtime`
//! (S7 split 66); the old
//! `jackin_runtime::runtime::launch::image_plan::*` paths keep
//! working through a re-export shim.

pub mod image_plan;
