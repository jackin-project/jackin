// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn create_prelude_file_browser_plan_routes_browser_outcomes() {
    use crate::tui::components::file_browser::FileBrowserOutcome;

    let path = PathBuf::from("/tmp/workspace");
    assert_eq!(
        create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::Cancel),
        CreatePreludeFileBrowserPlan::CancelPrelude
    );
    assert_eq!(
        create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::ResolveGitUrl(
            path.clone()
        )),
        CreatePreludeFileBrowserPlan::ResolveGitUrl(path.clone())
    );
    assert_eq!(
        create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::OpenGitUrl(
            "file:///tmp/workspace".to_owned()
        )),
        CreatePreludeFileBrowserPlan::OpenUrl("file:///tmp/workspace".to_owned())
    );
    assert_eq!(
        create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::Continue),
        CreatePreludeFileBrowserPlan::Continue
    );
    assert_eq!(
        create_prelude_file_browser_plan(FileBrowserOutcome::<PathBuf>::NavigateTo(path.clone())),
        CreatePreludeFileBrowserPlan::ApplyFileBrowserOutcome(FileBrowserOutcome::NavigateTo(path))
    );
}

#[test]
fn create_prelude_mount_dst_choice_plan_routes_choice_outcomes() {
    use crate::tui::components::mount_dst_choice::MountDstChoice;

    assert_eq!(
        create_prelude_mount_dst_choice_plan(jackin_oppicker::ModalOutcome::Commit(
            MountDstChoice::SamePath
        )),
        CreatePreludeMountDstChoicePlan::CommitSamePath
    );
    assert_eq!(
        create_prelude_mount_dst_choice_plan(jackin_oppicker::ModalOutcome::Commit(
            MountDstChoice::Edit
        )),
        CreatePreludeMountDstChoicePlan::OpenEditInput
    );
    assert_eq!(
        create_prelude_mount_dst_choice_plan(jackin_oppicker::ModalOutcome::Cancel),
        CreatePreludeMountDstChoicePlan::ReopenFileBrowserAtLastCwd
    );
    assert_eq!(
        create_prelude_mount_dst_choice_plan(jackin_oppicker::ModalOutcome::Continue),
        CreatePreludeMountDstChoicePlan::Continue
    );
}

#[test]
fn create_prelude_text_input_dst_plan_routes_input_outcomes() {
    assert_eq!(
        create_prelude_text_input_dst_plan(jackin_oppicker::ModalOutcome::Commit(
            "/workspace".to_owned()
        )),
        CreatePreludeTextInputDstPlan::Commit("/workspace".to_owned())
    );
    assert_eq!(
        create_prelude_text_input_dst_plan::<String>(jackin_oppicker::ModalOutcome::Cancel),
        CreatePreludeTextInputDstPlan::ReopenMountDstChoice
    );
    assert_eq!(
        create_prelude_text_input_dst_plan::<String>(jackin_oppicker::ModalOutcome::Continue),
        CreatePreludeTextInputDstPlan::Continue
    );
}

#[test]
fn create_prelude_text_input_name_plan_routes_input_outcomes() {
    assert_eq!(
        create_prelude_text_input_name_plan(jackin_oppicker::ModalOutcome::Commit(
            "workspace".to_owned()
        )),
        CreatePreludeTextInputNamePlan::Commit("workspace".to_owned())
    );
    assert_eq!(
        create_prelude_text_input_name_plan::<String>(jackin_oppicker::ModalOutcome::Cancel),
        CreatePreludeTextInputNamePlan::ReopenWorkdirPick
    );
    assert_eq!(
        create_prelude_text_input_name_plan::<String>(jackin_oppicker::ModalOutcome::Continue),
        CreatePreludeTextInputNamePlan::Continue
    );
}

#[test]
fn create_prelude_workdir_pick_plan_routes_input_outcomes() {
    assert_eq!(
        create_prelude_workdir_pick_plan(
            jackin_oppicker::ModalOutcome::Commit("src".to_owned()),
            true
        ),
        CreatePreludeWorkdirPickPlan::Commit("src".to_owned())
    );
    assert_eq!(
        create_prelude_workdir_pick_plan::<String>(jackin_oppicker::ModalOutcome::Cancel, true),
        CreatePreludeWorkdirPickPlan::ReopenTextInputDst
    );
    assert_eq!(
        create_prelude_workdir_pick_plan::<String>(jackin_oppicker::ModalOutcome::Cancel, false),
        CreatePreludeWorkdirPickPlan::ReopenMountDstChoice
    );
    assert_eq!(
        create_prelude_workdir_pick_plan::<String>(jackin_oppicker::ModalOutcome::Continue, true),
        CreatePreludeWorkdirPickPlan::Continue
    );
}

#[test]
fn console_modal_letter_input_kind_maps_text_filters_and_other_modals() {
    assert_eq!(
        RectTestModal::TextInput {
            target: (),
            state: (),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::TextInput)
    );
    assert_eq!(
        RectTestModal::RolePicker {
            state: TestRolePicker(2),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::FilterPicker)
    );
    assert_eq!(
        RectTestModal::RoleOverridePicker {
            state: TestRolePicker(2),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::FilterPicker)
    );
    assert_eq!(
        RectTestModal::OpPicker {
            secrets_target: None,
            state: Box::new(TestOpPicker(false)),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::FilterPicker)
    );
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::Other)
    );
}

#[test]
fn console_modal_list_key_target_maps_list_modal_key_handlers() {
    assert_eq!(
        RectTestModal::GithubPicker {
            state: TestGithubPicker(2)
        }
        .list_key_target(),
        crate::tui::update::ListModalKeyTarget::GithubPicker
    );
    assert_eq!(
        RectTestModal::RolePicker {
            state: TestRolePicker(2)
        }
        .list_key_target(),
        crate::tui::update::ListModalKeyTarget::RolePicker
    );
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.list_key_target(),
        crate::tui::update::ListModalKeyTarget::ErrorPopup
    );
    assert_eq!(
        RectTestModal::ContainerInfo {
            state: TestContainerInfo
        }
        .list_key_target(),
        crate::tui::update::ListModalKeyTarget::ContainerInfo
    );
    assert_eq!(
        RectTestModal::StatusPopup { state: () }.list_key_target(),
        crate::tui::update::ListModalKeyTarget::Dismiss
    );
}

#[test]
fn console_modal_list_scroll_target_maps_scrollable_list_modals() {
    assert_eq!(
        RectTestModal::GithubPicker {
            state: TestGithubPicker(2)
        }
        .list_scroll_target(),
        crate::tui::update::ListModalScrollTarget::GithubPicker
    );
    assert_eq!(
        RectTestModal::RolePicker {
            state: TestRolePicker(2)
        }
        .list_scroll_target(),
        crate::tui::update::ListModalScrollTarget::RolePicker
    );
    assert_eq!(
        RectTestModal::OpPicker {
            secrets_target: None,
            state: Box::new(TestOpPicker(false))
        }
        .list_scroll_target(),
        crate::tui::update::ListModalScrollTarget::OpPicker
    );
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.list_scroll_target(),
        crate::tui::update::ListModalScrollTarget::None
    );
}

#[test]
fn console_modal_shared_scroll_target_maps_reused_picker_modals() {
    assert_eq!(
        RectTestModal::WorkdirPick { state: () }.shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::WorkdirPick
    );
    assert_eq!(
        RectTestModal::RoleOverridePicker {
            state: TestRolePicker(2)
        }
        .shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::RolePicker
    );
    assert_eq!(
        RectTestModal::RoleOverridePicker {
            state: TestRolePicker(2)
        }
        .shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::RolePicker
    );
    assert_eq!(
        RectTestModal::OpPicker {
            secrets_target: None,
            state: Box::new(TestOpPicker(false))
        }
        .shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::OpPicker
    );
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::None
    );
}

#[test]
fn console_modal_ticks_op_picker_animation_only() {
    let mut op_picker = RectTestModal::OpPicker {
        secrets_target: None,
        state: Box::new(TestOpPicker(true)),
    };
    assert!(op_picker.tick_active_animation());

    let mut idle_op_picker = RectTestModal::OpPicker {
        secrets_target: None,
        state: Box::new(TestOpPicker(false)),
    };
    assert!(!idle_op_picker.tick_active_animation());

    let mut error = RectTestModal::ErrorPopup { state: TestError };
    assert!(!error.tick_active_animation());
}

#[test]
fn console_manager_stage_ticks_editor_and_settings_only() {
    type Stage = ConsoleManagerStage<(), TestAnimationTick, TestAnimationTick>;

    let mut editor = Stage::Editor(TestAnimationTick(true));
    assert!(editor.tick_active_animation());

    let mut settings = Stage::Settings(TestAnimationTick(true));
    assert!(settings.tick_active_animation());

    let mut idle_editor = Stage::Editor(TestAnimationTick(false));
    assert!(!idle_editor.tick_active_animation());

    let mut list = Stage::List;
    assert!(!list.tick_active_animation());

    let mut create = Stage::CreatePrelude(());
    assert!(!create.tick_active_animation());

    let mut delete = Stage::ConfirmDelete {
        name: "workspace".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete?"),
    };
    assert!(!delete.tick_active_animation());
}

#[test]
fn create_prelude_completed_requires_name_and_mount_fields() {
    let mut prelude = ConsoleCreatePreludeState::<()>::new();
    prelude.accept_mount_src(PathBuf::from("/host/proj"));
    prelude.accept_mount_dst("/work/proj".into(), true);
    prelude.accept_workdir("/work/proj".into());

    assert!(prelude.completed().is_none());

    prelude.accept_name("proj".into());
    let (name, workspace) = prelude.completed().expect("complete prelude");

    assert_eq!(name, "proj");
    assert_eq!(workspace.workdir, "/work/proj");
    assert_eq!(workspace.mounts.len(), 1);
    assert_eq!(workspace.mounts[0].src, "/host/proj");
    assert_eq!(workspace.mounts[0].dst, "/work/proj");
    assert!(workspace.mounts[0].readonly);
    assert_eq!(workspace.mounts[0].isolation, MountIsolation::Shared);
}

#[test]
fn create_prelude_builds_pending_first_mount() {
    let mut prelude = ConsoleCreatePreludeState::<()>::new();
    assert!(prelude.pending_first_mount().is_none());

    prelude.accept_mount_src(PathBuf::from("/host/proj"));
    prelude.accept_mount_dst("/work/proj".into(), true);
    let mount = prelude
        .pending_first_mount()
        .expect("src and dst should build mount");

    assert_eq!(mount.src, "/host/proj");
    assert_eq!(mount.dst, "/work/proj");
    assert!(mount.readonly);
    assert_eq!(mount.isolation, MountIsolation::Shared);
}

#[test]
fn create_prelude_opens_workdir_pick_from_pending_mount() {
    let mut prelude = ConsoleCreatePreludeState::<jackin_config::MountConfig>::new();
    assert!(!prelude.open_workdir_pick_from_pending_mount(|mount| mount));
    assert!(prelude.modal.is_none());

    prelude.accept_mount_src(PathBuf::from("/host/proj"));
    prelude.accept_mount_dst("/work/proj".into(), false);

    assert!(prelude.open_workdir_pick_from_pending_mount(|mount| mount));

    let Some(mount) = prelude.modal else {
        panic!("expected workdir pick modal payload");
    };
    assert_eq!(mount.src, "/host/proj");
    assert_eq!(mount.dst, "/work/proj");
    assert!(!mount.readonly);
}

#[test]
fn create_prelude_reopens_mount_dst_choice_from_source() {
    let mut prelude = ConsoleCreatePreludeState::<String>::new();
    prelude.accept_mount_src(PathBuf::from("/host/proj"));

    prelude.reopen_mount_dst_choice(|src| src);

    assert_eq!(prelude.modal.as_deref(), Some("/host/proj"));
}
