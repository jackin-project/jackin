//! jackin-runtime-launch-load-cleanup: launch teardown coordinator.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`load_cleanup::LoadCleanup`] — teardown coordinator.
//!
//! `LoadCleanup` coordinates Docker resource teardown for a failed or
//! completed launch, plus the atomic-write guard launch uses for
//! single-file bind mounts. Split out of `jackin-runtime` (S7 split
//! 68); the old `jackin_runtime::runtime::launch::load_cleanup::*`
//! paths keep working through a module re-export in `launch.rs`.

pub mod load_cleanup;
