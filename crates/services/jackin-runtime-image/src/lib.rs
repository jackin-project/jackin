//! jackin-runtime-image: role image build, prewarm, and refresh pipeline.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`image::prewarm_role_images`] — role image prewarm orchestration.
//!
//! Decides, builds, prewarms, and refreshes per-agent role images:
//! binary preparation, build execution, sibling prewarm, staleness
//! sentinels, and published-version tracking. Split out of
//! `jackin-runtime` (S7 split 60); the old
//! `jackin_runtime::runtime::image::*` paths keep working
//! through a re-export shim.

pub mod image;
