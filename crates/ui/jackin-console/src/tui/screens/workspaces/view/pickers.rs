// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace picker sidebars.

use super::{panel, picker_sidebar_title};
use ratatui::{Frame, layout::Rect, text::Line};

#[must_use]
pub fn account_picker_title(container_id: Option<&str>) -> String {
    container_id.map_or_else(
        || " Account ".to_owned(),
        |container_id| format!(" {container_id} — Account "),
    )
}

pub fn render_picker_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    labels: Vec<String>,
    selected: Option<usize>,
    focused: bool,
) {
    let theme = termrock::style::DesignSystem::default();
    let block = panel(&theme, Some(title), focused).block();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = labels
        .into_iter()
        .enumerate()
        .map(|(id, label)| termrock::widgets::ListRow::item(id, Line::from(label)))
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        &termrock::widgets::List::new(&rows, &theme),
        inner,
        &mut termrock::widgets::ListState::new(focused.then_some(selected).flatten()),
    );
}

pub fn render_account_picker_sidebar(
    frame: &mut Frame<'_>,
    area: Rect,
    container_id: Option<&str>,
    labels: Vec<String>,
    selected: usize,
    focused: bool,
) {
    let title = account_picker_title(container_id);
    render_picker_sidebar(frame, area, &title, labels, Some(selected), focused);
}

pub fn render_role_picker_sidebar<R: crate::tui::components::role_picker::RoleChoice>(
    frame: &mut Frame<'_>,
    area: Rect,
    workspace_name: &str,
    picker: &crate::tui::components::role_picker::RolePickerState<R>,
    focused: bool,
) {
    let title = picker_sidebar_title(workspace_name);
    let labels = picker
        .filtered
        .iter()
        .map(crate::tui::components::role_picker::RoleChoice::key)
        .collect();
    render_picker_sidebar(
        frame,
        area,
        &title,
        labels,
        picker.list_state.selected().copied(),
        focused,
    );
}

pub fn render_agent_picker_sidebar<A: crate::tui::components::agent_choice::AgentChoice>(
    frame: &mut Frame<'_>,
    area: Rect,
    role_name: &str,
    picker: &crate::tui::components::agent_choice::AgentChoiceState<A>,
    focused: bool,
) {
    let title = picker_sidebar_title(role_name);
    let labels = picker
        .choices
        .iter()
        .map(|agent| crate::tui::components::agent_choice::agent_picker_label(*agent).to_owned())
        .collect();
    let selected = picker
        .choices
        .iter()
        .position(|agent| *agent == picker.focused);
    render_picker_sidebar(frame, area, &title, labels, selected, focused);
}
