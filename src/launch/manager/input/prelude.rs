//! Create-workspace wizard: prelude stage dispatch and its multi-step
//! modal sequence (`FileBrowser` → `MountDstChoice` → [`TextInput`] →
//! `WorkdirPick` → `TextInputName`).

use crossterm::event::{KeyCode, KeyEvent};

use super::super::super::widgets::{ModalOutcome, workdir_pick::WorkdirPickState};
use super::super::state::{ManagerState, Modal};
use super::InputOutcome;
use crate::config::AppConfig;
use crate::paths::JackinPaths;

pub(super) fn handle_prelude_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> InputOutcome {
    if key.code == KeyCode::Esc {
        *state = ManagerState::from_config(config, cwd);
    }
    InputOutcome::Continue
}

/// Prelude-side transition: mount-src and mount-dst are both known, now
/// advance to the `PickWorkdir` step by opening a `WorkdirPick` modal.
///
/// Factored out so both the `MountDstChoice::Ok` path (no `TextInput`) and
/// the `TextInputDst` commit path (operator edited dst) end the same way.
/// Callers are responsible for having already pushed the mount dst onto
/// the prelude (via `accept_mount_dst`).
fn prelude_advance_to_workdir_pick(prelude: &mut super::super::state::CreatePreludeState<'_>) {
    let mount = crate::workspace::MountConfig {
        src: prelude
            .pending_mount_src
            .as_ref()
            .expect("mount src must be set before advancing to workdir pick")
            .display()
            .to_string(),
        dst: prelude
            .pending_mount_dst
            .clone()
            .expect("mount dst must be set before advancing to workdir pick"),
        readonly: prelude.pending_readonly,
    };
    prelude.modal = Some(Modal::WorkdirPick {
        state: WorkdirPickState::from_mounts(&[mount]),
    });
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_prelude_modal(
    prelude: &mut super::super::state::CreatePreludeState<'_>,
    key: KeyEvent,
) {
    use super::super::super::widgets::text_input::TextInputState;
    use super::super::state::{FileBrowserTarget, TextInputTarget};

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
                    (state.handle_key(key), Some(cwd))
                } else {
                    return;
                };
            match outcome {
                ModalOutcome::Commit(path) => {
                    prelude.modal = None;
                    prelude.last_browser_cwd = browser_cwd;
                    prelude.accept_mount_src(path);
                    // Offer the 3-button choice: OK (dst=src, skip TextInput),
                    // Edit destination (open TextInput), or Cancel.
                    let src = prelude
                        .pending_mount_src
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    prelude.modal = Some(Modal::MountDstChoice {
                        target: FileBrowserTarget::CreateFirstMountSrc,
                        state: crate::launch::widgets::mount_dst_choice::MountDstChoiceState::new(
                            src,
                        ),
                    });
                }
                ModalOutcome::Cancel => {
                    // Step 1 of the wizard — no prior state to rewind to.
                    // Close the modal; the outer dispatcher treats
                    // `modal = None + pending_name = None` as "cancelled"
                    // and drops back to the workspace list.
                    prelude.modal = None;
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::MountDstChoice => {
            use crate::launch::widgets::mount_dst_choice::MountDstChoice;
            let outcome = if let Some(Modal::MountDstChoice { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return;
            };
            match outcome {
                ModalOutcome::Commit(MountDstChoice::Ok) => {
                    // Fast path: dst = src, skip TextInput, chain straight
                    // to WorkdirPick (mirrors the post-TextInputDst tail).
                    let default_dst = prelude.default_mount_dst().unwrap_or_default();
                    prelude.modal = None;
                    prelude.used_edit_dst = false;
                    prelude.accept_mount_dst(default_dst, false);
                    prelude_advance_to_workdir_pick(prelude);
                }
                ModalOutcome::Commit(MountDstChoice::Edit) => {
                    // Re-enter today's flow: open TextInput pre-filled with
                    // the host path. The TextInputDst branch below handles
                    // the advance to WorkdirPick once the operator commits.
                    let default_dst = prelude.default_mount_dst().unwrap_or_default();
                    prelude.used_edit_dst = true;
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::MountDst,
                        state: TextInputState::new("Destination", default_dst),
                    });
                }
                ModalOutcome::Cancel => {
                    // Step-back: reopen FileBrowserSrc at the last-seen
                    // browser cwd (captured when src was committed). The
                    // mount src field is left stashed so `default_mount_dst`
                    // keeps working if the operator re-commits the same path.
                    reopen_file_browser_at_last_cwd(prelude);
                }
                ModalOutcome::Continue => {}
            }
        }
        PreludeModalDis::TextInputDst => {
            let outcome = if let Some(Modal::TextInput { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return;
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
                return;
            };
            match outcome {
                ModalOutcome::Commit(workdir) => {
                    prelude.modal = None;
                    prelude.accept_workdir(workdir);
                    let default_name = prelude.default_name().unwrap_or_default();
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::Name,
                        state: TextInputState::new("Name this workspace", default_name),
                    });
                }
                ModalOutcome::Cancel => {
                    // Step-back: rewind to whichever dst-step the operator
                    // took — TextInputDst if they edited the destination,
                    // otherwise MountDstChoice (fast-path OK).
                    if prelude.used_edit_dst {
                        let current_dst = prelude.pending_mount_dst.clone().unwrap_or_default();
                        prelude.modal = Some(Modal::TextInput {
                            target: TextInputTarget::MountDst,
                            state: TextInputState::new("Destination", current_dst),
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
                return;
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
}

/// Reopen the `FileBrowserSrc` modal positioned at the last-seen cwd.
/// Used by step-back navigation from `MountDstChoice`. Silently starts at
/// `$HOME` when the browser fails to build or no cwd was recorded.
fn reopen_file_browser_at_last_cwd(prelude: &mut super::super::state::CreatePreludeState<'_>) {
    use super::super::state::FileBrowserTarget;
    let Ok(mut fb) = crate::launch::widgets::file_browser::FileBrowserState::new_from_home() else {
        prelude.modal = None;
        return;
    };
    if let Some(cwd) = prelude.last_browser_cwd.as_ref() {
        fb.set_cwd(cwd);
    }
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: fb,
    });
}

/// Reopen the `MountDstChoice` modal seeded from the stashed mount src.
/// Used by step-back navigation from `TextInputDst` / `WorkdirPick`.
fn reopen_mount_dst_choice(prelude: &mut super::super::state::CreatePreludeState<'_>) {
    use super::super::state::FileBrowserTarget;
    let src = prelude
        .pending_mount_src
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    prelude.modal = Some(Modal::MountDstChoice {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: crate::launch::widgets::mount_dst_choice::MountDstChoiceState::new(src),
    });
}
