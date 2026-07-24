// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `UsageDialogTab` type and usage method family extracted from the dialog
//! coordinator. Free type re-exported from parent. `usage_tab_index_at` and
//! `usage_provider_tab_target` promoted per plan.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageDialogTab {
    Overview,
    Provider,
}

use super::Dialog;

impl Dialog {
    /// Accent colour for a usage bucket's meter by severity. `Normal` keeps the
    /// default (no accent → phosphor green); `Warn`/`Danger` grade toward amber
    /// and red so an account approaching its cap reads as such at a glance.
    fn usage_severity_accent(
        severity: jackin_protocol::control::UsageSeverity,
    ) -> Option<ratatui::style::Color> {
        match severity {
            jackin_protocol::control::UsageSeverity::Normal => None,
            jackin_protocol::control::UsageSeverity::Warn => Some(jackin_tui::tokens::DEBUG_AMBER),
            jackin_protocol::control::UsageSeverity::Danger => Some(
                termrock::Theme::default()
                    .style(termrock::style::Role::Danger)
                    .fg
                    .unwrap_or_default(),
            ),
        }
    }

    pub(crate) fn usage_state(
        &self,
    ) -> Option<crate::tui::components::container_info_surface::ContainerInfoState> {
        let Self::Usage {
            view,
            selected,
            scroll,
            ..
        } = self
        else {
            return None;
        };
        if *selected == UsageDialogTab::Overview {
            return Some(Self::usage_overview_state(view, scroll.clone()));
        }
        // Rust owns every provider-card field, string, and order in
        // `jackin_usage::usage::usage_detail_presentation` so this dialog and the
        // native Desktop Usage window stay parity-locked. The dialog only maps
        // each shared row to a `ContainerInfoRow`, prepending its TUI meter glyph
        // (geometry from `meter_percent`) to a bucket's leading segment.
        let presentation = jackin_usage::usage::usage_detail_presentation(view);
        let mut rows = Vec::with_capacity(presentation.rows.len());
        for row in &presentation.rows {
            let value = match row.meter_percent {
                Some(meter_percent) => {
                    format!("{} {}", Self::usage_meter(meter_percent), row.display_label)
                }
                None => row.display_label.clone(),
            };
            let mut info_row =
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    row.label.clone(),
                    value,
                );
            if let Some(accent) = Self::usage_severity_accent(row.severity) {
                info_row = info_row.accent(accent);
            }
            rows.push(info_row);
        }
        let mut state =
            crate::tui::components::container_info_surface::ContainerInfoState::new("Usage", rows);
        state.scroll = scroll.clone();
        Some(state)
    }

    fn usage_overview_state(
        view: &jackin_protocol::control::FocusedUsageView,
        scroll: termrock::scroll::DialogScroll,
    ) -> crate::tui::components::container_info_surface::ContainerInfoState {
        let mut rows = Vec::new();
        if view.tabs.is_empty() {
            rows.push(
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Providers",
                    "usage unavailable",
                ),
            );
        } else {
            for tab in &view.tabs {
                // One quota-focused line per provider, matching the Overview
                // preview: "<provider>  <quota summary / lifecycle>". The
                // account identity lives in the focused header above, not on
                // every row. status_label is the daemon-enriched
                // "Session 37% left · Resets in 1h 21m" (or a lifecycle word).
                let quota = if tab.status_label.trim().is_empty() {
                    "status unavailable"
                } else {
                    tab.status_label.trim()
                };
                let value = quota.to_owned();
                rows.push(
                    crate::tui::components::container_info_surface::ContainerInfoRow::new(
                        Self::usage_provider_header_label(&tab.label),
                        value,
                    ),
                );
            }
        }
        let mut state =
            crate::tui::components::container_info_surface::ContainerInfoState::new("Usage", rows);
        state.scroll = scroll;
        state
    }

    fn usage_provider_header_label(label: &str) -> String {
        crate::tui::components::dialog_widgets::usage_provider_display_label(label).to_owned()
    }

    pub(super) fn usage_tab_index_at(
        view: &jackin_protocol::control::FocusedUsageView,
        selected: UsageDialogTab,
        area: ratatui::layout::Rect,
        row: u16,
        col: u16,
    ) -> Option<usize> {
        let inner = crate::tui::components::dialog_widgets::usage_dialog_inner_area(area);
        let tabs = crate::tui::components::dialog_widgets::usage_tab_strip_labels(view, selected);
        let tab_area = crate::tui::components::dialog_widgets::usage_tab_strip_area(inner, &tabs);
        let row0 = if row == tab_area.y.saturating_add(1) {
            row.saturating_sub(1)
        } else {
            row
        };
        let col0 = if col >= tab_area.x.saturating_add(1) {
            col.saturating_sub(1)
        } else {
            col
        };
        if row0 != tab_area.y {
            return None;
        }
        crate::tui::components::dialog_widgets::usage_tab_strip_index_at(&tabs, tab_area, col0)
    }

    pub(super) fn usage_provider_tab_target(&mut self, step: isize) -> Option<String> {
        let Self::Usage { view, selected, .. } = self else {
            return None;
        };
        if view.tabs.is_empty() {
            return None;
        }
        if *selected == UsageDialogTab::Overview {
            // tabs is non-empty (guarded above), so first/last are always Some.
            let target = if step >= 0 {
                view.tabs.first()
            } else {
                view.tabs.last()
            };
            return target.map(|tab| tab.label.clone());
        }
        let current = view.tabs.iter().position(|tab| tab.active).unwrap_or(0);
        if step < 0 && current == 0 {
            *selected = UsageDialogTab::Overview;
            return None;
        }
        let next = if step >= 0 && current + 1 >= view.tabs.len() {
            *selected = UsageDialogTab::Overview;
            return None;
        } else if step >= 0 {
            current + 1
        } else {
            current - 1
        };
        Some(view.tabs[next].label.clone())
    }

    #[cfg(test)]
    pub(crate) fn usage_selected_tab(&self) -> Option<UsageDialogTab> {
        let Self::Usage { selected, .. } = self else {
            return None;
        };
        Some(*selected)
    }

    fn usage_meter(remaining_percent: u8) -> String {
        const WIDTH: usize = 32;
        let remaining = usize::from(remaining_percent.min(100));
        let filled = if remaining >= 100 {
            WIDTH
        } else {
            remaining * WIDTH / 100
        };
        format!(
            "{}{}",
            "█".repeat(filled),
            "·".repeat(WIDTH.saturating_sub(filled))
        )
    }

    pub fn new_usage(view: jackin_protocol::control::FocusedUsageView) -> Self {
        Self::new_usage_with_tab(view, UsageDialogTab::Provider)
    }

    pub(crate) fn new_usage_with_tab(
        view: jackin_protocol::control::FocusedUsageView,
        selected: UsageDialogTab,
    ) -> Self {
        Self::Usage {
            view: Box::new(view),
            selected,
            tab_bar_focused: true,
            hovered_tab: None,
            scroll: termrock::scroll::DialogScroll::new(),
        }
    }
}
