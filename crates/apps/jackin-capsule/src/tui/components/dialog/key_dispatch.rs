// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Top-level dialog key dispatch across all modal variants.

use super::github_context::GithubContextView;
use super::input::{exec_picker_handle_key, export_file_handle_key, rename_tab_handle_key};
use super::{Dialog, DialogAction};

use crate::tui::keymap::READ_ONLY_DISMISS_KEYMAP;
use crate::tui::keymap::raw_bytes_to_chord;

impl Dialog {
    /// Mutable body-scroll state for the read-only info dialogs whose content
    /// can overflow (`ContainerInfo`, `GitHubContext`). `None` for dialogs that do
    /// not scroll. Lets the daemon route mouse-wheel events to the dialog body.
    /// Handle a raw key byte and return the resulting action.
    #[expect(
        clippy::too_many_lines,
        reason = "Dialog key-event dispatcher with one arm per key binding. \
                  Each arm carries its focused state transition; extracting \
                  arms into sub-dispatchers would obscure per-binding readability."
    )]
    #[expect(
        clippy::excessive_nesting,
        reason = "Dialog key-event dispatcher: per-key + per-Dialog-variant nested \
                  with state-update branches. Modal nesting is the dispatch protocol."
    )]
    pub fn handle_key(
        &mut self,
        key: &[u8],
        github: Option<&GithubContextView<'_>>,
    ) -> DialogAction {
        // Text-input dialog has its own dismissal / editing rules and
        // must intercept keys before the arrow-key + dismiss-key
        // shortcuts below would steal them (e.g. `q` is a legal
        // character inside a custom tab name).
        if let Self::RenameTab { tab_idx, input } = self {
            return rename_tab_handle_key(*tab_idx, input, key);
        }
        // The exec credential picker is multi-select (Space toggles), so it
        // intercepts keys before the shared single-select arrow/dismiss logic.
        if let Self::ExecPicker(state) = self {
            return exec_picker_handle_key(state, key);
        }
        if let Self::ExportFile {
            input,
            reveal_after_export,
            open_after_export,
        } = self
        {
            return export_file_handle_key(input, *reveal_after_export, *open_after_export, key);
        }
        if let Self::SpawnFailure(state) = self {
            let _ = state;
            return if matches!(key, b"\r" | b"\n" | b"\x1b" | b"\x03" | b"\x11") {
                DialogAction::Dismiss
            } else {
                DialogAction::Redraw
            };
        }
        // Read-only info dialogs (ContainerInfo, GitHubContext): Esc /
        // dismiss keys close, Enter copies the dialog's value to the
        // operator's clipboard with the `copied` flag flipped to true
        // so the next render's check affordance confirms the OSC 52
        // fired. The dialog stays open until dismissed so the feedback
        // is actually visible.
        if matches!(self, Self::Usage { .. }) {
            if matches!(key, b"r" | b"R") {
                return DialogAction::RefreshUsage;
            }
            if matches!(key, b"\t" | b"\x1b[Z") {
                if let Self::Usage {
                    tab_bar_focused, ..
                } = self
                {
                    *tab_bar_focused = !*tab_bar_focused;
                }
                return DialogAction::Redraw;
            }
            let tab_bar_focused = matches!(
                self,
                Self::Usage {
                    tab_bar_focused: true,
                    ..
                }
            );
            if tab_bar_focused {
                if raw_bytes_to_chord(key)
                    .and_then(|chord| READ_ONLY_DISMISS_KEYMAP.dispatch(chord))
                    .is_some()
                {
                    return DialogAction::Dismiss;
                }
                if let Some(tab) = match key {
                    b"\x1b[C" => self.usage_provider_tab_target(1),
                    b"\x1b[D" => self.usage_provider_tab_target(-1),
                    _ => None,
                } {
                    return DialogAction::SwitchUsageProvider {
                        provider_label: tab.label,
                        account_id: tab.id,
                    };
                }
                return DialogAction::Redraw;
            }
            if raw_bytes_to_chord(key)
                .and_then(|chord| READ_ONLY_DISMISS_KEYMAP.dispatch(chord))
                .is_some()
            {
                if let Self::Usage {
                    tab_bar_focused, ..
                } = self
                {
                    *tab_bar_focused = true;
                }
                return DialogAction::Redraw;
            }
            if let Self::Usage { scroll, .. } = self
                && crate::tui::scroll_input::apply_raw_dialog_scroll_key(
                    scroll,
                    key,
                    termrock::scroll::ScrollAxes {
                        vertical: true,
                        horizontal: true,
                    },
                )
            {
                return DialogAction::Redraw;
            }
            return DialogAction::Redraw;
        }
        if matches!(
            self,
            Self::ContainerInfo { .. } | Self::GitHubContext { .. }
        ) {
            if raw_bytes_to_chord(key)
                .and_then(|chord| READ_ONLY_DISMISS_KEYMAP.dispatch(chord))
                .is_some()
            {
                return DialogAction::Dismiss;
            }
            // Scroll the read-only body (offsets clamp at render time): Up/Down +
            // k/j vertical, Left/Right + h/l horizontal. The shared state is
            // rebuilt each frame, so the offset lives on the dialog enum.
            let body_scroll = match self {
                Self::ContainerInfo { scroll, .. } | Self::GitHubContext { scroll, .. } => {
                    Some(scroll)
                }
                _ => None,
            };
            if let Some(scroll) = body_scroll
                && crate::tui::scroll_input::apply_raw_dialog_scroll_key(
                    scroll,
                    key,
                    termrock::scroll::ScrollAxes {
                        vertical: true,
                        horizontal: true,
                    },
                )
            {
                return DialogAction::Redraw;
            }
            return match key {
                b"\r" | b"\n" => {
                    // ContainerInfo: Enter copies the shared default copy
                    // target. Mouse clicks copy whichever row was clicked.
                    if let Some((row, payload)) = self
                        .container_info_state()
                        .and_then(|state| state.keyboard_copy_payload())
                    {
                        if let Self::ContainerInfo { copied_row, .. } = self {
                            *copied_row = Some(row);
                        }
                        return DialogAction::CopyToClipboard(payload);
                    }
                    if let Some((_, payload)) = self
                        .github_context_state(github)
                        .and_then(|state| state.keyboard_copy_payload())
                    {
                        if let Self::GitHubContext { copied, .. } = self {
                            *copied = true;
                        }
                        DialogAction::CopyToClipboard(payload)
                    } else {
                        DialogAction::Redraw
                    }
                }
                b"o" | b"O" => match self {
                    Self::GitHubContext { .. } => github
                        .and_then(|view| view.status.loaded())
                        .map_or(DialogAction::Redraw, |pr| {
                            DialogAction::OpenHostUrl(pr.url.clone())
                        }),
                    _ => DialogAction::Redraw,
                },
                b"c" | b"C" => {
                    if !matches!(self, Self::GitHubContext { .. }) {
                        return DialogAction::Redraw;
                    }
                    github
                        .and_then(|view| view.status.loaded())
                        .and_then(|pr| pr.checks.as_ref())
                        .and_then(crate::pull_request::PullRequestChecks::ci_url)
                        .map_or(DialogAction::Redraw, |url| {
                            DialogAction::OpenHostUrl(url.to_owned())
                        })
                }
                b"r" | b"R" => DialogAction::Redraw,
                _ => DialogAction::Redraw,
            };
        }
        if let Self::ConfirmAction { kind, selected_yes } = self {
            return match key {
                b"y" | b"Y" => DialogAction::ConfirmedAction(*kind),
                b"n" | b"N" | b"\x1b" | b"\x03" | b"\x11" => DialogAction::Dismiss,
                b"\t" | b"\x1b[Z" | b"\x1b[D" | b"\x1b[C" | b"h" | b"l" => {
                    *selected_yes = !*selected_yes;
                    DialogAction::Redraw
                }
                b"\r" | b"\n" => {
                    if *selected_yes {
                        DialogAction::ConfirmedAction(*kind)
                    } else {
                        DialogAction::Dismiss
                    }
                }
                _ => DialogAction::Redraw,
            };
        }
        // Coalesced typing (scripted input, paste, batched PTY reads)
        // arrives as one multi-byte `Data` chunk — the input parser
        // coalesces contiguous plain bytes. Dispatch an ESC-free chunk
        // byte-by-byte in order so filter-then-confirm (`b"split\r"`)
        // works as one write; the first substantive action wins and
        // stops the scan. Chunks holding ESC keep the legacy
        // whole-chunk dispatch so escape sequences stay atomic.
        if key.len() > 1 && !key.contains(&0x1B) {
            let mut result = DialogAction::Redraw;
            for byte in key {
                let action = self.handle_filter_list_key(&[*byte]);
                if !matches!(action, DialogAction::Redraw) {
                    result = action;
                    break;
                }
            }
            return result;
        }
        self.handle_filter_list_key(key)
    }
}
