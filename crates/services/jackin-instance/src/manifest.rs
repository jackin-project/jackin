// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Instance index (`instances.json`) and per-container manifest: status tracking, session records.
//!
//! The index is the host-side registry of every container jackin❯ has
//! launched; the per-instance manifest records lifecycle status and agent
//! session history. Not responsible for Docker interaction — purely JSON
//! persistence under `~/.jackin/data/`.
// Pure index/session data types now live in `jackin-core` so that
// `jackin-console` can use them without depending on `jackin-runtime`.
pub use jackin_core::{
    InstanceIndexEntry, InstanceQuery, InstanceStatus, SessionRecord, SessionStatus,
};
mod index;
mod records;
mod resources;
mod store;

pub use records::{
    AdmittedInstance, INSTANCE_INDEX_VERSION, INSTANCE_MANIFEST_VERSION, InstanceIndex,
    InstanceManifest, NewInstanceManifest, RegistrationState, host_path_fingerprint,
};
pub use resources::{AppleContainerResources, BackendResources, DockerIdentity, DockerResources};

pub(crate) use index::now_rfc3339;
#[cfg(test)]
pub(crate) use index::{INSTANCE_INDEX_FILE, INSTANCE_INDEX_LOCK_FILE};

#[cfg(test)]
mod tests;
