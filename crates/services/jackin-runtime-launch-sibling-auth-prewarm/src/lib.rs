//! jackin-runtime-launch-sibling-auth-prewarm: sibling auth prewarm for a role.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`sibling_auth_prewarm::spawn_sibling_auth_prewarm`] —
//! prewarm the auth slots of a role's non-selected (sibling) agents on
//! the blocking pool.
//!
//! The await phase ([`sibling_auth_prewarm::await_sibling_auth_prewarm`])
//! waits for those writes before role-state mount admission, over the
//! blocking-pool worker ([`sibling_auth_prewarm::spawn_auth_prewarm_worker`])
//! and the prewarm bundle ([`sibling_auth_prewarm::SiblingAuthPrewarm`]).
//! Split out of `jackin-runtime` (S7 split 97): a self-contained
//! spawn/await pair over the role manifest plus the capsule-setup auth
//! bindings, decoupled from the launch runtime core. The old
//! `jackin_runtime::runtime::launch::spawn_sibling_auth_prewarm` path
//! keeps working through the hub re-export.

pub mod sibling_auth_prewarm;
