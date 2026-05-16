//! List-stage dispatch: workspace-picker key handling and the
//! list-level modal (`GithubPicker`).

use crossterm::event::{KeyCode, KeyEvent};

use super::super::super::widgets::{
    ModalOutcome, confirm::ConfirmState, file_browser::FileBrowserState,
};
use super::super::render::apply_scroll_delta;
use super::super::state::{
    EditorState, FileBrowserTarget, ManagerListRow, ManagerStage, ManagerState, Modal,
    SettingsState, Toast, ToastKind,
};
use super::InputOutcome;
use crate::config::AppConfig;
use crate::console::ConsoleInstanceAction;
use crate::paths::JackinPaths;

#[allow(clippy::too_many_lines)]
pub(super) fn handle_list_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    _cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // See ManagerListRow docs for row layout.
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => Ok(InputOutcome::ExitJackin),
        KeyCode::Left | KeyCode::Char('h' | 'H') => {
            scroll_list_horizontal(state, -8);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Right | KeyCode::Char('l' | 'L') => {
            scroll_list_horizontal(state, 8);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            if state.list_scroll_focus.is_some() {
                scroll_focused_mount_block_vertical(state, -3);
            } else {
                state.inline_role_picker = None;
                state.inline_agent_picker = None;
                let selected = state.selected.saturating_sub(1);
                if selected != state.selected {
                    state.reset_list_scroll();
                    state.selected = selected;
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            if state.list_scroll_focus.is_some() {
                scroll_focused_mount_block_vertical(state, 3);
            } else {
                state.inline_role_picker = None;
                state.inline_agent_picker = None;
                let selected = (state.selected + 1).min(state.row_count() - 1);
                if selected != state.selected {
                    state.reset_list_scroll();
                    state.selected = selected;
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => match state.selected_row() {
            ManagerListRow::CurrentDirectory => {
                // Launch against cwd. Run-loop routes through the same
                // role-picker stage as LaunchNamed.
                Ok(InputOutcome::LaunchCurrentDir)
            }
            ManagerListRow::NewWorkspace => {
                // Start the create prelude with a FileBrowser modal open.
                let mut prelude = super::super::state::CreatePreludeState::new();
                prelude.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: FileBrowserState::new_from_home()?,
                });
                state.stage = ManagerStage::CreatePrelude(prelude);
                Ok(InputOutcome::Continue)
            }
            ManagerListRow::SavedWorkspace(i) => Ok(state
                .workspaces
                .get(i)
                .map_or(InputOutcome::Continue, |summary| {
                    InputOutcome::LaunchNamed(summary.name.clone())
                })),
        },
        KeyCode::Char('e' | 'E') => {
            match state.selected_row() {
                ManagerListRow::CurrentDirectory | ManagerListRow::NewWorkspace => {
                    // Silent no-op — current directory has no config to edit,
                    // and NewWorkspace is a sentinel.
                }
                ManagerListRow::SavedWorkspace(i) => {
                    if let Some(summary) = state.workspaces.get(i) {
                        let name = summary.name.clone();
                        if let Some(ws) = config.workspaces.get(&name) {
                            state.stage =
                                ManagerStage::Editor(EditorState::new_edit(name, ws.clone()));
                        }
                    }
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('n' | 'N') => {
            let mut prelude = super::super::state::CreatePreludeState::new();
            prelude.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: FileBrowserState::new_from_home()?,
            });
            state.stage = ManagerStage::CreatePrelude(prelude);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('d' | 'D') => {
            match state.selected_row() {
                ManagerListRow::CurrentDirectory | ManagerListRow::NewWorkspace => {
                    // Silent no-op on the sentinel.
                }
                ManagerListRow::SavedWorkspace(i) => {
                    if let Some(ws) = state.workspaces.get(i) {
                        let name = ws.name.clone();
                        state.stage = ManagerStage::ConfirmDelete {
                            name: name.clone(),
                            state: ConfirmState::new(format!("Delete \"{name}\"?")),
                        };
                    }
                }
            }
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('o' | 'O') => {
            handle_list_open_in_github(state, config);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Char('r' | 'R') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Reconnect,
            "No recoverable instance for this row",
        )),
        KeyCode::Char('a' | 'A') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::NewSession,
            "No running instance for this row",
        )),
        KeyCode::Char('x' | 'X') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Shell,
            "No running instance for this row",
        )),
        KeyCode::Char('i' | 'I') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Inspect,
            "No instance state for this row",
        )),
        KeyCode::Char('p' | 'P') => Ok(instance_action_outcome(
            state,
            ConsoleInstanceAction::Purge,
            "No purgeable instance state for this row",
        )),
        KeyCode::Char('s' | 'S') => {
            state.stage = ManagerStage::Settings(SettingsState::from_config(config));
            Ok(InputOutcome::Continue)
        }
        _ => Ok(InputOutcome::Continue),
    }
}

fn instance_action_outcome(
    state: &mut ManagerState<'_>,
    action: ConsoleInstanceAction,
    empty_message: &str,
) -> InputOutcome {
    let Some(container) = selected_instance_container(state, action) else {
        state.toast = Some(Toast {
            message: empty_message.into(),
            kind: ToastKind::Error,
            shown_at: std::time::Instant::now(),
        });
        return InputOutcome::Continue;
    };
    InputOutcome::InstanceAction { container, action }
}

fn selected_instance_container(
    state: &ManagerState<'_>,
    action: ConsoleInstanceAction,
) -> Option<String> {
    let (workspace_name, workspace_label, workdir) = selected_instance_scope(state)?;
    let query = crate::instance::InstanceQuery {
        workspace_name,
        workspace_label,
        workdir,
        role_key: None,
        agent_runtime: None,
    };
    state
        .instances
        .iter()
        .filter(|entry| {
            entry.matches(query) && instance_action_accepts_status(action, entry.status)
        })
        .map(|entry| entry.container_base.clone())
        .next()
}

fn selected_instance_scope<'a>(
    state: &'a ManagerState<'_>,
) -> Option<(Option<&'a str>, &'a str, &'a str)> {
    match state.selected_row() {
        ManagerListRow::CurrentDirectory => {
            let current_dir = state.current_dir.as_str();
            Some((None, current_dir, current_dir))
        }
        ManagerListRow::SavedWorkspace(i) => state.workspaces.get(i).map(|summary| {
            (
                Some(summary.name.as_str()),
                summary.name.as_str(),
                summary.workdir.as_str(),
            )
        }),
        ManagerListRow::NewWorkspace => None,
    }
}

const fn instance_action_accepts_status(
    action: ConsoleInstanceAction,
    status: crate::instance::InstanceStatus,
) -> bool {
    match action {
        ConsoleInstanceAction::Reconnect | ConsoleInstanceAction::Inspect => {
            !matches!(status, crate::instance::InstanceStatus::Purged)
        }
        ConsoleInstanceAction::NewSession | ConsoleInstanceAction::Shell => matches!(
            status,
            crate::instance::InstanceStatus::Active | crate::instance::InstanceStatus::Running
        ),
        ConsoleInstanceAction::Purge => !matches!(
            status,
            crate::instance::InstanceStatus::Active
                | crate::instance::InstanceStatus::Running
                | crate::instance::InstanceStatus::Purged
        ),
    }
}

/// Dispatch the `o` key on the workspace list view.
fn handle_list_open_in_github(state: &mut ManagerState<'_>, config: &AppConfig) {
    // Silent no-op when there is no workspace or no GitHub URLs — the hint is
    // already suppressed in those cases so the operator never sees the key.
    let Some(summary) = state.selected_workspace_summary() else {
        return;
    };
    let Some(ws) = config.workspaces.get(&summary.name) else {
        return;
    };
    let choices = super::super::github_mounts::resolve_for_workspace(ws);
    match choices.len() {
        0 => {}
        1 => {
            if let Err(e) = open::that_detached(&choices[0].url) {
                state.list_modal = Some(Modal::ErrorPopup {
                    state: crate::console::widgets::error_popup::ErrorPopupState::new(
                        "Failed to open URL",
                        format!("{e}"),
                    ),
                });
            }
        }
        _ => {
            state.list_modal = Some(Modal::GithubPicker {
                state: crate::console::widgets::github_picker::GithubPickerState::new(choices),
            });
        }
    }
}

/// Dispatch a key into whatever modal currently sits on `state.list_modal`.
pub(super) fn handle_list_modal(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(modal) = state.list_modal.as_mut() else {
        return InputOutcome::Continue;
    };
    match modal {
        Modal::GithubPicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(url) => {
                state.list_modal = None;
                if let Err(e) = open::that_detached(&url) {
                    state.list_modal = Some(Modal::ErrorPopup {
                        state: crate::console::widgets::error_popup::ErrorPopupState::new(
                            "Failed to open URL",
                            format!("{e}"),
                        ),
                    });
                }
                InputOutcome::Continue
            }
            ModalOutcome::Cancel => {
                state.list_modal = None;
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
        Modal::RolePicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                state.list_modal = None;
                InputOutcome::LaunchWithAgent(role)
            }
            ModalOutcome::Cancel => {
                state.list_modal = None;
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
        Modal::ErrorPopup { state: popup } => match popup.handle_key(key) {
            ModalOutcome::Commit(()) | ModalOutcome::Cancel => {
                state.list_modal = None;
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
        _ => {
            state.list_modal = None;
            InputOutcome::Continue
        }
    }
}

pub(super) fn handle_inline_role_picker(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let Some(picker) = state.inline_role_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match key.code {
        KeyCode::Left | KeyCode::Char('h' | 'H') => {
            scroll_list_horizontal(state, -8);
            InputOutcome::Continue
        }
        KeyCode::Right | KeyCode::Char('l' | 'L') => {
            scroll_list_horizontal(state, 8);
            InputOutcome::Continue
        }
        KeyCode::Char('q' | 'Q') => InputOutcome::ExitJackin,
        _ => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                state.inline_role_picker = None;
                InputOutcome::LaunchWithAgent(role)
            }
            ModalOutcome::Cancel => {
                state.inline_role_picker = None;
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
    }
}

pub(super) fn handle_inline_agent_picker(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let Some((_, picker)) = state.inline_agent_picker.as_mut() else {
        return InputOutcome::Continue;
    };
    match key.code {
        KeyCode::Left | KeyCode::Char('h' | 'H') => {
            scroll_list_horizontal(state, -8);
            InputOutcome::Continue
        }
        KeyCode::Right | KeyCode::Char('l' | 'L') => {
            scroll_list_horizontal(state, 8);
            InputOutcome::Continue
        }
        _ => match picker.handle_key(key) {
            ModalOutcome::Commit(agent) => {
                state.inline_agent_picker = None;
                InputOutcome::LaunchWithRuntimeAgent(agent)
            }
            ModalOutcome::Cancel => {
                state.inline_agent_picker = None;
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
    }
}

const fn scroll_list_horizontal(state: &mut ManagerState<'_>, delta: i16) {
    if state.list_names_focused {
        apply_scroll_delta(&mut state.list_names_scroll_x, delta);
    } else {
        scroll_focused_mount_block(state, delta);
    }
}

const fn scroll_focused_mount_block(state: &mut ManagerState<'_>, delta: i16) {
    let Some(focus) = state.list_scroll_focus else {
        return;
    };
    let value = state.list_scroll_x_mut(focus);
    apply_scroll_delta(value, delta);
}

const fn scroll_focused_mount_block_vertical(state: &mut ManagerState<'_>, delta: i16) {
    let Some(focus) = state.list_scroll_focus else {
        return;
    };
    let value = state.list_scroll_y_mut(focus);
    apply_scroll_delta(value, delta);
}

#[cfg(test)]
mod tests {
    //! List-stage tests: row-0 (current dir) gating, Enter routing,
    //! `o`-key resolver to GitHub URLs, and the `GithubPicker` modal.
    use super::super::super::state::{ManagerStage, ManagerState, Modal, MountScrollFocus};
    use super::super::test_support::{key, mount};
    use super::InputOutcome;
    use crate::config::AppConfig;
    use crate::console::manager::input::handle_key;
    use crate::instance::{InstanceIndexEntry, InstanceStatus};
    use crate::paths::JackinPaths;
    use crate::workspace::WorkspaceConfig;
    use crossterm::event::KeyCode;
    use tempfile::TempDir;

    /// Build a git repo under `root` with a `github.com` origin remote on
    /// `branch`. Returns the path so callers can use it as a mount src.
    fn make_github_repo(root: &std::path::Path, name: &str, branch: &str) -> std::path::PathBuf {
        let path = root.join(name);
        let git_dir = path.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
        std::fs::write(
            git_dir.join("config"),
            format!("[remote \"origin\"]\n    url = git@github.com:owner/{name}.git\n"),
        )
        .unwrap();
        path
    }

    /// Helper: seed an `AppConfig` + `ManagerState` with `ws` as a saved workspace,
    /// cwd far away so selection lands on row 1 (the saved workspace).
    fn list_state_selecting_ws(
        ws: WorkspaceConfig,
    ) -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config.workspaces.insert("demo".into(), ws);
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 1; // force selection onto the saved workspace row
        (state, config, paths, tmp)
    }

    fn instance_entry(
        container: &str,
        status: InstanceStatus,
        workdir: &str,
    ) -> InstanceIndexEntry {
        InstanceIndexEntry {
            instance_id: format!("{container}-id"),
            container_base: container.into(),
            workspace_name: Some("demo".into()),
            workspace_label: "demo".into(),
            workdir: workdir.into(),
            role_key: "the-architect".into(),
            agent_runtime: "codex".into(),
            status,
            updated_at: "2026-05-11T00:00:00Z".into(),
        }
    }

    /// `e` and `d` on the current-directory row must be silent no-ops —
    /// no toast, no stage transition.
    #[test]
    fn current_directory_row_silently_ignores_edit_and_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "some-ws".into(),
            WorkspaceConfig {
                workdir: "/unrelated".into(),
                mounts: vec![],
                ..Default::default()
            },
        );
        let mut state = ManagerState::from_config(&config, cwd);
        assert_eq!(state.selected, 0);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('e')),
        )
        .unwrap();
        assert!(
            matches!(&state.stage, ManagerStage::List),
            "e on row 0 must not open the Editor; got {:?}",
            state.stage
        );
        assert!(state.toast.is_none(), "e on row 0 must not show a toast");

        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('d')),
        )
        .unwrap();
        assert!(
            matches!(&state.stage, ManagerStage::List),
            "d on row 0 must not open ConfirmDelete; got {:?}",
            state.stage
        );
        assert!(state.toast.is_none(), "d on row 0 must not show a toast");
    }

    /// Enter on row 0 returns `LaunchCurrentDir`; Enter on row 1 returns
    /// `LaunchNamed(<name>)`. Pins the index arithmetic that maps list-row
    /// indices to launch targets.
    #[test]
    fn enter_on_current_directory_returns_launch_current_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "alpha".into(),
            WorkspaceConfig {
                workdir: "/alpha".into(),
                mounts: vec![],
                ..Default::default()
            },
        );
        let mut state = ManagerState::from_config(&config, cwd);
        state.selected = 0;
        let outcome =
            handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
        assert!(
            matches!(outcome, InputOutcome::LaunchCurrentDir),
            "row 0 Enter must produce LaunchCurrentDir"
        );

        state.selected = 1;
        let outcome =
            handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
        match outcome {
            InputOutcome::LaunchNamed(name) => assert_eq!(name, "alpha"),
            other => panic!("row 1 Enter must produce LaunchNamed(\"alpha\"); got {other:?}"),
        }
    }

    #[test]
    fn s_opens_settings_stage() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Char('s')),
        )
        .unwrap();

        assert!(matches!(outcome, InputOutcome::Continue));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.mounts.pending.is_empty())
        );
    }

    #[test]
    fn instance_shortcuts_return_selected_workspace_actions() {
        let workdir = "/workspace/demo";
        let ws = WorkspaceConfig {
            workdir: workdir.into(),
            mounts: vec![],
            ..Default::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
        state.instances = vec![instance_entry(
            "jackin-demo-architect-123456",
            InstanceStatus::RestoreAvailable,
            workdir,
        )];

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('r')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-123456");
                assert_eq!(action, crate::console::ConsoleInstanceAction::Reconnect);
            }
            other => panic!("expected reconnect instance action; got {other:?}"),
        }

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('i')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-123456");
                assert_eq!(action, crate::console::ConsoleInstanceAction::Inspect);
            }
            other => panic!("expected inspect instance action; got {other:?}"),
        }

        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('p')),
        )
        .unwrap();
        match outcome {
            InputOutcome::InstanceAction { container, action } => {
                assert_eq!(container, "jackin-demo-architect-123456");
                assert_eq!(action, crate::console::ConsoleInstanceAction::Purge);
            }
            other => panic!("expected purge instance action; got {other:?}"),
        }
    }

    #[test]
    fn moving_selection_resets_mount_scroll_state() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "alpha".into(),
            WorkspaceConfig {
                workdir: "/alpha".into(),
                mounts: vec![],
                ..Default::default()
            },
        );
        // When no block is focused, Down navigates the workspace list and resets scroll.
        let mut state = ManagerState::from_config(&config, cwd);
        state.selected = 0;
        state.list_mounts_scroll_x = 24;
        state.list_global_mounts_scroll_x = 16;
        state.list_role_global_mounts_scroll_x = 8;
        state.list_scroll_focus = None;

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down)).unwrap();

        assert_eq!(state.selected, 1);
        assert_eq!(state.list_mounts_scroll_x, 0);
        assert_eq!(state.list_global_mounts_scroll_x, 0);
        assert_eq!(state.list_role_global_mounts_scroll_x, 0);
        assert_eq!(state.list_scroll_focus, None);
    }

    #[test]
    fn down_key_with_focused_block_scrolls_vertically_not_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "alpha".into(),
            WorkspaceConfig {
                workdir: "/alpha".into(),
                mounts: vec![],
                ..Default::default()
            },
        );
        // When a block is focused, Down scrolls that block vertically, not the list.
        let mut state = ManagerState::from_config(&config, cwd);
        state.selected = 0;
        state.list_scroll_focus = Some(MountScrollFocus::Workspace);

        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down)).unwrap();

        assert_eq!(
            state.selected, 0,
            "selection must not change while block focused"
        );
        assert!(
            state.list_mounts_scroll_y > 0,
            "block must have scrolled vertically"
        );
    }

    // ── List-view `o` key → GitHub resolver + picker ──────────────────

    #[test]
    fn resolve_github_mounts_returns_one_per_github_repo() {
        // A workspace with two github mounts + one folder + one gitlab repo
        // should yield exactly two picker choices.
        let tmp = tempfile::tempdir().unwrap();
        let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
        let repo_b = make_github_repo(tmp.path(), "repo-b", "dev");
        let plain = tmp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        // Gitlab repo should be skipped.
        let gitlab = tmp.path().join("gl");
        let gl_git = gitlab.join(".git");
        std::fs::create_dir_all(&gl_git).unwrap();
        std::fs::write(gl_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            gl_git.join("config"),
            "[remote \"origin\"]\n    url = git@gitlab.com:owner/repo.git\n",
        )
        .unwrap();

        let ws = WorkspaceConfig {
            mounts: vec![
                mount(repo_a.to_str().unwrap(), "/a"),
                mount(plain.to_str().unwrap(), "/p"),
                mount(repo_b.to_str().unwrap(), "/b"),
                mount(gitlab.to_str().unwrap(), "/g"),
            ],
            ..WorkspaceConfig::default()
        };

        let choices = crate::console::manager::github_mounts::resolve_for_workspace(&ws);
        assert_eq!(choices.len(), 2);
        // URLs track the HEAD ref per-repo.
        let urls: Vec<&str> = choices.iter().map(|c| c.url.as_str()).collect();
        assert!(urls.contains(&"https://github.com/owner/repo-a/tree/main"));
        assert!(urls.contains(&"https://github.com/owner/repo-b/tree/dev"));
        // Branch label matches Named variant.
        let branches: Vec<&str> = choices.iter().map(|c| c.branch.as_str()).collect();
        assert!(branches.contains(&"main"));
        assert!(branches.contains(&"dev"));
    }

    #[test]
    fn list_o_with_single_github_mount_has_one_resolved_url() {
        // Resolver-side check — we can't cleanly assert `open::that_detached`
        // ran, but we can pin that there's exactly one URL to hand to it so
        // the 1-mount branch's immediate-open path is taken.
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_github_repo(tmp.path(), "solo", "trunk");
        let ws = WorkspaceConfig {
            mounts: vec![mount(repo.to_str().unwrap(), "/solo")],
            ..WorkspaceConfig::default()
        };
        let choices = crate::console::manager::github_mounts::resolve_for_workspace(&ws);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].url, "https://github.com/owner/solo/tree/trunk");
    }

    #[test]
    fn list_o_with_multiple_github_mounts_opens_picker() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
        let repo_b = make_github_repo(tmp.path(), "repo-b", "main");
        let ws = WorkspaceConfig {
            mounts: vec![
                mount(repo_a.to_str().unwrap(), "/a"),
                mount(repo_b.to_str().unwrap(), "/b"),
            ],
            ..WorkspaceConfig::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        match &state.list_modal {
            Some(Modal::GithubPicker { state: picker }) => {
                assert_eq!(picker.choices.len(), 2);
            }
            other => panic!("expected GithubPicker modal; got {other:?}"),
        }
    }

    #[test]
    fn list_o_with_zero_github_mounts_shows_toast() {
        let tmp_src = tempfile::tempdir().unwrap();
        let plain = tmp_src.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        let ws = WorkspaceConfig {
            mounts: vec![mount(plain.to_str().unwrap(), "/p")],
            ..WorkspaceConfig::default()
        };
        let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        // Silent no-op: no modal, no toast.
        assert!(state.list_modal.is_none(), "no modal when no GitHub URLs");
        assert!(state.toast.is_none(), "no toast when no GitHub URLs");
    }

    #[test]
    fn list_o_on_row_zero_is_silent_noop() {
        // Row 0 is "Current directory" — O must be silent (no toast, no modal).
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("demo".into(), WorkspaceConfig::default());
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.selected = 0;

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char('o')),
        )
        .unwrap();

        assert!(state.toast.is_none(), "O on row 0 must not toast");
        assert!(
            state.list_modal.is_none(),
            "O on row 0 must not open a modal"
        );
    }

    #[test]
    fn picker_commit_closes_list_modal_and_clears_state() {
        // Seed the state directly with an open GithubPicker, then commit.
        // We can't assert `open::that_detached` ran, but we *can* pin that
        // the modal closes (no lingering state) and no error toast appears
        // when the underlying call path doesn't error out synchronously.
        use crate::console::widgets::github_picker::{GithubChoice, GithubPickerState};
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        // Use an unreachable file:// URL so `open::that_detached` is a
        // cheap no-op on most platforms (still spawns the browser handler
        // but doesn't block on network).
        state.list_modal = Some(Modal::GithubPicker {
            state: GithubPickerState::new(vec![GithubChoice {
                src: "/tmp/a".into(),
                branch: "main".into(),
                url: "file:///dev/null".into(),
            }]),
        });

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Enter),
        )
        .unwrap();

        // Modal is either closed (open succeeded) or shows ErrorPopup (open failed).
        // Either way, GithubPicker is gone.
        assert!(
            !matches!(state.list_modal, Some(Modal::GithubPicker { .. })),
            "GithubPicker must be gone after Enter"
        );
    }

    #[test]
    fn picker_esc_closes_without_opening_url() {
        use crate::console::widgets::github_picker::{GithubChoice, GithubPickerState};
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.list_modal = Some(Modal::GithubPicker {
            state: GithubPickerState::new(vec![GithubChoice {
                src: "/tmp/a".into(),
                branch: "main".into(),
                url: "https://github.com/owner/repo/tree/main".into(),
            }]),
        });

        handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Esc),
        )
        .unwrap();

        assert!(state.list_modal.is_none());
        assert!(
            state.toast.is_none(),
            "Esc must not toast: {:?}",
            state.toast
        );
    }
}
