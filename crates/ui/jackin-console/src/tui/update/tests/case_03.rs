// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn source_picker_plan_routes_source_outcomes() {
    use crate::tui::components::source_picker::SourceChoice;

    assert_eq!(
        source_picker_plan(jackin_oppicker::ModalOutcome::Commit(SourceChoice::Plain)),
        SourcePickerPlan::Plain
    );
    assert_eq!(
        source_picker_plan(jackin_oppicker::ModalOutcome::Commit(SourceChoice::Op)),
        SourcePickerPlan::Op
    );
    assert_eq!(
        source_picker_plan(jackin_oppicker::ModalOutcome::Cancel),
        SourcePickerPlan::Dismiss
    );
    assert_eq!(
        source_picker_plan(jackin_oppicker::ModalOutcome::Continue),
        SourcePickerPlan::Continue
    );
}

#[test]
fn list_github_picker_plan_routes_picker_outcomes() {
    assert_eq!(
        list_github_picker_plan(jackin_oppicker::ModalOutcome::Commit(
            "https://github.com/jackin-project/jackin".to_owned()
        )),
        ListGithubPickerPlan::OpenUrl("https://github.com/jackin-project/jackin".to_owned())
    );
    assert_eq!(
        list_github_picker_plan(jackin_oppicker::ModalOutcome::Cancel),
        ListGithubPickerPlan::Dismiss
    );
    assert_eq!(
        list_github_picker_plan(jackin_oppicker::ModalOutcome::Continue),
        ListGithubPickerPlan::Continue
    );
}

#[test]
fn list_role_picker_plan_routes_picker_outcomes() {
    assert_eq!(
        list_role_picker_plan(jackin_oppicker::ModalOutcome::Commit("agent-smith")),
        ListRolePickerPlan::Launch("agent-smith")
    );
    assert_eq!(
        list_role_picker_plan::<&str>(jackin_oppicker::ModalOutcome::Cancel),
        ListRolePickerPlan::Dismiss
    );
    assert_eq!(
        list_role_picker_plan::<&str>(jackin_oppicker::ModalOutcome::Continue),
        ListRolePickerPlan::Continue
    );
}

#[test]
fn dismissible_modal_plan_dismisses_commit_and_cancel() {
    assert_eq!(
        dismissible_modal_plan(jackin_oppicker::ModalOutcome::Commit(())),
        DismissibleModalPlan::Dismiss
    );
    assert_eq!(
        dismissible_modal_plan::<()>(jackin_oppicker::ModalOutcome::Cancel),
        DismissibleModalPlan::Dismiss
    );
    assert_eq!(
        dismissible_modal_plan::<()>(jackin_oppicker::ModalOutcome::Continue),
        DismissibleModalPlan::Continue
    );
}
