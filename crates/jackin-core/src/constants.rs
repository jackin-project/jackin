//! Shared string constants used across all jackin❯ crates.
//!
//! These are the canonical definitions — callers import from here rather than
//! repeating the literal. All belong to the role-repo contract or the
//! operator-env protocol.

/// Filename of the role manifest that every role repo must contain.
pub const MANIFEST_FILENAME: &str = "jackin.role.toml";

/// Name of the Dockerfile that every role repo must contain.
pub const DOCKERFILE_NAME: &str = "Dockerfile";

/// Env var that carries the Claude Code OAuth token into role containers.
///
/// Set by `auth_forward = "oauth_token"` mode; Claude Code inside the
/// container reads it to skip interactive login.
pub const CLAUDE_OAUTH_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Current role manifest schema version. Serde default for `RoleManifest.version`.
pub const CURRENT_MANIFEST_VERSION: &str = "v1alpha6";

/// Serde-default helper for `RoleManifest.version`.
pub fn current_manifest_version() -> String {
    CURRENT_MANIFEST_VERSION.to_owned()
}

/// Container-name grammar shared by the host launcher and the in-container
/// capsule. The launcher constructs names of the shape
/// `jk-<id>[-<workspace>]-<role>`; both binaries must agree on how to parse
/// them.
pub const CONTAINER_PREFIX: &str = "jk";

/// Prefix with the trailing separator, used by [`instance_id_from_container_base`]
/// to strip the family marker before splitting.
pub const CONTAINER_PREFIX_DASH: &str = "jk-";

/// Extract the instance-ID component from a container base name.
///
/// Returns `None` when the name does not start with `jk-` or has no `-` after
/// the id component. Single parser shared by host manifest construction
/// (`JACKIN_INSTANCE_ID` injection) and the capsule's status bar so the two
/// surfaces cannot drift on what a `jk-…` name means.
#[must_use]
pub fn instance_id_from_container_base(container_base: &str) -> Option<&str> {
    container_base
        .strip_prefix(CONTAINER_PREFIX_DASH)?
        .split_once('-')
        .map(|(id, _)| id)
}
