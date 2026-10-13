// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn prelude_with_browser_committed(
    src: &str,
) -> crate::tui::state::CreatePreludeState<'static> {
    let mut prelude = crate::tui::state::CreatePreludeState::new();
    prelude.accept_mount_src(std::path::PathBuf::from(src));
    prelude.wizard.next();
    prelude.modal = Some(Modal::MountDstChoice {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: create_prelude_mount_dst_choice_state(src),
    });
    prelude
}

pub(super) fn handle_prelude_modal_with_effects(
    prelude: &mut crate::tui::state::CreatePreludeState<'_>,
    key: crossterm::event::KeyEvent,
) {
    let outcome = handle_prelude_modal(prelude, key);
    if !matches!(outcome, PreludeModalOutcome::ReopenFileBrowserAtLastCwd) {
        return;
    }

    let Ok(mut file_browser) = crate::services::file_browser::state_from_home() else {
        prelude.modal = None;
        return;
    };
    if let Some(cwd) = prelude.last_browser_cwd.as_ref() {
        crate::services::file_browser::clamp_state_to_cwd(&mut file_browser, cwd);
    }
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: file_browser,
    });
}

pub(super) fn handle_prelude_modal(
    prelude: &mut crate::tui::state::CreatePreludeState<'_>,
    key: crossterm::event::KeyEvent,
) -> PreludeModalOutcome {
    raw_handle_prelude_modal(prelude, key, Rect::new(0, 0, 120, 40))
}

pub(super) const GOLDEN_SRC: &str = "/home/user/project";

pub(super) fn observed_step(prelude: &crate::tui::state::CreatePreludeState<'_>) -> Option<usize> {
    prelude.modal.as_ref()?;
    Some(prelude.wizard.step())
}

pub(super) fn observed_completion(
    prelude: &crate::tui::state::CreatePreludeState<'_>,
) -> CreatePreludeCompletionStatus {
    create_prelude_completion_status(prelude.modal.is_some(), prelude.completed().is_some())
}

pub(super) fn progress(step_index: usize, completed: &[&str], skipped: &[&str]) -> WizardProgress {
    WizardProgress {
        step_index,
        phase: WizardPhase::Step,
        completed: completed.iter().map(|s| (*s).to_owned()).collect(),
        skipped: skipped.iter().map(|s| (*s).to_owned()).collect(),
        failure_message: None,
    }
}

pub(super) fn file_browser_src_prelude() -> crate::tui::state::CreatePreludeState<'static> {
    let mut prelude = crate::tui::state::CreatePreludeState::new();
    let fb = crate::tui::components::file_browser::FileBrowserState::from_listing(
        crate::services::file_browser::listing_from_home()
            .expect("file browser should build in test env"),
    );
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: fb,
    });
    prelude
}
