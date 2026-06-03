//! Workspace CRUD shim — impl blocks now live in `jackin-config::app_config_workspaces`.

#[cfg(test)]
pub(crate) use crate::config::{AppConfig, WorkspaceConfig, WorkspaceEdit};

#[cfg(test)]
mod tests;
