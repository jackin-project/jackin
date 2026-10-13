// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn golden_walk_4_edit_dst_full_rewind_cancels() {
    // Walk 2 to TextInputName, then Esc at each step:
    // TextInputName → WorkdirPick → TextInputDst (used_edit_dst rule) →
    // MountDstChoice → FileBrowser → Esc ⇒ Cancelled.
    let mut prelude = prelude_with_browser_committed(GOLDEN_SRC);
    handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    assert_eq!(observed_step(&prelude), Some(CREATE_PRELUDE_STEP_NAME));

    let mut steps = Vec::new();
    handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
    steps.push(observed_step(&prelude));
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
            Some(CREATE_PRELUDE_STEP_MOUNT_DST_EDIT),
            Some(CREATE_PRELUDE_STEP_MOUNT_DST_CHOICE),
            Some(CREATE_PRELUDE_STEP_MOUNT_SRC),
            None,
        ]
    );
    assert!(prelude.pending_name.is_none());
    assert_eq!(
        prelude.wizard.progress(),
        progress(
            CREATE_PRELUDE_STEP_MOUNT_SRC,
            &["mount-src", "mount-dst-choice", "mount-dst-edit", "workdir"],
            &[],
        )
    );
    assert_eq!(
        observed_completion(&prelude),
        CreatePreludeCompletionStatus::Cancelled
    );
}

#[test]
fn golden_walk_5a_direction_change_edit_branch_matches_uninterrupted() {
    // Forward 2 (Edit → TextInputDst commit), back 1 (Esc →
    // TextInputDst), forward again: the final pending fields are
    // identical to walk 2's.
    let mut prelude = prelude_with_browser_committed(GOLDEN_SRC);
    let mut steps = vec![observed_step(&prelude)];

    handle_prelude_modal(&mut prelude, key(KeyCode::Char('e')));
    steps.push(observed_step(&prelude));
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    steps.push(observed_step(&prelude));
    // Back 1: used_edit_dst rewind lands on TextInputDst.
    handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
    steps.push(observed_step(&prelude));
    // Forward again to completion.
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
fn golden_walk_5b_direction_change_same_path_branch_matches_uninterrupted() {
    // Forward 2 (SamePath → WorkdirPick commit), back 1 (Esc →
    // WorkdirPick), forward again: identical to walk 1.
    let mut prelude = prelude_with_browser_committed(GOLDEN_SRC);
    let mut steps = vec![observed_step(&prelude)];

    handle_prelude_modal(&mut prelude, key(KeyCode::Char('m')));
    steps.push(observed_step(&prelude));
    handle_prelude_modal(&mut prelude, key(KeyCode::Enter));
    steps.push(observed_step(&prelude));
    // Back 1: Esc at TextInputName reopens WorkdirPick.
    handle_prelude_modal(&mut prelude, key(KeyCode::Esc));
    steps.push(observed_step(&prelude));
    // Forward again to completion: WorkdirPick commit →
    // TextInputName commit.
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
fn golden_walk_6_esc_at_file_browser_src_cancels() {
    // Esc at step 1 (FileBrowserSrc) cancels the whole prelude.
    let mut prelude = file_browser_src_prelude();
    assert_eq!(observed_step(&prelude), Some(CREATE_PRELUDE_STEP_MOUNT_SRC));

    handle_prelude_modal(&mut prelude, key(KeyCode::Esc));

    assert_eq!(observed_step(&prelude), None);
    assert!(prelude.pending_mount_src.is_none());
    assert_eq!(
        prelude.wizard.progress(),
        progress(CREATE_PRELUDE_STEP_MOUNT_SRC, &[], &[])
    );
    assert_eq!(
        observed_completion(&prelude),
        CreatePreludeCompletionStatus::Cancelled
    );
}
