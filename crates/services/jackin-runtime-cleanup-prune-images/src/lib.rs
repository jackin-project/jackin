//! jackin-runtime-cleanup-prune-images: unused jackin-managed Docker image pruning.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`prune_images::prune_images`] —
//! remove `jk_*` images no role container still references.
//!
//! Split out of `jackin-runtime` (S7 split 99): a self-contained
//! best-effort image sweep over the Docker client plus the
//! naming labels and prune-output rows, decoupled from the
//! instance/role pruning that stays in the hub. The old
//! `jackin_runtime::runtime::cleanup::prune_images` path
//! keeps working through the hub re-export.

pub mod prune_images;
