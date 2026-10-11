// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn draw<F: FnOnce(&mut Frame<'_>)>(width: u16, height: u16, render: F) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render(f)).unwrap();
    term.backend().buffer().clone()
}

pub(super) fn render_save_discard() -> (Buffer, Rect) {
    use crate::tui::components::{SaveDiscardState, render_save_discard_dialog as render};
    let area = Rect::new(0, 0, 70, 7);
    let state = SaveDiscardState::new("Save changes?");
    let buf = draw(area.width, area.height, |f| render(f, area, &state));
    (buf, area)
}

pub(super) fn render_confirm() -> (Buffer, Rect) {
    use crate::tui::components::{ConfirmState, render_confirm_dialog as render};
    let area = Rect::new(0, 0, 60, 7);
    let state = ConfirmState::new("Delete workspace?");
    let buf = draw(area.width, area.height, |f| render(f, area, &state));
    (buf, area)
}

pub(super) fn render_mount_dst() -> (Buffer, Rect) {
    use crate::tui::components::mount_dst_choice::{MountDstChoiceState, render};
    let area = Rect::new(0, 0, 80, 8);
    let state = MountDstChoiceState::new("/home/user/app");
    let buf = draw(area.width, area.height, |f| render(f, area, &state));
    (buf, area)
}

pub(super) fn render_confirm_save() -> (Buffer, Rect) {
    use crate::tui::components::confirm_save::{ConfirmSaveState, render};
    use ratatui::text::Line;
    let area = Rect::new(0, 0, 70, 10);
    let state = ConfirmSaveState::<jackin_config::MountConfig>::new(vec![
        Line::from("Create workspace: demo"),
        Line::from(""),
        Line::from("Working directory: /home/user/demo"),
    ]);
    let buf = draw(area.width, area.height, |f| render(f, area, &state));
    (buf, area)
}

pub(super) fn row_text(buf: &Buffer, y: u16) -> String {
    (buf.area.x..buf.area.x + buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect()
}

pub(super) fn button_row_y(buf: &Buffer, labels: &[&str]) -> u16 {
    (buf.area.y..buf.area.y + buf.area.height)
        .find(|y| {
            let row = row_text(buf, *y);
            labels.iter().all(|label| row.contains(label))
        })
        .expect("button row should be visible")
}

pub(super) fn render_manager_state(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    width: u16,
    height: u16,
) -> String {
    let buf = render_manager_buffer(state, config, cwd, width, height);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn render_manager_buffer(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    width: u16,
    height: u16,
) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    prepare_for_render(state, config, cwd, area);
    terminal
        .draw(|frame| render(frame, area, state, config, cwd))
        .unwrap();
    terminal.backend().buffer().clone()
}

#[expect(
    clippy::excessive_nesting,
    reason = "Focused-region flood fill has nested traversal and membership checks"
)]
pub(super) fn focused_region_count(buf: &Buffer) -> usize {
    let area = buf.area;
    let mut seen = std::collections::BTreeSet::<(u16, u16)>::new();
    let mut clusters = 0usize;

    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let coord = (x, y);
            if seen.contains(&coord) || !is_focused_region_cell(buf, coord) {
                continue;
            }
            clusters += 1;
            let mut stack = vec![coord];
            seen.insert(coord);
            while let Some((cx, cy)) = stack.pop() {
                for next in neighbors(cx, cy, area) {
                    if seen.insert(next) && is_focused_region_cell(buf, next) {
                        stack.push(next);
                    }
                }
            }
        }
    }

    clusters
}

pub(super) fn neighbors(x: u16, y: u16, area: Rect) -> impl Iterator<Item = (u16, u16)> {
    let min_x = area.x;
    let min_y = area.y;
    let max_x = area.x + area.width - 1;
    let max_y = area.y + area.height - 1;
    [
        x.checked_sub(1).map(|nx| (nx, y)),
        (x < max_x).then_some((x + 1, y)),
        y.checked_sub(1).map(|ny| (x, ny)),
        (y < max_y).then_some((x, y + 1)),
    ]
    .into_iter()
    .flatten()
    .filter(move |(nx, ny)| *nx >= min_x && *ny >= min_y)
}

pub(super) fn is_focused_region_cell(buf: &Buffer, coord: (u16, u16)) -> bool {
    let cell = &buf[coord];
    // Product focus chrome via jackin helpers (TermRock owns Role→RGB tables).
    let focused = termrock::style::DesignSystem::default()
        .style(termrock::style::Role::BorderFocused)
        .fg;
    cell.fg == focused.unwrap_or_default()
}

pub(super) fn test_cwd() -> std::path::PathBuf {
    std::path::PathBuf::from("/workspace")
}

pub(super) fn detail_config() -> AppConfig {
    toml::from_str(
        r#"
[roles."chainargos/agent-smith"]
git = "https://example.invalid/agent-smith.git"

[docker.mounts]
cache = { src = "/cache", dst = "/cache", readonly = false }

[docker.mounts."chainargos/agent-smith"]
secrets = { src = "/secrets", dst = "/secrets", readonly = true }

[workspaces.ws]
workdir = "/workspace"
allowed_roles = ["chainargos/agent-smith"]

[[workspaces.ws.mounts]]
src = "/workspace"
dst = "/workspace"
readonly = false
"#,
    )
    .expect("valid detail-pane config")
}

pub(super) fn list_with_modal<'a>(
    config: &AppConfig,
    cwd: &std::path::Path,
    modal: Modal<'a>,
) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(config, cwd);
    state.list_modal = Some(modal);
    state
}

pub(super) fn settings_mounts_with_modal<'a>(
    config: &AppConfig,
    cwd: &std::path::Path,
    modal: SettingsModal<'a>,
) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
    settings.active_tab = crate::tui::state::SettingsTab::Mounts;
    settings.set_active_content_focused(true);
    settings.mounts.modals.open(modal);
    state.stage = ManagerStage::Settings(settings);
    state
}

pub(super) fn settings_env_with_modal<'a>(
    config: &AppConfig,
    cwd: &std::path::Path,
    modal: SettingsModal<'a>,
) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
    settings.active_tab = crate::tui::state::SettingsTab::Environments;
    settings.set_active_content_focused(true);
    settings.env.modals.open(modal);
    state.stage = ManagerStage::Settings(settings);
    state
}

pub(super) fn settings_auth_with_modal(
    config: &AppConfig,
    cwd: &std::path::Path,
    modal: SettingsModal<'static>,
) -> ManagerState<'static> {
    let mut state = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
    settings.active_tab = crate::tui::state::SettingsTab::Auth;
    settings.set_active_content_focused(true);
    settings.auth.modals.open(modal);
    state.stage = ManagerStage::Settings(settings);
    state
}

pub(super) fn auth_form_modal() -> Modal<'static> {
    let kind = crate::tui::auth::AuthKind::Claude;
    Modal::AuthForm {
        target: crate::tui::state::AuthFormTarget::Workspace { kind },
        state: Box::new(crate::tui::state::AuthForm::new(kind)),
        focus: crate::tui::state::AuthFormFocus::Mode,
        literal_buffer: String::new(),
    }
}
