//! jackin-runtime-launch-restore: restore candidate discovery and resolution.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`restore_resolve::resolve_restore_candidate`] — full resolve
//! without early-scan reuse; [`restore::present_restore_choice`] — the rich
//! launch dialog over same-role + related candidates.
//!
//! Restore candidates for `jackin load` ([`restore::related_restore_candidates`],
//! [`restore::matching_instance_manifests`]), the launch-dialog choice over
//! them ([`restore::present_restore_choice`]), the resolution engine mapping
//! Docker inspect state to [`restore_resolve::RestoreResolution`], and
//! preserved-status persistence ([`restore::write_preserved_status_if_applicable`]).
//! Split out of `jackin-runtime` (S7 split 89); the old
//! `jackin_runtime::runtime::launch::restore::*` and
//! `jackin_runtime::runtime::launch::restore_resolve::*` paths keep working
//! through item re-exports in `launch.rs` and the hub shims.

pub mod restore;
pub mod restore_resolve;
