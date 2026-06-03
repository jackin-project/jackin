//! Shared string constants used across all jackin' crates.
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

/// Canonical source of truth from `jackin-protocol` — re-exported here so
/// crates that depend on `jackin-core` (but not `jackin-protocol` directly)
/// can use the prefix without literal drift.
pub use jackin_protocol::{CONTAINER_PREFIX, CONTAINER_PREFIX_DASH};
