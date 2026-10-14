// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn prelude_mount_same_path_chains_to_workdir_pick_with_dst_equal_src() {
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
    let mut prelude = prelude_with_browser_committed("/home/user/project");
    handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));

    match &prelude.modal {
        Some(Modal::TextInput { target, .. }) => {
            assert_eq!(target, &crate::tui::state::TextInputTarget::MountDst);
        }
        other => panic!("expected TextInput(MountDst); got {other:?}"),
    }

    assert!(prelude.pending_mount_dst.is_none());

    assert_eq!(prelude.wizard.step(), CREATE_PRELUDE_STEP_MOUNT_DST_EDIT);
}

#[test]
fn prelude_cancel_on_mount_dst_choice_rewinds_to_file_browser() {
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
    let mut prelude = prelude_with_browser_committed("/home/user/project");

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
    let mut prelude = prelude_with_browser_committed("/home/user/project");
    handle_prelude_modal(&mut prelude, key(KeyCode::Char('m')));
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
    let mut prelude = prelude_with_browser_committed("/home/user/project");
    handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));

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
    let mut prelude = crate::tui::state::CreatePreludeState::new();
    prelude.accept_mount_src(std::path::PathBuf::from("/home/user/project"));
    prelude.accept_mount_dst("/home/user/project".into(), false);
    prelude.accept_workdir("/home/user/project".into());

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
    let mut prelude = prelude_with_browser_committed(GOLDEN_SRC);
    let mut steps = vec![observed_step(&prelude)];
    assert_eq!(
        observed_completion(&prelude),
        CreatePreludeCompletionStatus::InProgress
    );

    handle_prelude_modal(&mut prelude, key(KeyCode::Char('m')));
    steps.push(observed_step(&prelude));

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
