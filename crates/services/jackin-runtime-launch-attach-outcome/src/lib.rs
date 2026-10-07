//! jackin-runtime-launch-attach-outcome: instance status persistence.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`attach_outcome::record_instance_attach_outcome`] —
//! outcome record.
//!
//! Instance-status writes and attach-outcome recording against recorded
//! instance state. Split out of `jackin-runtime` (S7 split 84);
//! the old `jackin_runtime::runtime::launch::*` paths keep
//! working through item re-exports in `launch.rs`.

pub mod attach_outcome;
