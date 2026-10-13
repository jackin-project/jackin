//! jackin-runtime-attach-inspect: instance inspection.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`inspect::inspect_hardline_instance`] — inspection.
//!
//! Hardline instance inspection plus container, network,
//! and mount state descriptions. Split out of
//! `jackin-runtime` (S7 split 80); the old
//! `jackin_runtime::runtime::attach::inspect::*` paths keep
//! working through a module re-export in `attach.rs`.

pub mod inspect;
