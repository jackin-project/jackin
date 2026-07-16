// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Dialog constructor methods (`new_*`) extracted from the main impl block
//! in the coordinator. Each remains an inherent `impl Dialog` method so
//! `Dialog::new_foo(...)` call sites continue to resolve unchanged.

use std::sync::Arc;

use termrock::components::TextField;

use super::input::{first_selectable_idx, picker_filtered_rows};
use super::{
    ConfirmKind, Dialog, InspectRow, MAX_CUSTOM_LABEL_LEN, PaletteCloseLabel, PickerIntent,
    ProviderChoice,
};

impl Dialog {
    pub fn new_command_palette(close_label: PaletteCloseLabel) -> Self {
        Self::CommandPalette {
            selected: 0,
            filter: String::new(),
            close_label,
        }
    }

    pub fn new_rename_tab(tab_idx: usize, initial: impl Into<String>) -> Self {
        let input = TextField::new(initial.into()).with_max_chars(MAX_CUSTOM_LABEL_LEN);
        Self::RenameTab { tab_idx, input }
    }

    pub fn new_export_file() -> Self {
        Self::new_export_file_with_post_action(false, false)
    }

    pub fn new_export_file_and_reveal() -> Self {
        Self::new_export_file_with_post_action(true, false)
    }

    pub fn new_export_file_and_open() -> Self {
        Self::new_export_file_with_post_action(false, true)
    }

    fn new_export_file_with_post_action(
        reveal_after_export: bool,
        open_after_export: bool,
    ) -> Self {
        Self::ExportFile {
            input: TextField::new("").with_max_chars(4096),
            reveal_after_export,
            open_after_export,
        }
    }

    pub fn new_split_direction_picker() -> Self {
        Self::SplitDirectionPicker {
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn new_close_target_picker() -> Self {
        Self::CloseTargetPicker {
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn new_confirm_action(kind: ConfirmKind) -> Self {
        Self::ConfirmAction {
            kind,
            selected_yes: false,
        }
    }

    pub fn new_provider_picker(
        agent: Option<String>,
        providers: Vec<ProviderChoice>,
        intent: PickerIntent,
    ) -> Self {
        Self::ProviderPicker {
            agent,
            providers,
            selected: 0,
            intent,
        }
    }

    /// Construct an `AgentPicker` with `selected` pre-initialised to
    /// the first selectable row of the unfiltered layout. Saves every
    /// caller from having to know about the leading "agents" section
    /// row that pushes the first selectable index off zero — and
    /// keeps the "no agents installed" case working (the layout
    /// degenerates to `[Section("shells"), Shell]`, first selectable
    /// is still `1`).
    pub fn new_agent_picker(agents: Vec<String>, intent: PickerIntent) -> Self {
        let filter = String::new();
        let visible = picker_filtered_rows(&agents, &filter);
        Self::AgentPicker {
            agents,
            selected: first_selectable_idx(&visible),
            intent,
            filter,
        }
    }

    /// Build the in-capsule dirty-exit modal from per-repo summary lines and
    /// the pre-built inspect rows (shared with the Inspect sub-dialog).
    #[must_use]
    pub fn new_exit_dirty(summary: Vec<String>, inspect_rows: Arc<[InspectRow]>) -> Self {
        Self::ExitDirty {
            summary,
            selected: 0,
            inspect_rows,
        }
    }

    /// Build the read-only Inspect list opened from the dirty-exit modal.
    #[must_use]
    pub fn new_exit_inspect(lines: Arc<[InspectRow]>) -> Self {
        Self::ExitInspect { lines, selected: 0 }
    }
}
