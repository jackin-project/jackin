//! Host-side path layout — re-exported from `jackin-core`.
//!
//! The canonical definition of `JackinPaths` lives in `jackin-core` so that
//! `jackin-config` (which depends on `jackin-core`) can use it without
//! circular dependencies.

pub use jackin_core::JackinPaths;
