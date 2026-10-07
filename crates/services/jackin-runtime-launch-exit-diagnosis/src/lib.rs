//! jackin-runtime-launch-exit-diagnosis: premature-exit diagnosis.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`exit_diagnosis::diagnose_premature_exit_by_id`] — exit diagnosis.
//!
//! Exit diagnosis helpers for premature exits, attach failures, and
//! outcome inspection. Split out of `jackin-runtime` (S7 split 71); the
//! old `jackin_runtime::runtime::launch::exit_diagnosis::*` paths keep
//! working through a module re-export in `launch.rs`.

pub mod exit_diagnosis;
