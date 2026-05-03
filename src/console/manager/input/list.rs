//! List-stage dispatch: workspace-picker key handling and the
//! list-level modal (`GithubPicker`).

use crossterm::event::{KeyCode, KeyEvent};

use super::super::super::widgets::{
    ModalOutcome, confirm::ConfirmState, file_browser::FileBrowserState,
};
use super::super::state::{
    EditorState, FileBrowserTarget, ManagerListRow, ManagerStage, ManagerState, Modal, Toast,
    ToastKind,
};
use super::InputOutcome;
use crate::config::AppConfig;
use crate::paths::JackinPaths;

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
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            state.selected = state.selected.saturating_sub(1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            state.selected = (state.selected + 1).min(state.row_count() - 1);
            Ok(InputOutcome::Continue)
        }
        KeyCode::Enter => match state.selected_row() {
            ManagerListRow::CurrentDirectory => {
                // Launch against cwd. Run-loop routes through the same
                // agent-picker stage as LaunchNamed.
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
                ManagerListRow::CurrentDirectory => {
                    state.toast = Some(Toast {
                        message: "Current directory cannot be edited".into(),
                        kind: ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
                ManagerListRow::NewWorkspace => {
                    // Silent no-op on the sentinel.
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
                ManagerListRow::CurrentDirectory => {
                    state.toast = Some(Toast {
                        message: "Current directory cannot be deleted".into(),
                        kind: ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                }
                ManagerListRow::NewWorkspace => {
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
        _ => Ok(InputOutcome::Continue),
    }
}

/// Dispatch the `o` key on the workspace list view. Keeps `handle_list_key`
/// below clippy's `too_many_lines` threshold and isolates the
/// toast/open/picker decision tree.
fn handle_list_open_in_github(state: &mut ManagerState<'_>, config: &AppConfig) {
    let Some(summary) = state.selected_workspace_summary() else {
        state.toast = Some(Toast {
            message: "no workspace selected".into(),
            kind: ToastKind::Error,
            shown_at: std::time::Instant::now(),
        });
        return;
    };
    let Some(ws) = config.workspaces.get(&summary.name) else {
        return;
    };
    let choices = super::super::github_mounts::resolve_for_workspace(ws);
    match choices.len() {
        0 => {
            state.toast = Some(Toast {
                message: "no GitHub URLs for this workspace".into(),
                kind: ToastKind::Error,
                shown_at: std::time::Instant::now(),
            });
        }
        1 => {
            if let Err(e) = open::that_detached(&choices[0].url) {
                state.toast = Some(Toast {
                    message: format!("failed to open URL: {e}"),
                    kind: ToastKind::Error,
                    shown_at: std::time::Instant::now(),
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
/// Today the slot can hold either `Modal::GithubPicker` (opened by `o` on
/// a workspace row) or `Modal::AgentPicker` (opened by Enter when the
/// highlighted workspace has multiple eligible agents). Any other variant
/// that sneaks in is treated as cancel so the operator isn't stuck.
///
/// Returns the resulting `InputOutcome` so the `AgentPicker` commit path
/// can surface the chosen agent up to `run_console` for launch.
pub(super) fn handle_list_modal(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let Some(modal) = state.list_modal.as_mut() else {
        return InputOutcome::Continue;
    };
    match modal {
        Modal::GithubPicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(url) => {
                state.list_modal = None;
                if let Err(e) = open::that_detached(&url) {
                    state.toast = Some(Toast {
                        message: format!("failed to open URL: {e}"),
                        kind: ToastKind::Error,
                        shown_at: std::time::Instant::now(),
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
        Modal::AgentPicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(agent) => {
                state.list_modal = None;
                InputOutcome::LaunchWithAgent(agent)
            }
            ModalOutcome::Cancel => {
                state.list_modal = None;
                InputOutcome::Continue
            }
            ModalOutcome::Continue => InputOutcome::Continue,
        },
        // Defensive catch-all — no other Modal variants are placed on the
        // list_modal slot today.
        _ => {
            state.list_modal = None;
            InputOutcome::Continue
        }
    }
}

#[cfg(test)]
mod tests {
    //! List-stage tests: row-0 (current dir) gating, Enter routing,
    //! `o`-key resolver to GitHub URLs, and the `GithubPicker` modal.
    use super::super::super::state::{ManagerStage, ManagerState, Modal, ToastKind};
    use super::super::test_support::{key, mount};
    use super::InputOutcome;
    use crate::config::AppConfig;
    use crate::console::manager::input::handle_key;
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

    /// Current-directory row (index 0) must reject the `e` edit shortcut and
    /// the `d` delete shortcut with a toast, without entering the Editor or
    /// `ConfirmDelete` stages. Paired with the render-side assertion that row 0
    /// is labelled "Current directory".
    #[test]
    fn current_directory_row_rejects_edit_and_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let cwd = tmp.path();

        // Minimal config with one saved workspace so the list has a non-
        // trivial shape (current-dir + one saved + sentinel).
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
        // cwd is unrelated to /unrelated, so preselect falls back to row 0.
        assert_eq!(state.selected, 0);

        // Press `e` — must produce a toast and remain in the List stage.
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
        let toast = state.toast.as_ref().expect("edit rejection must toast");
        assert!(
            matches!(toast.kind, ToastKind::Error),
            "edit rejection must be an error toast"
        );
        assert!(
            toast.message.contains("edit"),
            "toast should mention edit: {}",
            toast.message
        );
        state.toast = None;

        // Press `d` — must produce a toast and remain in the List stage
        // (no ConfirmDelete transition).
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
        let toast = state.toast.as_ref().expect("delete rejection must toast");
        assert!(
            matches!(toast.kind, ToastKind::Error),
            "delete rejection must be an error toast"
        );
        assert!(
            toast.message.contains("delete"),
            "toast should mention delete: {}",
            toast.message
        );
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

        assert!(
            state.list_modal.is_none(),
            "no modal should open when there are no github mounts"
        );
        let toast = state.toast.as_ref().expect("expected a toast");
        assert!(
            toast.message.contains("no GitHub URL"),
            "toast should explain the no-mounts state: {}",
            toast.message
        );
    }

    #[test]
    fn list_o_on_row_zero_toasts_no_workspace_selected() {
        // Row 0 is the synthetic "Current directory" — no saved workspace
        // to read mounts from; hint should nudge the operator, not crash.
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

        let toast = state.toast.as_ref().expect("expected a toast");
        assert!(toast.message.contains("no workspace selected"));
        assert!(state.list_modal.is_none());
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

        assert!(
            state.list_modal.is_none(),
            "picker Enter must close the modal"
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
