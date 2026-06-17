//! Create-workspace wizard: prelude stage dispatch and its multi-step
//! modal sequence (`FileBrowser` → `MountDstChoice` → [`TextInput`] →
//! `WorkdirPick` → `TextInputName`).

use crossterm::event::{KeyCode, KeyEvent};

use super::InputOutcome;
use crate::config::AppConfig;
use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::state::{ManagerState, Modal};
use crate::paths::JackinPaths;
use jackin_console::tui::components::file_browser::{
    FileBrowserOutcome, FileBrowserState, listing_rect,
};
use jackin_console::tui::components::modal_rects::{self, ModalRectMode};
use jackin_console::tui::screens::workspaces::view::{
    create_prelude_mount_destination_default, create_prelude_mount_destination_input_state,
    create_prelude_mount_dst_choice_state, create_prelude_workdir_pick_state,
    create_prelude_workspace_name_input_state,
};
use jackin_tui::ModalOutcome;

pub(super) type PreludeModalOutcome = jackin_console::tui::message::ConsolePreludeModalOutcome;

pub(super) fn handle_prelude_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> InputOutcome {
    if key.code == KeyCode::Esc {
        let _unused = update_manager(
            state,
            ManagerMessage::ReloadFromConfig {
                config: Box::new(config.clone()),
                cwd: cwd.to_path_buf(),
            },
        );
    }
    InputOutcome::Continue
}

/// Prelude-side transition: mount-src and mount-dst are both known, now
/// advance to the `PickWorkdir` step by opening a `WorkdirPick` modal.
///
/// Factored out so both the `MountDstChoice::SamePath` path (no `TextInput`) and
/// the `TextInputDst` commit path (operator edited dst) end the same way.
/// Callers are responsible for having already pushed the mount dst onto
/// the prelude (via `accept_mount_dst`).
fn prelude_advance_to_workdir_pick(
    prelude: &mut crate::console::tui::state::CreatePreludeState<'_>,
) {
    let mount = jackin_console::services::workspace::shared_mount_config(
        prelude
            .pending_mount_src
            .as_ref()
            .expect("mount src must be set before advancing to workdir pick")
            .display()
            .to_string(),
        prelude
            .pending_mount_dst
            .clone()
            .expect("mount dst must be set before advancing to workdir pick"),
        prelude.pending_readonly,
    );
    prelude.modal = Some(Modal::WorkdirPick {
        state: create_prelude_workdir_pick_state(&[mount]),
    });
}

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(super) fn handle_prelude_modal(
    prelude: &mut crate::console::tui::state::CreatePreludeState<'_>,
    key: KeyEvent,
    term_size: ratatui::layout::Rect,
) -> PreludeModalOutcome {
    use crate::console::tui::state::{FileBrowserTarget, TextInputTarget};

    // Determine which step we're on by inspecting the modal discriminant,
    // then dispatch. We do this with a discriminant enum so we can end the
    // immutable/mutable borrow on `prelude.modal` before mutating other
    // fields on `prelude` (Rust borrow rules).
    enum PreludeModalDis {
        FileBrowserSrc,
        MountDstChoice,
        TextInputDst,
        WorkdirPick,
        TextInputName,
        Other,
    }
    let dis = match &prelude.modal {
        Some(Modal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            ..
        }) => PreludeModalDis::FileBrowserSrc,
        Some(Modal::MountDstChoice {
            target: FileBrowserTarget::CreateFirstMountSrc,
            ..
        }) => PreludeModalDis::MountDstChoice,
        Some(Modal::TextInput {
            target: TextInputTarget::MountDst,
            ..
        }) => PreludeModalDis::TextInputDst,
        Some(Modal::WorkdirPick { .. }) => PreludeModalDis::WorkdirPick,
        Some(Modal::TextInput {
            target: TextInputTarget::Name,
            ..
        }) => PreludeModalDis::TextInputName,
        _ => PreludeModalDis::Other,
    };

    match dis {
        PreludeModalDis::FileBrowserSrc => {
            // Capture the current browser cwd on Commit so step-back from
            // MountDstChoice can restore it. Read before moving the
            // outcome out of `prelude.modal`.
            let (outcome, browser_cwd) =
                if let Some(Modal::FileBrowser { state, .. }) = &mut prelude.modal {
                    let cwd = state.cwd().to_path_buf();
                    let page_rows = file_browser_page_rows(term_size, state);
                    let outcome = state.handle_key_with_page_rows(key, Some(page_rows));
                    (outcome, Some(cwd))
                } else {
                    return PreludeModalOutcome::Continue;
                };
            match outcome {
                FileBrowserOutcome::Cancel => {
                    // Step 1 of the wizard — no prior state to rewind to.
                    // Close the modal; the outer dispatcher treats
                    // `modal = None + pending_name = None` as "cancelled"
                    // and drops back to the workspace list.
                    prelude.modal = None;
                }
                FileBrowserOutcome::ResolveGitUrl(path) => {
                    return PreludeModalOutcome::ResolveFileBrowserGitUrl(path);
                }
                FileBrowserOutcome::OpenGitUrl(url) => {
                    return PreludeModalOutcome::OpenUrl(url);
                }
                FileBrowserOutcome::Continue => {}
                FileBrowserOutcome::Commit(_)
                | FileBrowserOutcome::NavigateTo(_)
                | FileBrowserOutcome::NavigateUp
                | FileBrowserOutcome::RequestCommit(_) => {
                    return PreludeModalOutcome::ApplyFileBrowserOutcome {
                        outcome,
                        browser_cwd,
                    };
                }
            }

            fn file_browser_page_rows(
                term_size: ratatui::layout::Rect,
                state: &FileBrowserState,
            ) -> u16 {
                let modal_area =
                    modal_rects::modal_rect_for_mode(term_size, ModalRectMode::FileBrowser);
                let listing_area = listing_rect(modal_area, state.rejected_reason.is_some());
                u16::try_from(jackin_tui::components::viewport_height(listing_area))
                    .unwrap_or(u16::MAX)
            }
        }
        PreludeModalDis::MountDstChoice => {
            use jackin_console::tui::components::mount_dst_choice::MountDstChoice;
            let outcome = if let Some(Modal::MountDstChoice { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return PreludeModalOutcome::Continue;
            };
            match outcome {
                ModalOutcome::Commit(MountDstChoice::SamePath) => {
                    // Fast path: dst = src, skip TextInput, chain straight
                    // to WorkdirPick (mirrors the post-TextInputDst tail).
                    let default_dst = prelude.default_mount_dst();
                    prelude.modal = None;
                    prelude.used_edit_dst = false;
                    prelude.accept_mount_dst(default_dst, false);
                    prelude_advance_to_workdir_pick(prelude);
                }
                ModalOutcome::Commit(MountDstChoice::Edit) => {
                    // Re-enter today's flow: open TextInput pre-filled with
                    // the host path. The TextInputDst branch below handles
                    // the advance to WorkdirPick once the operator commits.
                    let default_dst = prelude.default_mount_dst();
                    prelude.used_edit_dst = true;
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::MountDst,
                        state: create_prelude_mount_destination_input_state(default_dst),
                    });
                }
                ModalOutcome::Cancel => {
                    // Step-back: reopen FileBrowserSrc at the last-seen
                    // browser cwd (captured when src was committed). The
                    // mount src field is left stashed so `default_mount_dst`
                    // keeps working if the operator re-commits the same path.
                    return PreludeModalOutcome::ReopenFileBrowserAtLastCwd;
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::TextInputDst => {
            let outcome = if let Some(Modal::TextInput { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return PreludeModalOutcome::Continue;
            };
            match outcome {
                ModalOutcome::Commit(dst) => {
                    prelude.modal = None;
                    // readonly defaults to false (toggle for readonly is
                    // future work — spec allows this simplification).
                    prelude.accept_mount_dst(dst, false);
                    prelude_advance_to_workdir_pick(prelude);
                }
                ModalOutcome::Cancel => {
                    // Step-back: reopen MountDstChoice with the stashed src.
                    reopen_mount_dst_choice(prelude);
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::WorkdirPick => {
            let outcome = if let Some(Modal::WorkdirPick { state }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return PreludeModalOutcome::Continue;
            };
            match outcome {
                ModalOutcome::Commit(workdir) => {
                    prelude.modal = None;
                    prelude.accept_workdir(workdir);
                    let default_name = prelude.default_name();
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::Name,
                        state: create_prelude_workspace_name_input_state(default_name),
                    });
                }
                ModalOutcome::Cancel => {
                    // Step-back: rewind to whichever dst-step the operator
                    // took — TextInputDst if they edited the destination,
                    // otherwise MountDstChoice (fast-path mount at same path).
                    if prelude.used_edit_dst {
                        let current_dst = create_prelude_mount_destination_default(
                            prelude.pending_mount_dst.as_deref(),
                        );
                        prelude.modal = Some(Modal::TextInput {
                            target: TextInputTarget::MountDst,
                            state: create_prelude_mount_destination_input_state(current_dst),
                        });
                    } else {
                        reopen_mount_dst_choice(prelude);
                    }
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::TextInputName => {
            let outcome = if let Some(Modal::TextInput { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return PreludeModalOutcome::Continue;
            };
            match outcome {
                ModalOutcome::Commit(name) => {
                    prelude.modal = None;
                    prelude.accept_name(name);
                    // Prelude complete — the outer handle_key dispatcher
                    // checks for this and transitions to Editor(Create).
                }
                ModalOutcome::Cancel => {
                    // Step-back: reopen WorkdirPick from the stashed
                    // mount src/dst — mirrors the post-TextInputDst tail.
                    prelude_advance_to_workdir_pick(prelude);
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::Other => {}
    }
    PreludeModalOutcome::Continue
}

/// Reopen the `MountDstChoice` modal seeded from the stashed mount src.
/// Used by step-back navigation from `TextInputDst` / `WorkdirPick`.
fn reopen_mount_dst_choice(prelude: &mut crate::console::tui::state::CreatePreludeState<'_>) {
    use crate::console::tui::state::FileBrowserTarget;
    let src_display = prelude
        .pending_mount_src
        .as_ref()
        .map(|p| p.display().to_string());
    let src = create_prelude_mount_destination_default(src_display.as_deref());
    prelude.modal = Some(Modal::MountDstChoice {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: create_prelude_mount_dst_choice_state(src),
    });
}

#[cfg(test)]
mod tests;
