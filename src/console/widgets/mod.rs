//! Reusable widgets for the workspace manager TUI.
//!
//! Two of the widgets wrap ratatui ecosystem crates (`ratatui-textarea`,
//! `tui-widget-list`). The rest are hand-rolled — `FileBrowser` was
//! originally built on `ratatui-explorer` but was rewritten in-house so
//! git-repo rows can carry a distinct trailing suffix (the library
//! exposes only a single shared `dir_style`). All are consumed by both
//! the manager (PR 2) and the Secrets tab (PR 3).

use ratatui::style::Color;

/// Canonical phosphor-green palette used across every TUI surface.
pub(crate) const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
pub(crate) const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
pub(crate) const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
pub(crate) const WHITE: Color = Color::Rgb(255, 255, 255);

pub mod agent_choice;
pub mod auth_panel;
pub mod confirm;
pub mod confirm_save;
pub mod error_popup;
pub mod file_browser;
pub mod github_picker;
pub mod mount_dst_choice;
pub mod op_picker;
pub mod panel_rain;
pub mod role_picker;
pub mod save_discard;
pub mod scope_picker;
pub mod scrollable;
pub mod source_picker;
pub mod text_input;
pub mod workdir_pick;

/// Outcome of a modal's event-handling cycle. Passed back to the
/// manager state machine to decide whether to close the modal, commit
/// its value, or keep it open.
#[derive(Debug, Clone)]
pub enum ModalOutcome<T> {
    /// User is still interacting with the modal — keep rendering.
    Continue,
    /// User committed with this value (e.g. Enter in text input).
    Commit(T),
    /// User cancelled (Esc).
    Cancel,
}

/// Wrap-around cursor move for any list-style picker. `delta` is `-1`
/// for Up, `+1` for Down. No-op when `count == 0`.
pub(crate) fn cycle_select(list_state: &mut tui_widget_list::ListState, count: usize, delta: i32) {
    if count == 0 {
        return;
    }
    let cur = list_state.selected.unwrap_or(0);
    let next = if delta < 0 {
        if cur == 0 { count - 1 } else { cur - 1 }
    } else if cur + 1 >= count {
        0
    } else {
        cur + 1
    };
    list_state.select(Some(next));
}

#[cfg(test)]
mod consistency_tests {
    //! Cross-widget visual-consistency pins.
    //!
    //! Every modal renders with the same chrome: `PHOSPHOR_DARK` border
    //! (RGB 0/80/18), a title wrapped in leading + trailing spaces so
    //! `┌ Title ─...` renders with breathing room, and a hint footer
    //! whose separator glyphs use `PHOSPHOR_DARK`. These tests pin that
    //! contract so a future drift doesn't silently degrade the look.
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

    use super::{PHOSPHOR_DARK, WHITE};

    /// Render a closure into a fresh `TestBackend` and return the resulting
    /// buffer. Size is chosen to comfortably fit every modal under test.
    fn draw<F: FnOnce(&mut ratatui::Frame)>(width: u16, height: u16, render: F) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f)).unwrap();
        term.backend().buffer().clone()
    }

    /// Return the title glyphs rendered on the top border row (y = 0).
    /// The border itself is ` ─ ` glyphs; the title is the contiguous run
    /// of printable non-border characters. Confirms the title has leading
    /// + trailing space padding.
    fn top_border_title(buf: &Buffer) -> String {
        let mut out = String::new();
        let mut in_title = false;
        for x in 0..buf.area.width {
            let sym = buf[(x, 0)].symbol();
            let is_border = matches!(sym, "┌" | "┐" | "─" | "│");
            if is_border {
                if in_title {
                    break;
                }
                continue;
            }
            // First non-border, non-empty cell starts the title.
            if !in_title && !sym.is_empty() {
                in_title = true;
            }
            if in_title {
                out.push_str(sym);
            }
        }
        out
    }

    /// Assert every cell on the top and bottom border rows uses
    /// `PHOSPHOR_DARK` as its foreground colour (title cells are exempt —
    /// they're WHITE+BOLD).
    fn assert_border_is_phosphor_dark(buf: &Buffer, area: Rect, widget: &str) {
        // Top border, skipping the title span.
        for x in area.x..area.x + area.width {
            let cell = &buf[(x, area.y)];
            if cell.symbol().is_empty() {
                continue;
            }
            let is_title_cell = cell.fg == WHITE;
            if is_title_cell {
                continue;
            }
            assert_eq!(
                cell.fg, PHOSPHOR_DARK,
                "{widget}: top-border cell at ({x},{}) fg={:?}, expected PHOSPHOR_DARK",
                area.y, cell.fg,
            );
        }
        // Bottom border — should be all PHOSPHOR_DARK.
        let by = area.y + area.height - 1;
        for x in area.x..area.x + area.width {
            let cell = &buf[(x, by)];
            if cell.symbol().is_empty() {
                continue;
            }
            assert_eq!(
                cell.fg, PHOSPHOR_DARK,
                "{widget}: bottom-border cell at ({x},{by}) fg={:?}, expected PHOSPHOR_DARK",
                cell.fg,
            );
        }
    }

    // Note: the former `assert_hint_row_present` helper and
    // `all_modal_hint_rows_use_canonical_styles` test were removed when hint
    // lines moved out of widget interiors into the main footer. Widgets no
    // longer render an internal hint row; the footer is the single source of
    // truth for available key hints.

    /// Build and render the `SaveDiscardCancel` modal into a full-area
    /// buffer. Returns (buffer, area).
    fn render_save_discard() -> (Buffer, Rect) {
        use super::save_discard::{SaveDiscardState, render};
        let area = Rect::new(0, 0, 70, 7);
        let state = SaveDiscardState::new("Save changes?");
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    fn render_confirm() -> (Buffer, Rect) {
        use super::confirm::{ConfirmState, render};
        let area = Rect::new(0, 0, 60, 7);
        let state = ConfirmState::new("Delete workspace?");
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    fn render_mount_dst() -> (Buffer, Rect) {
        use super::mount_dst_choice::{MountDstChoiceState, render};
        let area = Rect::new(0, 0, 80, 8);
        let state = MountDstChoiceState::new("/home/user/app");
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    fn render_text_input() -> (Buffer, Rect) {
        use super::text_input::{TextInputState, render};
        let area = Rect::new(0, 0, 60, 6);
        let state = TextInputState::new("Name this workspace", "demo");
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    fn render_workdir_pick() -> (Buffer, Rect) {
        use super::workdir_pick::{WorkdirPickState, render};
        use crate::workspace::MountConfig;
        let area = Rect::new(0, 0, 60, 12);
        let mounts = [MountConfig {
            src: "/home/user/app".into(),
            dst: "/home/user/app".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }];
        let state = WorkdirPickState::from_mounts(&mounts);
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    fn render_github_picker() -> (Buffer, Rect) {
        use super::github_picker::{GithubChoice, GithubPickerState, render};
        let area = Rect::new(0, 0, 60, 10);
        let state = GithubPickerState::new(vec![GithubChoice {
            src: "/home/user/app".into(),
            branch: "main".into(),
            url: "https://github.com/example/app/tree/main".into(),
        }]);
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    fn render_role_picker() -> (Buffer, Rect) {
        use super::role_picker::{RolePickerState, render};
        use crate::selector::RoleSelector;
        let area = Rect::new(0, 0, 60, 10);
        let state = RolePickerState::new(vec![
            RoleSelector::parse("chainargos/agent-smith").unwrap(),
            RoleSelector::parse("chainargos/agent-brown").unwrap(),
        ]);
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    fn render_confirm_save() -> (Buffer, Rect) {
        use super::confirm_save::{ConfirmSaveState, render};
        use ratatui::text::Line;
        let area = Rect::new(0, 0, 70, 10);
        let state = ConfirmSaveState::new(vec![
            Line::from("Create workspace: demo"),
            Line::from(""),
            Line::from("Working directory: /home/user/demo"),
        ]);
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    fn render_agent_choice() -> (Buffer, Rect) {
        use super::agent_choice::{AgentChoiceState, render};
        let area = Rect::new(0, 0, 50, 7);
        let state = AgentChoiceState::new();
        let buf = draw(area.width, area.height, |f| render(f, area, &state));
        (buf, area)
    }

    /// Every choice/list modal's title must start AND end with a space so
    /// `┌ Title ...` renders with breathing room around the label.
    #[test]
    fn all_modal_block_titles_have_padding() {
        for (name, (buf, _area)) in [
            ("SaveDiscardCancel", render_save_discard()),
            ("Confirm", render_confirm()),
            ("MountDstChoice", render_mount_dst()),
            ("TextInput", render_text_input()),
            ("WorkdirPick", render_workdir_pick()),
            ("GithubPicker", render_github_picker()),
            ("AgentPicker", render_role_picker()),
            ("ConfirmSave", render_confirm_save()),
            ("AgentChoice", render_agent_choice()),
        ] {
            let title = top_border_title(&buf);
            assert!(
                title.starts_with(' '),
                "{name} title {title:?} must start with a leading space"
            );
            assert!(
                title.ends_with(' '),
                "{name} title {title:?} must end with a trailing space"
            );
        }
    }

    /// Every modal's top and bottom border runs in `PHOSPHOR_DARK`.
    #[test]
    fn all_modal_borders_are_phosphor_dark() {
        for (name, (buf, area)) in [
            ("SaveDiscardCancel", render_save_discard()),
            ("Confirm", render_confirm()),
            ("MountDstChoice", render_mount_dst()),
            ("TextInput", render_text_input()),
            ("WorkdirPick", render_workdir_pick()),
            ("GithubPicker", render_github_picker()),
            ("AgentPicker", render_role_picker()),
            ("ConfirmSave", render_confirm_save()),
            ("AgentChoice", render_agent_choice()),
        ] {
            assert_border_is_phosphor_dark(&buf, area, name);
        }
    }

    // `all_modal_hint_rows_use_canonical_styles` test removed — hints moved to footer.
}
