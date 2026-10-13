//! jackin-runtime-launch-dry-run: dry-run identity resolution.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`dry_run::resolve_dry_run_identity`] —
//! identity resolution.
//!
//! Canonical `--dry-run` identity resolution and model projection against
//! launch admission. Split out of `jackin-runtime` (S7 split 85);
//! the old `jackin_runtime::runtime::launch::*` paths keep
//! working through a re-export shim in `dry_run.rs`.

pub mod dry_run;
