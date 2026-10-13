//! jackin-runtime-discovery: role container discovery.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`discovery::list_role_names`] — name enumeration.
//!
//! Lists running and managed jackin role containers via Docker
//! label queries. Split out of `jackin-runtime` (S7 split 53);
//! the old `jackin_runtime::runtime::discovery::*` paths keep
//! working through a re-export shim.

pub mod discovery;
