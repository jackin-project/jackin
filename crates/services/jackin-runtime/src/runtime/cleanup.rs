// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Container and class teardown: purge role-state directories, remove Docker
//! resources (containers, images, networks, volumes), and update the instance
//! index to reflect the deletion.
//!
//! Drives each filesystem teardown to completion before batching index
//! updates — if an early deletion fails, already-deleted entries are still
//! recorded so the index stays consistent with disk state.

mod absent;
mod eject;
mod exile;
mod prune;
mod purge;
mod purge_absent;
// Moved to jackin_runtime_cleanup_resolve::resolve (S7
// split 82); the item re-export keeps every
// `cleanup::*` path stable.
// Moved to jackin_runtime_cleanup_timing::timing (S7
// split 91); the item re-export keeps every
// `cleanup::*` path stable.
// Moved to jackin_runtime_cleanup_dind_gc::dind_gc (S7
// split 93); the item re-export keeps every
// `cleanup::*` path stable.

pub use absent::prune_all_instances;
pub use eject::eject_role;
pub use exile::exile_all;
pub use prune::{prune_cache, prune_images, prune_instances, prune_jackin_home, prune_roles};
pub use purge::{purge_class_data, purge_container_state};

pub(crate) use absent::ensure_role_resources_absent_for_purge;
pub(crate) use eject::{eject_docker_role, eject_docker_role_with_handles};
pub(crate) use exile::prune_dir;
pub(crate) use jackin_runtime_cleanup_resolve::resolve::{
    docker_resources_for_state, resolve_cleanup_handles_for_state, resolve_dind_handle_for_state,
    resolve_role_handle_for_state,
};
// `resolve_optional_container_handle` had its hub re-export retired by S7
// split 92: the moved `launch_dind` module was its sole consumer and now
// names it through `jackin-runtime-cleanup-resolve` directly.
pub(crate) use jackin_runtime_cleanup_dind_gc::dind_gc::gc_orphaned_resources;
pub(crate) use jackin_runtime_cleanup_timing::timing::{cleanup_failure, cleanup_timing};
pub(crate) use purge::purge_container_filesystem;
pub(crate) use purge_absent::{ensure_backend_absent_for_purge, remove_socket_dir};

#[cfg(test)]
mod tests;
