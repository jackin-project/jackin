// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod absent;
mod eject;
mod exile;
mod prune;
mod purge;
mod purge_absent;

pub use absent::prune_all_instances;
pub use eject::eject_role;
pub use exile::exile_all;
pub use prune::{prune_cache, prune_images, prune_instances, prune_jackin_home, prune_roles};
pub use purge::{purge_class_data, purge_container_state};

pub(crate) use absent::ensure_role_resources_absent_for_purge;
pub(crate) use eject::{eject_docker_role, eject_docker_role_with_handles};
pub(crate) use jackin_runtime_cleanup_dind_gc::dind_gc::gc_orphaned_resources;
pub(crate) use jackin_runtime_cleanup_resolve::resolve::{
    docker_resources_for_state, resolve_cleanup_handles_for_state, resolve_dind_handle_for_state,
    resolve_role_handle_for_state,
};
pub(crate) use jackin_runtime_cleanup_timing::timing::{cleanup_failure, cleanup_timing};
pub(crate) use purge::purge_container_filesystem;
pub(crate) use purge_absent::{ensure_backend_absent_for_purge, remove_socket_dir};

#[cfg(test)]
mod tests;
