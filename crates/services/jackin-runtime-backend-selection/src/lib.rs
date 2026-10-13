//! jackin-runtime-backend-selection: backend selection for persisted instances.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`backend_selection::backend_for_state`] —
//! resolve the container backend (Docker vs Apple Container) for a
//! persisted instance from its on-disk state.
//!
//! The manifest form ([`backend_selection::backend_for_manifest`])
//! selects from an already-loaded manifest over the selector
//! ([`backend_selection::InstanceBackend`]).
//! Split out of `jackin-runtime` (S7 split 98): a self-contained
//! selector over the instance manifest plus the core paths,
//! decoupled from the backend lifecycle dispatch. The old
//! `jackin_runtime::runtime::backend::backend_for_state` path
//! keeps working through the hub re-export.

pub mod backend_selection;
