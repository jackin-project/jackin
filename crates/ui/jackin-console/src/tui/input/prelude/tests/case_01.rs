// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn prelude_mount_same_path_chains_to_workdir_pick_with_dst_equal_src() {
    // Mount-at-same-path on the choice modal should: (a) set prelude.pending_mount_dst
    // to src, (b) advance the step to PickWorkdir, (c) open the
    // WorkdirPick modal pre-loaded with the staged mount.
    let mut prelude = prelude_with_browser_committed("/home/user/project");
    handle_prelude_modal(&mut prelude, key(KeyCode::Char('m')));

    assert!(
        matches!(prelude.modal, Some(Modal::WorkdirPick { .. })),
        "Mount at same path must chain to WorkdirPick; got {:?}",
        prelude.modal
    );
    assert_eq!(
        prelude.pending_mount_dst.as_deref(),
        Some("/home/user/project"),
        "Mount-at-same-path fast path stores dst = src on the prelude"
    );
    assert!(!prelude.pending_readonly);
    assert_eq!(prelude.wizard.step(), CREATE_PRELUDE_STEP_WORKDIR);
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
            assert_eq!(target, &crate::tui::state::TextInputTarget::MountDst);
        }
        other => panic!("expected TextInput(MountDst); got {other:?}"),
    }
    // Edit must not itself store a dst — the TextInput commit will.
    assert!(prelude.pending_mount_dst.is_none());
    // The wizard sits on the `mount-dst-edit` step — the TextInputDst
    // commit is what advances to `workdir`.
    assert_eq!(prelude.wizard.step(), CREATE_PRELUDE_STEP_MOUNT_DST_EDIT);
}

#[test]
fn prelude_cancel_on_mount_dst_choice_rewinds_to_file_browser() {
    // Esc on MountDstChoice must not close the wizard — it must
    // step back to FileBrowserSrc so the operator can pick a
    // different source folder without losing state.
    let mut prelude = prelude_with_browser_committed("/home/user/project");
    handle_prelude_modal_with_effects(&mut prelude, key(KeyCode::Esc));
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

    let mut prelude = crate::tui::state::CreatePreludeState::new();
    prelude.accept_mount_src(home.clone());
    prelude.wizard.next();
    prelude.last_browser_cwd = Some(home.clone());
    prelude.modal = Some(Modal::MountDstChoice {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: create_prelude_mount_dst_choice_state(home.display().to_string()),
    });

    handle_prelude_modal_with_effects(&mut prelude, key(KeyCode::Esc));

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
    // When the operator took the mount-at-same-path fast path for dst, Esc on
    // WorkdirPick must step back to MountDstChoice.
    let mut prelude = prelude_with_browser_committed("/home/user/project");
    handle_prelude_modal(&mut prelude, key(KeyCode::Char('m'))); // same path → WorkdirPick
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
    // about `used_edit_dst` state at this point. The wizard is on
    // `mount-dst-edit` after the Edit key; the dst commit advances it.
    prelude.used_edit_dst = true;
    prelude.accept_mount_dst("/home/user/project".into(), false);
    prelude.wizard.next();
    prelude.modal = Some(Modal::WorkdirPick {
        state: create_prelude_workdir_pick_state(&[jackin_config::MountConfig {
            src: "/home/user/project".into(),
            dst: "/home/user/project".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        }]),
    });

    handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
    match &prelude.modal {
        Some(Modal::TextInput { target, .. }) => {
            assert_eq!(target, &crate::tui::state::TextInputTarget::MountDst);
        }
        other => panic!("expected TextInput(MountDst); got {other:?}"),
    }
}

#[test]
fn prelude_esc_at_name_step_returns_to_workdir_pick() {
    // Name is the last step in the wizard — Esc on TextInputName
    // must rewind to WorkdirPick so the operator can change the
    // workdir without abandoning the partial workspace.
    let mut prelude = crate::tui::state::CreatePreludeState::new();
    prelude.accept_mount_src(std::path::PathBuf::from("/home/user/project"));
    prelude.accept_mount_dst("/home/user/project".into(), false);
    prelude.accept_workdir("/home/user/project".into());
    // Same-path walk: src commit → SamePath (skip `mount-dst-edit`) →
    // workdir commit ⇒ wizard on `name`.
    prelude.wizard.next();
    prelude.wizard.next();
    prelude.wizard.skip();
    prelude.wizard.next();
    prelude.modal = Some(Modal::TextInput {
        target: crate::tui::state::TextInputTarget::Name,
        state: create_prelude_workspace_name_input_state("project"),
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
    let mut prelude = crate::tui::state::CreatePreludeState::new();
    let fb = crate::tui::components::file_browser::FileBrowserState::from_listing(
        crate::services::file_browser::listing_from_home()
            .expect("file browser should build in test env"),
    );
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

#[test]
fn golden_walk_1_same_path_forward_completes() {
    // FileBrowser commit (seeded) → SamePath → WorkdirPick commit →
    // TextInputName commit ⇒ Complete, dst = src, workdir = src,
    // name = dst basename.
    let mut prelude = prelude_with_browser_committed(GOLDEN_SRC);
    let mut steps = vec![observed_step(&prelude)];
    assert_eq!(
        observed_completion(&prelude),
        CreatePreludeCompletionStatus::InProgress
    );

    handle_prelude_modal(&mut prelude, key(KeyCode::Char('m')));
    steps.push(observed_step(&prelude));
    // SamePath fast path: `mount-dst-edit` marked skipped, wizard on
    // `workdir`.
    assert_eq!(
        prelude.wizard.progress(),
        progress(
            CREATE_PRELUDE_STEP_WORKDIR,
            &["mount-src", "mount-dst-choice"],
            &["mount-dst-edit"],
        )
    );
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    steps.push(observed_step(&prelude));
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    steps.push(observed_step(&prelude));

    assert_eq!(
        steps,
        [
            Some(CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE),
            Some(CREATE_PRELUDE_STEP_WORKDIR),
            Some(CREATE_PRELUDE_STEP_NAME),
            None,
        ]
    );
    assert_eq!(prelude.pending_mount_dst.as_deref(), Some(GOLDEN_SRC));
    assert!(!prelude.pending_readonly);
    assert!(!prelude.used_edit_dst);
    assert_eq!(prelude.pending_workdir.as_deref(), Some(GOLDEN_SRC));
    assert_eq!(prelude.pending_name.as_deref(), Some("project"));
    assert_eq!(
        prelude.wizard.progress(),
        progress(
            CREATE_PRELUDE_STEP_NAME,
            &["mount-src", "mount-dst-choice", "workdir", "name"],
            &["mount-dst-edit"],
        )
    );
    assert_eq!(
        observed_completion(&prelude),
        CreatePreludeCompletionStatus::Complete
    );
}

#[test]
fn golden_walk_2_edit_dst_forward_completes() {
    // FileBrowser commit (seeded) → Edit → TextInputDst commit →
    // WorkdirPick commit → TextInputName commit ⇒ Complete.
    let mut prelude = prelude_with_browser_committed(GOLDEN_SRC);
    let mut steps = vec![observed_step(&prelude)];

    handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));
    steps.push(observed_step(&prelude));
    assert_eq!(
        observed_completion(&prelude),
        CreatePreludeCompletionStatus::InProgress
    );
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    steps.push(observed_step(&prelude));
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    steps.push(observed_step(&prelude));
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    steps.push(observed_step(&prelude));

    assert_eq!(
        steps,
        [
            Some(CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE),
            Some(CREATE_PRELUDE_STEP_MOUNT_DST_EDIT),
            Some(CREATE_PRELUDE_STEP_WORKDIR),
            Some(CREATE_PRELUDE_STEP_NAME),
            None,
        ]
    );
    assert_eq!(prelude.pending_mount_dst.as_deref(), Some(GOLDEN_SRC));
    assert!(!prelude.pending_readonly);
    assert!(prelude.used_edit_dst);
    assert_eq!(prelude.pending_workdir.as_deref(), Some(GOLDEN_SRC));
    assert_eq!(prelude.pending_name.as_deref(), Some("project"));
    assert_eq!(
        prelude.wizard.progress(),
        progress(
            CREATE_PRELUDE_STEP_NAME,
            &[
                "mount-src",
                "mount-dst-choice",
                "mount-dst-edit",
                "workdir",
                "name",
            ],
            &[],
        )
    );
    assert_eq!(
        observed_completion(&prelude),
        CreatePreludeCompletionStatus::Complete
    );
}

#[test]
fn golden_walk_3_same_path_full_rewind_cancels() {
    // Walk 1 to TextInputName, then Esc at each step:
    // TextInputName → WorkdirPick → MountDstChoice → FileBrowser (at
    // last cwd) → Esc ⇒ Cancelled.
    let mut prelude = prelude_with_browser_committed(GOLDEN_SRC);
    handle_prelude_modal(&mut prelude, key(KeyCode::Char('m')));
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    assert_eq!(observed_step(&prelude), Some(CREATE_PRELUDE_STEP_NAME));

    let mut steps = Vec::new();
    handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
    steps.push(observed_step(&prelude));
    handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
    steps.push(observed_step(&prelude));
    handle_prelude_modal_with_effects(&mut prelude, key(KeyCode::Esc));
    steps.push(observed_step(&prelude));
    handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
    steps.push(observed_step(&prelude));

    assert_eq!(
        steps,
        [
            Some(CREATE_PRELUDE_STEP_WORKDIR),
            Some(CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE),
            Some(CREATE_PRELUDE_STEP_MOUNT_SRC),
            None,
        ]
    );
    assert!(
        prelude.pending_name.is_none(),
        "Esc rewinds never commit a name"
    );
    assert_eq!(
        prelude.wizard.progress(),
        progress(
            CREATE_PRELUDE_STEP_MOUNT_SRC,
            &["mount-src", "mount-dst-choice", "workdir"],
            &["mount-dst-edit"],
        )
    );
    assert_eq!(
        observed_completion(&prelude),
        CreatePreludeCompletionStatus::Cancelled
    );
}
