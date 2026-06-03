//! jackin-core: universal vocabulary types shared across all jackin' crates.
//!
//! This is a leaf crate — it has no jackin' dependencies, no tokio, no
//! subprocess, no filesystem access. Every higher crate depends on this one,
//! never the reverse.
//!
//! Public surface: `Agent`, `MountIsolation`, `AuthForwardMode`, and shared
//! string constants.

pub mod agent;
pub mod auth;
pub mod constants;
pub mod env_value;
pub mod isolation;
pub mod paths;

pub use agent::{Agent, ParseAgentError};
pub use auth::AuthForwardMode;
pub use env_value::{EnvValue, FieldTarget, OpRef};
pub use isolation::{MountIsolation, ParseMountIsolationError};
pub use paths::JackinPaths;
