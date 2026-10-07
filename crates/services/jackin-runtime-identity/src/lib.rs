//! jackin-runtime-identity: host git identity and capsule supervisor user.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`identity::load_git_identity`] — best-effort git capture.
//!
//! Captures host `user.name`/`user.email` for in-container git defaults and
//! exposes the fixed root-supervisor `--user` value used at the capsule
//! boundary. Split out of `jackin-runtime` (S7 split 45); the old
//! `jackin_runtime::runtime::identity::*` paths keep working through a
//! re-export shim.

pub mod identity;
