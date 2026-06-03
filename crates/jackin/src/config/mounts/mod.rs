//! Global mount display shim — `GlobalMountRow`, `WorkspaceGlobalMountRows`,
//! and the `AppConfig` mount impl blocks now live in `jackin-config::app_config_mounts`.

pub use jackin_config::{GlobalMountRow, WorkspaceGlobalMountRows};

#[cfg(test)]
pub(crate) use crate::config::{AppConfig, GlobalMountConfig, MountConfig, MountEntry};
#[cfg(test)]
pub(crate) use std::collections::BTreeMap;

#[cfg(test)]
mod tests;
