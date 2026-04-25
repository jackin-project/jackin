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
                        state: crate::console::widgets::mount_dst_choice::MountDstChoiceState::new(
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
            use crate::console::widgets::mount_dst_choice::MountDstChoice;
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
    let Ok(mut fb) = crate::console::widgets::file_browser::FileBrowserState::new_from_home()
    else {
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
        state: crate::console::widgets::mount_dst_choice::MountDstChoiceState::new(src),
    });
}

#[cfg(test)]
mod tests {
    //! Create-wizard tests: the prelude's multi-step modal sequence
    //! (FileBrowserSrc → MountDstChoice → TextInputDst → WorkdirPick →
    //! TextInputName) and its step-back / Esc semantics.
    use super::super::super::state::{FileBrowserTarget, Modal};
    use super::super::test_support::key;
    use super::handle_prelude_modal;
    use crossterm::event::KeyCode;

    /// Seed a `CreatePreludeState` whose `MountDstChoice` modal is open
    /// for `src`. Mirrors the state the `FileBrowserSrc::Commit` branch of
    /// `handle_prelude_modal` leaves the prelude in, without needing to
    /// synthesise a FileBrowser `Commit(path)` event (no public way to do
    /// that cleanly from outside the widget).
    fn prelude_with_browser_committed(
        src: &str,
    ) -> super::super::super::state::CreatePreludeState<'static> {
        let mut prelude = super::super::super::state::CreatePreludeState::new();
        prelude.accept_mount_src(std::path::PathBuf::from(src));
        prelude.modal = Some(Modal::MountDstChoice {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: crate::console::widgets::mount_dst_choice::MountDstChoiceState::new(src),
        });
        prelude
    }

    #[test]
    fn prelude_ok_chains_to_workdir_pick_with_dst_equal_src() {
        // OK on the choice modal should: (a) set prelude.pending_mount_dst
        // to src, (b) advance the step to PickWorkdir, (c) open the
        // WorkdirPick modal pre-loaded with the staged mount.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('o')));

        assert!(
            matches!(prelude.modal, Some(Modal::WorkdirPick { .. })),
            "OK must chain to WorkdirPick; got {:?}",
            prelude.modal
        );
        assert_eq!(
            prelude.pending_mount_dst.as_deref(),
            Some("/home/user/project"),
            "OK fast-path stores dst = src on the prelude"
        );
        assert!(!prelude.pending_readonly);
        assert!(matches!(
            prelude.step,
            super::super::super::state::CreateStep::PickWorkdir
        ));
    }

    #[test]
    fn prelude_edit_opens_textinput_preserving_chain_to_workdir_pick() {
        // Edit destination on the choice modal must open a TextInput
        // pre-filled with the src (today's flow). The TextInputDst
        // commit branch then advances to WorkdirPick — so this test pins
        // that the Edit-path does not short-circuit; the chain continues
        // through TextInput like before.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));

        match &prelude.modal {
            Some(Modal::TextInput { target, .. }) => {
                assert_eq!(
                    target,
                    &super::super::super::state::TextInputTarget::MountDst
                );
            }
            other => panic!("expected TextInput(MountDst); got {other:?}"),
        }
        // Edit must not itself store a dst — the TextInput commit will.
        assert!(prelude.pending_mount_dst.is_none());
        // The prelude's internal step is still PickFirstMountDst (not
        // advanced yet) — TextInput commit is what calls accept_mount_dst.
        assert!(matches!(
            prelude.step,
            super::super::super::state::CreateStep::PickFirstMountDst
        ));
    }

    #[test]
    fn prelude_cancel_on_mount_dst_choice_rewinds_to_file_browser() {
        // Esc on MountDstChoice must not close the wizard — it must
        // step back to FileBrowserSrc so the operator can pick a
        // different source folder without losing state.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            matches!(prelude.modal, Some(Modal::FileBrowser { .. })),
            "Esc on MountDstChoice must reopen FileBrowser; got {:?}",
            prelude.modal
        );
        assert!(
            prelude.pending_mount_dst.is_none(),
            "Cancel must not store a dst"
        );
    }

    #[test]
    fn prelude_esc_at_mount_dst_choice_returns_to_file_browser_at_last_cwd() {
        // Step-back from MountDstChoice must reopen FileBrowser seeded at
        // the last cwd the browser was pointing at when src was committed.
        // The FileBrowser root is always `$HOME`, so the restored cwd has
        // to live inside `$HOME` — we use `$HOME` itself which is always
        // a valid target for `set_cwd` to honour.
        let home = directories::BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .expect("resolve $HOME");

        let mut prelude = super::super::super::state::CreatePreludeState::new();
        prelude.accept_mount_src(home.clone());
        prelude.last_browser_cwd = Some(home.clone());
        prelude.modal = Some(Modal::MountDstChoice {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: crate::console::widgets::mount_dst_choice::MountDstChoiceState::new(
                &home.display().to_string(),
            ),
        });

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));

        match &prelude.modal {
            Some(Modal::FileBrowser { state, .. }) => {
                let cwd = state.cwd().to_path_buf();
                assert!(
                    cwd == home || cwd.starts_with(&home),
                    "FileBrowser should restore a cwd inside $HOME (got {cwd:?})"
                );
            }
            other => panic!("expected FileBrowser, got {other:?}"),
        }
    }

    #[test]
    fn prelude_esc_at_text_input_dst_returns_to_mount_dst_choice() {
        // Tapping "Edit destination" opens TextInputDst; Esc inside that
        // TextInput must rewind to the MountDstChoice modal — not close
        // the wizard.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        // Choose the Edit branch to open the TextInput.
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));
        assert!(matches!(prelude.modal, Some(Modal::TextInput { .. })));

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            matches!(prelude.modal, Some(Modal::MountDstChoice { .. })),
            "Esc on TextInputDst must reopen MountDstChoice; got {:?}",
            prelude.modal
        );
    }

    #[test]
    fn prelude_esc_at_workdir_pick_returns_to_mount_dst_choice_fast_path() {
        // When the operator took the OK (fast path) for dst, Esc on
        // WorkdirPick must step back to MountDstChoice.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('o'))); // OK → WorkdirPick
        assert!(matches!(prelude.modal, Some(Modal::WorkdirPick { .. })));

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            matches!(prelude.modal, Some(Modal::MountDstChoice { .. })),
            "Esc on WorkdirPick (fast-path) must rewind to MountDstChoice; got {:?}",
            prelude.modal
        );
    }

    #[test]
    fn prelude_esc_at_workdir_pick_returns_to_text_input_dst_when_edit_used() {
        // When the operator took the Edit branch, Esc on WorkdirPick must
        // rewind to the TextInputDst step so they can retry the typed dst.
        let mut prelude = prelude_with_browser_committed("/home/user/project");
        handle_prelude_modal(&mut prelude, key(KeyCode::Char('e'))); // open TextInputDst
        // Simulate commit of typed dst (Enter closes TextInput) by
        // advancing the modal directly to WorkdirPick — we only care
        // about `used_edit_dst` state at this point.
        prelude.used_edit_dst = true;
        prelude.accept_mount_dst("/home/user/project".into(), false);
        prelude.modal = Some(Modal::WorkdirPick {
            state: crate::console::widgets::workdir_pick::WorkdirPickState::from_mounts(&[
                crate::workspace::MountConfig {
                    src: "/home/user/project".into(),
                    dst: "/home/user/project".into(),
                    readonly: false,
                },
            ]),
        });

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        match &prelude.modal {
            Some(Modal::TextInput { target, .. }) => {
                assert_eq!(
                    target,
                    &super::super::super::state::TextInputTarget::MountDst
                );
            }
            other => panic!("expected TextInput(MountDst); got {other:?}"),
        }
    }

    #[test]
    fn prelude_esc_at_name_step_returns_to_workdir_pick() {
        // Name is the last step in the wizard — Esc on TextInputName
        // must rewind to WorkdirPick so the operator can change the
        // workdir without abandoning the partial workspace.
        let mut prelude = super::super::super::state::CreatePreludeState::new();
        prelude.accept_mount_src(std::path::PathBuf::from("/home/user/project"));
        prelude.accept_mount_dst("/home/user/project".into(), false);
        prelude.accept_workdir("/home/user/project".into());
        prelude.modal = Some(Modal::TextInput {
            target: super::super::super::state::TextInputTarget::Name,
            state: crate::console::widgets::text_input::TextInputState::new(
                "Name this workspace",
                "project",
            ),
        });

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            matches!(prelude.modal, Some(Modal::WorkdirPick { .. })),
            "Esc on TextInputName must reopen WorkdirPick; got {:?}",
            prelude.modal
        );
        assert!(prelude.pending_name.is_none(), "Esc must not commit a name");
    }

    #[test]
    fn prelude_esc_at_file_browser_src_returns_to_list() {
        // Step 1 (FileBrowserSrc) has no prior state to restore — Esc
        // must close the modal so the outer dispatcher drops back to
        // the workspace list (today's "cancelled" contract).
        let mut prelude = super::super::super::state::CreatePreludeState::new();
        let fb = crate::console::widgets::file_browser::FileBrowserState::new_from_home()
            .expect("file browser should build in test env");
        prelude.modal = Some(Modal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: fb,
        });

        handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
        assert!(
            prelude.modal.is_none(),
            "Esc on FileBrowserSrc must close the modal; got {:?}",
            prelude.modal
        );
        assert!(prelude.pending_name.is_none());
    }
}
