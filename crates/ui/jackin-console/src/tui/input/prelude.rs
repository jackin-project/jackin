// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Create-workspace wizard: prelude stage dispatch and its multi-step
//! modal sequence (`FileBrowser` → `MountDstChoice` → [`TextInput`] →
//! `WorkdirPick` → `TextInputName`).

use crossterm::event::KeyEvent;

use super::InputOutcome;
use crate::tui::components::file_browser::page_rows_for_modal;
use crate::tui::model::{
    CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE, CREATE_PRELUDE_STEP_MOUNT_DST_EDIT,
    CREATE_PRELUDE_STEP_MOUNT_SRC, CREATE_PRELUDE_STEP_NAME, CREATE_PRELUDE_STEP_WORKDIR,
    CreatePreludeFileBrowserPlan, CreatePreludeKeyPlan, CreatePreludeMountDstChoicePlan,
    CreatePreludeTextInputDstPlan, CreatePreludeTextInputNamePlan, CreatePreludeWorkdirPickPlan,
    create_prelude_file_browser_plan, create_prelude_key_plan,
    create_prelude_mount_dst_choice_plan, create_prelude_text_input_dst_plan,
    create_prelude_text_input_name_plan, create_prelude_workdir_pick_plan,
};
use crate::tui::screens::workspaces::view::{
    create_prelude_mount_destination_default, create_prelude_mount_destination_input_state,
    create_prelude_mount_dst_choice_state, create_prelude_workdir_pick_state,
    create_prelude_workspace_name_input_state,
};
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{ManagerState, Modal};
use jackin_config::AppConfig;
use jackin_core::JackinPaths;
pub type PreludeModalOutcome = crate::tui::message::ConsolePreludeModalOutcome;

pub fn handle_prelude_key(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    _paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> InputOutcome {
    match create_prelude_key_plan(key.code) {
        CreatePreludeKeyPlan::ReturnToList => {
            update_manager(
                state,
                ManagerMessage::ReloadFromConfig {
                    config: Box::new(config.clone()),
                    cwd: cwd.to_path_buf(),
                },
            );
        }
        CreatePreludeKeyPlan::Continue => {}
    }
    InputOutcome::Continue
}

/// Wizard/modal agreement invariant: while a modal is open, the wizard's
/// current step index must name the step that modal belongs to. Steps with
/// no open modal (cancelled, or completed before the outer dispatcher
/// reacts) are exempt.
///
/// Compiled unconditionally: `debug_assert!` still type-checks its
/// expression in release builds, so a `#[cfg(debug_assertions)]` helper
/// breaks `cargo build --release` (E0425, seen in the preview workflow's
/// cross-build).
fn create_prelude_step_matches_modal(step: usize, modal: Option<&Modal<'_>>) -> bool {
    use crate::tui::state::{FileBrowserTarget, Modal, TextInputTarget};
    let Some(modal) = modal else {
        return true;
    };
    match step {
        CREATE_PRELUDE_STEP_MOUNT_SRC => matches!(
            modal,
            Modal::FileBrowser {
                target: FileBrowserTarget::CreateFirstMountSrc,
                ..
            }
        ),
        CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE => {
            matches!(modal, Modal::MountDstChoice { .. })
        }
        CREATE_PRELUDE_STEP_MOUNT_DST_EDIT => matches!(
            modal,
            Modal::TextInput {
                target: TextInputTarget::MountDst,
                ..
            }
        ),
        CREATE_PRELUDE_STEP_WORKDIR => matches!(modal, Modal::WorkdirPick { .. }),
        CREATE_PRELUDE_STEP_NAME => matches!(
            modal,
            Modal::TextInput {
                target: TextInputTarget::Name,
                ..
            }
        ),
        _ => false,
    }
}

pub fn handle_prelude_modal(
    prelude: &mut crate::tui::state::CreatePreludeState<'_>,
    key: KeyEvent,
    term_size: ratatui::layout::Rect,
) -> PreludeModalOutcome {
    use crate::tui::state::TextInputTarget;

    debug_assert!(
        create_prelude_step_matches_modal(prelude.wizard.step(), prelude.modal.as_ref()),
        "create-prelude wizard step and open modal disagree"
    );

    match prelude.wizard.step() {
        CREATE_PRELUDE_STEP_MOUNT_SRC => {
            // Capture the current browser cwd on Commit so step-back from
            // MountDstChoice can restore it. Read before moving the
            // outcome out of `prelude.modal`.
            let (outcome, browser_cwd) =
                if let Some(Modal::FileBrowser { state, .. }) = &mut prelude.modal {
                    let cwd = state.cwd().to_path_buf();
                    let page_rows = page_rows_for_modal(term_size, state);
                    let outcome = state.handle_key_with_page_rows(key, Some(page_rows));
                    (outcome, Some(cwd))
                } else {
                    return PreludeModalOutcome::Continue;
                };
            match create_prelude_file_browser_plan(outcome) {
                CreatePreludeFileBrowserPlan::CancelPrelude => {
                    // Step 1 of the wizard — no prior state to rewind to.
                    // Close the modal; the outer dispatcher treats
                    // `modal = None + pending_name = None` as "cancelled"
                    // and drops back to the workspace list.
                    prelude.wizard.cancel();
                    prelude.modal = None;
                }
                CreatePreludeFileBrowserPlan::ResolveGitUrl(path) => {
                    return PreludeModalOutcome::ResolveFileBrowserGitUrl(path);
                }
                CreatePreludeFileBrowserPlan::OpenUrl(url) => {
                    return PreludeModalOutcome::OpenUrl(url);
                }
                CreatePreludeFileBrowserPlan::ApplyFileBrowserOutcome(outcome) => {
                    return PreludeModalOutcome::ApplyFileBrowserOutcome {
                        outcome,
                        browser_cwd,
                    };
                }
                CreatePreludeFileBrowserPlan::Continue => {}
            }
        }
        CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE => {
            let outcome = if let Some(Modal::MountDstChoice { state, .. }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return PreludeModalOutcome::Continue;
            };
            match create_prelude_mount_dst_choice_plan(outcome) {
                CreatePreludeMountDstChoicePlan::CommitSamePath => {
                    // Fast path: dst = src, skip TextInput, chain straight
                    // to WorkdirPick (mirrors the post-TextInputDst tail).
                    // `skip()` only marks the *current* step, so the wizard
                    // advance is next() onto `mount-dst-edit` then skip().
                    let default_dst = prelude.default_mount_dst();
                    prelude.modal = None;
                    prelude.used_edit_dst = false;
                    prelude.accept_mount_dst(default_dst, false);
                    prelude.wizard.next();
                    prelude.wizard.skip();
                    open_workdir_pick_from_pending_mount(prelude);
                }
                CreatePreludeMountDstChoicePlan::OpenEditInput => {
                    // Re-enter today's flow: open TextInput pre-filled with
                    // the host path. The TextInputDst branch below handles
                    // the advance to WorkdirPick once the operator commits.
                    let default_dst = prelude.default_mount_dst();
                    prelude.used_edit_dst = true;
                    prelude.wizard.next();
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::MountDst,
                        state: create_prelude_mount_destination_input_state(default_dst),
                    });
                }
                CreatePreludeMountDstChoicePlan::ReopenFileBrowserAtLastCwd => {
                    // Step-back: reopen FileBrowserSrc at the last-seen
                    // browser cwd (captured when src was committed). The
                    // mount src field is left stashed so `default_mount_dst`
                    // keeps working if the operator re-commits the same path.
                    prelude.wizard.back();
                    return PreludeModalOutcome::ReopenFileBrowserAtLastCwd;
                }
                CreatePreludeMountDstChoicePlan::Continue => {}
            }
        }
        CREATE_PRELUDE_STEP_MOUNT_DST_EDIT => {
            let outcome = if let Some(Modal::TextInput { state, .. }) = &mut prelude.modal {
                state.handle_key(key.into())
            } else {
                return PreludeModalOutcome::Continue;
            };
            match create_prelude_text_input_dst_plan(outcome) {
                CreatePreludeTextInputDstPlan::Commit(dst) => {
                    prelude.modal = None;
                    // readonly defaults to false (toggle for readonly is
                    // future work — spec allows this simplification).
                    prelude.accept_mount_dst(dst, false);
                    prelude.wizard.next();
                    open_workdir_pick_from_pending_mount(prelude);
                }
                CreatePreludeTextInputDstPlan::ReopenMountDstChoice => {
                    // Step-back: reopen MountDstChoice with the stashed src.
                    prelude.wizard.back();
                    reopen_mount_dst_choice(prelude);
                }
                CreatePreludeTextInputDstPlan::Continue => {}
            }
        }
        CREATE_PRELUDE_STEP_WORKDIR => {
            let outcome = if let Some(Modal::WorkdirPick { state }) = &mut prelude.modal {
                state.handle_key(key)
            } else {
                return PreludeModalOutcome::Continue;
            };
            // Step-back rewinds to TextInputDst when the operator edited the
            // destination, otherwise MountDstChoice for the same-path branch.
            // Consumer compensation: upstream `back()` decrements
            // unconditionally and would land on the skipped `mount-dst-edit`
            // step, so the rewind uses a backward `jump_to` instead
            // (permitted under `linear`).
            match create_prelude_workdir_pick_plan(outcome, prelude.used_edit_dst) {
                CreatePreludeWorkdirPickPlan::Commit(workdir) => {
                    prelude.modal = None;
                    prelude.accept_workdir(workdir);
                    prelude.wizard.next();
                    let default_name = prelude.default_name();
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::Name,
                        state: create_prelude_workspace_name_input_state(default_name),
                    });
                }
                CreatePreludeWorkdirPickPlan::ReopenTextInputDst => {
                    prelude.wizard.jump_to(CREATE_PRELUDE_STEP_MOUNT_DST_EDIT);
                    let current_dst = create_prelude_mount_destination_default(
                        prelude.pending_mount_dst.as_deref(),
                    );
                    prelude.modal = Some(Modal::TextInput {
                        target: TextInputTarget::MountDst,
                        state: create_prelude_mount_destination_input_state(current_dst),
                    });
                }
                CreatePreludeWorkdirPickPlan::ReopenMountDstChoice => {
                    prelude.wizard.jump_to(CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE);
                    reopen_mount_dst_choice(prelude);
                }
                CreatePreludeWorkdirPickPlan::Continue => {}
            }
        }
        CREATE_PRELUDE_STEP_NAME => {
            let outcome = if let Some(Modal::TextInput { state, .. }) = &mut prelude.modal {
                state.handle_key(key.into())
            } else {
                return PreludeModalOutcome::Continue;
            };
            match create_prelude_text_input_name_plan(outcome) {
                CreatePreludeTextInputNamePlan::Commit(name) => {
                    prelude.modal = None;
                    prelude.accept_name(name);
                    // Last step with review disabled ⇒ SubmitRequested;
                    // the outer handle_key dispatcher detects completion
                    // (modal closed + completed()) and transitions to
                    // Editor(Create) — unchanged.
                    prelude.wizard.next();
                }
                CreatePreludeTextInputNamePlan::ReopenWorkdirPick => {
                    // Step-back: reopen WorkdirPick from the stashed
                    // mount src/dst — mirrors the post-TextInputDst tail.
                    prelude.wizard.back();
                    open_workdir_pick_from_pending_mount(prelude);
                }
                CreatePreludeTextInputNamePlan::Continue => {}
            }
        }
        _ => {}
    }
    PreludeModalOutcome::Continue
}

/// Reopen the `MountDstChoice` modal seeded from the stashed mount src.
/// Used by step-back navigation from `TextInputDst` / `WorkdirPick`.
fn reopen_mount_dst_choice(prelude: &mut crate::tui::state::CreatePreludeState<'_>) {
    use crate::tui::state::FileBrowserTarget;
    prelude.reopen_mount_dst_choice(|src| Modal::MountDstChoice {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: create_prelude_mount_dst_choice_state(src),
    });
}

fn open_workdir_pick_from_pending_mount(prelude: &mut crate::tui::state::CreatePreludeState<'_>) {
    prelude.open_workdir_pick_from_pending_mount(|mount| Modal::WorkdirPick {
        state: create_prelude_workdir_pick_state(&[mount]),
    });
}

#[cfg(test)]
mod tests;
