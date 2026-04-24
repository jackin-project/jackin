//! End-to-end integration test for the workspace manager TUI.
//! Drives `manager::handle_key` with a scripted key stream — no live
//! terminal.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jackin::{
    config::{AppConfig, ConfigEditor},
    launch::manager::{ManagerStage, ManagerState, handle_key},
    paths::JackinPaths,
    workspace::{MountConfig, WorkspaceConfig},
};
use tempfile::tempdir;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn seed_config(paths: &JackinPaths, temp_dir: &std::path::Path) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;

    // Use a host path that actually exists (the tempdir) so
    // WorkspaceConfig's workdir-must-equal-or-be-covered-by-mount-dst
    // validation passes.
    let host_path = temp_dir.display().to_string();
    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path.clone(),
            readonly: false,
        }],
        allowed_agents: vec![],
        default_agent: None,
        last_agent: None,
        env: Default::default(),
        agents: Default::default(),
    };

    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace("big-monorepo", ws)?;
    ce.save()
}

#[test]
fn delete_workspace_via_manager() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;

    assert!(
        config.workspaces.contains_key("big-monorepo"),
        "seed failed"
    );

    let cwd = temp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    assert_eq!(state.workspaces.len(), 1);
    // Preselect lands on the saved workspace (selected=1) because the
    // cwd matches — press `d` to delete.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;
    assert!(
        matches!(state.stage, ManagerStage::ConfirmDelete { .. }),
        "expected ConfirmDelete stage after 'd', got {:?}",
        state.stage
    );

    // Press 'y' — commits the delete.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('y')),
    )?;

    // Config on disk should no longer have big-monorepo.
    let reloaded = AppConfig::load_or_init(&paths)?;
    assert!(
        !reloaded.workspaces.contains_key("big-monorepo"),
        "workspace should be deleted from disk"
    );

    // In-memory state: returned to List, empty workspace list.
    assert!(matches!(state.stage, ManagerStage::List));
    assert!(
        state.workspaces.is_empty(),
        "in-memory list should be empty"
    );

    Ok(())
}
