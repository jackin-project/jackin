// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    DialogRatatuiSnapshot, PickerItem, usage_body_rect, usage_info_lines_for_width,
    usage_panel_title, usage_tab_strip_area,
};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};
use termrock::style::DesignSystem;
use termrock::widgets::{
    Action, ChoiceDialog, ChoiceDialogState, DetailTableState, Dialog as MessageShell, List,
    ListRow, ListState, MessageDialog, Panel, PanelChrome, Tab, Tabs, TabsState, TextInput,
    TextInputState, Validation,
};

pub(crate) fn render_dialog_ratatui(
    frame: &mut Frame<'_>,
    rect: (u16, u16, u16, u16),
    snapshot: &DialogRatatuiSnapshot,
) {
    let (row, col, height, width) = rect;
    let area = Rect {
        x: col,
        y: row,
        width,
        height,
    };
    if area.right() > frame.area().width || area.bottom() > frame.area().height {
        return;
    }
    match snapshot {
        DialogRatatuiSnapshot::ConfirmAction {
            title,
            body,
            selected_yes,
            data_loss,
        } => {
            render_confirm_action(frame, area, title, body, *selected_yes, *data_loss);
        }
        DialogRatatuiSnapshot::FilterPicker {
            title,
            filter,
            items,
            selected,
            show_filter,
        } => {
            render_filter_picker(frame, area, title, filter, items, *selected, *show_filter);
        }
        DialogRatatuiSnapshot::TextInputDialog {
            dialog_title,
            label,
            value,
            cursor,
        } => {
            render_text_input_dialog(frame, area, dialog_title, label, value, *cursor);
        }
        DialogRatatuiSnapshot::ErrorPopup(state) => {
            let theme = DesignSystem::default();
            let dialog = MessageShell::new(
                &state.title,
                ratatui::text::Text::from(state.message.as_str()),
                &theme,
            )
            .style(Style::default())
            .emphasis(PanelChrome::Focused);
            frame.render_stateful_widget(
                &MessageDialog::new(dialog, &[], &theme).wrap(true),
                area,
                &mut DetailTableState::<usize>::default(),
            );
        }
        DialogRatatuiSnapshot::DebugInfo(state) => {
            crate::tui::components::container_info_surface::render_container_info(
                frame, area, state,
            );
        }
        DialogRatatuiSnapshot::UsageInfo {
            state,
            tabs,
            tab_bar_focused,
            hovered_tab,
        } => {
            render_usage_info(frame, area, state, tabs, *tab_bar_focused, *hovered_tab);
        }
    }
}

pub(crate) fn render_confirm_action(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    body: &str,
    selected_yes: bool,
    data_loss: bool,
) {
    let theme = DesignSystem::default();
    let body = if data_loss {
        "Exit jackin❯?\n\n! Exiting force-stops the container immediately.\n! Work not saved outside the container will be lost.".to_owned()
    } else {
        format!("{title}\n\n{body}")
    };
    let actions = [
        Action {
            id: true,
            label: "Yes",
            enabled: true,
            style: None,
        },
        Action {
            id: false,
            label: "No",
            enabled: true,
            style: None,
        },
    ];
    let dialog = MessageShell::new("Confirm", ratatui::text::Text::from(body), &theme)
        .style(Style::default())
        .emphasis(PanelChrome::Focused);
    frame.render_stateful_widget(
        &ChoiceDialog::new(dialog, &actions).gap(" "),
        area,
        &mut ChoiceDialogState::new(Some(selected_yes)),
    );
}

pub(crate) fn render_usage_info(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    tabs: &[(String, bool)],
    tab_bar_focused: bool,
    hovered_tab: Option<usize>,
) {
    let title = usage_panel_title(state, area.width);
    let theme = DesignSystem::default();
    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some(title.as_str()),
        PanelChrome::Focused,
        &theme,
    );
    if inner.height == 0 {
        return;
    }
    let tab_area = usage_tab_strip_area(inner, tabs);
    let canonical_tabs = tabs
        .iter()
        .enumerate()
        .map(|(id, (label, active))| Tab::new(id, label).active(*active))
        .collect::<Vec<_>>();
    let mut tabs_state = TabsState::new();
    tabs_state.selected = canonical_tabs
        .iter()
        .find(|tab| tab.active)
        .map(|tab| tab.id);
    tabs_state.hovered = hovered_tab;
    tabs_state.focused = tab_bar_focused;
    let base = DesignSystem::default();
    let active_hovered = base
        .style(termrock::style::Role::TabActiveHovered)
        .add_modifier(Modifier::UNDERLINED);
    let inactive_hovered = base
        .style(termrock::style::Role::TabInactiveHovered)
        .add_modifier(Modifier::UNDERLINED);
    let tabs_theme = base
        .with_role(termrock::style::Role::TabActiveHovered, active_hovered)
        .with_role(termrock::style::Role::TabInactiveHovered, inactive_hovered);
    frame.render_stateful_widget(
        &Tabs::new(&canonical_tabs, &tabs_theme).gap(termrock::widgets::TAB_GAP),
        tab_area,
        &mut tabs_state,
    );
    let body = usage_body_rect(area);
    let lines = usage_info_lines_for_width(state, body.width);
    let mut scroll = state.scroll.clone();
    termrock::layout::render_scrollable_dialog_body(frame, area, body, &lines, &mut scroll, &theme);
}

pub(crate) fn render_filter_picker(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    filter: &str,
    items: &[PickerItem],
    selected: usize,
    show_filter: bool,
) {
    let theme = DesignSystem::default();
    let block = Panel::new(&theme)
        .title(title)
        .emphasis(PanelChrome::Focused)
        .block();
    let inner = block.inner(area);
    Clear.render(area, frame.buffer_mut());
    block.render(area, frame.buffer_mut());

    if inner.height < 1 {
        return;
    }

    let list_area = if show_filter {
        let filter_area = Rect { height: 1, ..inner };
        let mut filter_state = TextInputState::new(filter).with_allow_empty(true);
        frame.render_stateful_widget(
            &TextInput::new("Filter", &theme)
                .placeholder("Filter")
                .validation(Validation::Valid),
            filter_area,
            &mut filter_state,
        );
        if inner.height < 3 {
            return;
        }
        Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(2),
            ..inner
        }
    } else {
        inner
    };

    let rows = items
        .iter()
        .enumerate()
        .map(|(id, item)| match item {
            PickerItem::Section(label) => {
                ListRow::separator(id, Line::from(label.clone())).disabled()
            }
            PickerItem::Item(label) => ListRow::item(
                id,
                Line::from(Span::styled(
                    label.clone(),
                    Style::default().fg(DesignSystem::default()
                        .style(termrock::style::Role::Accent)
                        .fg
                        .unwrap_or_default()),
                )),
            ),
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        &List::new(
            &rows,
            &DesignSystem::default().selection(termrock::style::SelectionChrome::Marker),
        ),
        list_area,
        &mut ListState::new(Some(selected)),
    );
}

pub(crate) fn render_text_input_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    dialog_title: &str,
    label: &str,
    value: &str,
    cursor: usize,
) {
    let theme = DesignSystem::default();
    let panel = Panel::new(&theme)
        .title(dialog_title)
        .emphasis(PanelChrome::Focused);
    let inner = panel.inner(area);
    frame.render_widget(&panel, area);
    if inner.height < 2 {
        return;
    }
    frame.render_widget(ratatui::widgets::Paragraph::new(format!("{label}:")), inner);
    let mut state = TextInputState::new(value).with_allow_empty(true);
    assert!(
        state.set_cursor_byte(cursor),
        "text-input snapshot cursor must remain on a grapheme boundary"
    );
    frame.render_stateful_widget(
        &TextInput::new(label, &theme)
            .placeholder("")
            .validation(Validation::Valid),
        Rect {
            y: inner.y.saturating_add(1),
            height: 1,
            ..inner
        },
        &mut state,
    );
}
