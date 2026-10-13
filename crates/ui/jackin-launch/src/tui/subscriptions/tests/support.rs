// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) struct RecordingTerminal {
    copied: Mutex<Vec<String>>,
    compact: Mutex<Vec<(String, String)>>,
    debug: Mutex<Vec<(String, String)>>,
}

impl RecordingTerminal {
    pub(super) const fn new() -> Self {
        Self {
            copied: Mutex::new(Vec::new()),
            compact: Mutex::new(Vec::new()),
            debug: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn copied(&self) -> Vec<String> {
        self.copied.lock().expect("test clipboard lock").clone()
    }

    pub(super) fn compact(&self) -> Vec<(String, String)> {
        self.compact.lock().expect("test compact lock").clone()
    }

    pub(super) fn debug(&self) -> Vec<(String, String)> {
        self.debug.lock().expect("test debug lock").clone()
    }
}

impl LaunchHostTerminal for RecordingTerminal {
    fn set_rich_surface_active(&self, _active: bool) {}
    fn host_screen_owned(&self) -> bool {
        false
    }
    fn is_debug_mode(&self) -> bool {
        true
    }
    fn emit_compact_line(&self, kind: &str, line: &str) {
        self.compact
            .lock()
            .expect("test compact lock")
            .push((kind.to_owned(), line.to_owned()));
    }
    fn emit_debug_line(&self, category: &str, line: &str) {
        self.debug
            .lock()
            .expect("test debug lock")
            .push((category.to_owned(), line.to_owned()));
    }
    fn set_pointer_shape(&self, _pointer: bool) {}
    fn copy_to_clipboard(&self, payload: &str) -> bool {
        self.copied
            .lock()
            .expect("test clipboard lock")
            .push(payload.to_owned());
        true
    }
    fn reveal_file(&self, _path: &std::path::Path) -> bool {
        false
    }
    fn open_file(&self, _path: &std::path::Path) -> bool {
        false
    }
}

pub(super) fn hit_point_for_payload(
    area: Rect,
    state: &crate::tui::components::container_info::ContainerInfoState,
    payload: &str,
) -> (u16, u16) {
    let rect = launch_container_info_rect(area, state, true);
    for row in rect.y..rect.y.saturating_add(rect.height) {
        for col in rect.x..rect.x.saturating_add(rect.width) {
            if crate::tui::components::container_info::copy_payload_at(rect, state, col, row)
                .is_some_and(|(_, candidate)| candidate == payload)
            {
                return (col, row);
            }
        }
    }
    panic!("copy target for {payload:?} not found");
}

pub(super) fn failure_failure() -> LaunchFailure {
    LaunchFailure {
        title: "Build failed".to_owned(),
        summary: "docker build failed".to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::DerivedImage,
    }
}

pub(super) fn quit_confirm_view() -> crate::LaunchView {
    let mut view = crate::initial_view();
    view.quit_confirm =
        Some(crate::tui::components::prompts::PromptConfirm::new("Exit jackin❯?").with_focus_yes());
    view
}
