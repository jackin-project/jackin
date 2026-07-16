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
            jackin_protocol::control::UsageSeverity::Warn => {
                Some(jackin_core::tui_theme::DEBUG_AMBER)
            }
            jackin_protocol::control::UsageSeverity::Danger => {
                Some(jackin_core::tui_theme::danger_fg())
            }
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
        let mut rows = Vec::new();
        rows.extend([
            crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "Focused",
                Self::usage_focused_label(view),
            ),
            crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "Header",
                Self::usage_provider_header_label(&view.account.provider_label),
            ),
            crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "Provider",
                view.account.provider_label.clone(),
            ),
            crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "Account",
                view.account.account_label.clone(),
            ),
            crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "Status",
                Self::usage_status_label(view.status),
            ),
            crate::tui::components::container_info_surface::ContainerInfoRow::new(
                "Updated",
                view.updated_label.clone(),
            ),
        ]);
        if let Some(username) = &view.account.username {
            rows.push(
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Username",
                    username.clone(),
                ),
            );
        }
        if let Some(plan) = &view.account.plan_label {
            rows.push(
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Plan",
                    plan.clone(),
                ),
            );
        }
        if let Some(origin) = &view.account.credential_origin {
            rows.push(
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Auth",
                    origin.clone(),
                ),
            );
        }
        for bucket in &view.buckets {
            let mut row = crate::tui::components::container_info_surface::ContainerInfoRow::new(
                bucket.label.clone(),
                Self::usage_bucket_value(bucket),
            );
            if let Some(accent) = Self::usage_severity_accent(bucket.severity) {
                row = row.accent(accent);
            }
            rows.push(row);
        }
        if let Some(error) = &view.last_error {
            rows.push(
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Detail",
                    error.clone(),
                ),
            );
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

    fn usage_focused_label(view: &jackin_protocol::control::FocusedUsageView) -> String {
        let account = view.account.account_label.trim();
        let account = if account.is_empty() {
            "account unavailable"
        } else {
            account
        };
        match (&view.focused_agent, &view.focused_provider) {
            (Some(agent), Some(provider)) => format!("{agent} · {provider} · {account}"),
            (Some(agent), None) => format!("{agent} · {account}"),
            (None, Some(provider)) => format!("{provider} · {account}"),
            (None, None) => format!("no focused agent · {account}"),
        }
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

    /// `<prefix>: <used> / <limit>` when both money labels are present, else
    /// whichever single label exists, else nothing. Shared by the spend-cap and
    /// dollar-budget lines so the partial-data rendering can't diverge.
    fn money_cap_part(used: Option<&str>, limit: Option<&str>, prefix: &str) -> Option<String> {
        match (used, limit) {
            (Some(used), Some(limit)) => Some(format!("{prefix}: {used} / {limit}")),
            (Some(label), None) | (None, Some(label)) => Some(label.to_owned()),
            (None, None) => None,
        }
    }

    fn usage_bucket_value(bucket: &jackin_protocol::control::QuotaBucketView) -> String {
        let mut parts = Vec::new();
        // Spend is identified by its semantic slot, not a label string, so a
        // window rename can't silently change how the cap renders (Bug 7 class:
        // presentation driven by data, not labels).
        if bucket.status_slot == Some(jackin_protocol::control::StatusSlot::Spend) {
            if let Some(remaining) = bucket.remaining_percent {
                let used = 100u8.saturating_sub(remaining);
                parts.push(format!("{} {used}% used", Self::usage_meter(used)));
            }
            parts.extend(Self::money_cap_part(
                bucket.used_label.as_deref(),
                bucket.limit_label.as_deref(),
                "Monthly cap",
            ));
            if parts.is_empty()
                || bucket.status != jackin_protocol::control::UsageSnapshotStatus::Fresh
            {
                parts.push(Self::usage_status_label(bucket.status));
            }
            return parts.join(" · ");
        }
        if let Some(remaining) = bucket.remaining_percent {
            if bucket.label == "Credits" && remaining == 0 && bucket.limit_label.is_some() {
                parts.push(format!("{} 0 left", Self::usage_meter(remaining)));
            } else {
                parts.push(format!(
                    "{} {remaining}% left",
                    Self::usage_meter(remaining)
                ));
            }
        }
        // Normal buckets show only `N% left · pace · Resets in …` on the
        // stats line (the roadmap previews never put a used/limit token there;
        // only `Extra usage`, handled above, shows a cap).
        if let Some(pace) = &bucket.pace_label {
            parts.push(pace.clone());
        }
        if let Some(reset) = &bucket.reset_label {
            parts.push(reset.clone());
        }
        // Dollar-bearing windows (Claude codename budgets such as `amber_ladder`,
        // the enterprise contractual budget) carry used/limit money. Show the
        // figures from the data — not a label match — so the global budget's
        // `$0 / $25,000` is visible the way the Extra-usage cap is (Bug 7).
        if bucket.status_slot != Some(jackin_protocol::control::StatusSlot::Spend)
            && (bucket.used_money.is_some() || bucket.limit_money.is_some())
        {
            parts.extend(Self::money_cap_part(
                bucket.used_label.as_deref(),
                bucket.limit_label.as_deref(),
                "Budget",
            ));
        } else if bucket.label == "Credits"
            && bucket.remaining_percent == Some(0)
            && let Some(limit) = &bucket.limit_label
        {
            parts.push(limit.clone());
        }
        if parts.is_empty() || bucket.status != jackin_protocol::control::UsageSnapshotStatus::Fresh
        {
            parts.push(Self::usage_status_label(bucket.status));
        }
        parts.join(" · ")
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

    fn usage_status_label(status: jackin_protocol::control::UsageSnapshotStatus) -> String {
        match status {
            jackin_protocol::control::UsageSnapshotStatus::Fresh => "fresh",
            jackin_protocol::control::UsageSnapshotStatus::Stale => "stale",
            jackin_protocol::control::UsageSnapshotStatus::NeedsLogin => "needs login",
            jackin_protocol::control::UsageSnapshotStatus::NeedsSecret => "needs secret",
            jackin_protocol::control::UsageSnapshotStatus::Unsupported => "unsupported",
            jackin_protocol::control::UsageSnapshotStatus::Unavailable => "unavailable",
            jackin_protocol::control::UsageSnapshotStatus::Error => "error",
        }
        .to_owned()
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
