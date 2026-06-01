//! Non-TUI config persistence services.

use crate::config::{AppConfig, RoleSource};
use crate::paths::JackinPaths;

/// Upsert one role source into the operator config and reload the saved model.
pub fn upsert_role_source(
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: &str,
    source: &RoleSource,
) -> anyhow::Result<()> {
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
    editor_doc.upsert_agent_source(key, source);
    *config = editor_doc.save()?;
    Ok(())
}

/// Remove one saved workspace from operator config and reload the saved model.
pub fn remove_workspace(
    config: &mut AppConfig,
    paths: &JackinPaths,
    name: &str,
) -> anyhow::Result<()> {
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
    editor_doc.remove_workspace(name)?;
    *config = editor_doc.save()?;
    Ok(())
}
