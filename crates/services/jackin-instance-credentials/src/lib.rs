//! jackin-instance-credentials: credential substrate for role containers.
//!
//! Outcome types, journaled auth-directory transactions, and secure path
//! primitives shared by the agent provisioners. Leaf crate: depends only on
//! `jackin-core`/`jackin-config` plus external crates.

pub mod auth_directory;
mod error;
mod limits;
mod mounts;
mod outcomes;
mod paths;
mod permissions;

pub use auth_directory::AuthMountLease;
pub use error::{InstanceError, SyncSourceValidationError};
pub use limits::{
    MAX_AUTH_SOURCE_FILE_BYTES, MAX_AUTH_SOURCE_TREE_BYTES, MAX_AUTH_SOURCE_TREE_ENTRIES,
};
pub use mounts::{mount_directory_present, mount_file_present};
pub use outcomes::{
    AuthProvisionOutcome, GithubAuthContext, GithubProvisionKind, GithubProvisionOutcome,
    GithubTokenSource, HostMissingReason,
};
pub use paths::{
    create_private_file_if_absent, is_platform_root_alias, read_bounded_local_file,
    reject_auth_path, reject_symlink, write_private_bytes, write_private_file,
};
pub use permissions::{
    PermissionRepairFailure, maybe_inject_permission_repair_failure, repair_permissions,
};
#[cfg(any(test, feature = "test-support"))]
pub use permissions::{PermissionRepairFailureGuard, inject_permission_repair_failure};
