// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical usage publication and stable account selection for the usage modal.

use jackin_protocol::usage_broker::{
    UsageAccountV2, UsageProjectionV2, UsageProviderV2, UsageUnresolvedGrantV2,
};

use super::Dialog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageDialogTab {
    Overview,
    Provider,
}

/// Display destination only. Refresh authority remains daemon-owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDialogDestination {
    pub provider_id: String,
    pub canonical_account_id: String,
}

impl UsageDialogDestination {
    pub(crate) fn account<'a>(
        &self,
        projection: &'a UsageProjectionV2,
    ) -> Option<(&'a UsageProviderV2, &'a UsageAccountV2)> {
        let provider = projection
            .providers
            .iter()
            .find(|provider| provider.provider_id == self.provider_id)?;
        let account = provider
            .accounts
            .iter()
            .find(|account| account.canonical_account_id == self.canonical_account_id)?;
        Some((provider, account))
    }
}

/// Display selection is either a canonical account or an unresolved configured grant.
/// A grant identity never carries canonical identity or refresh authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageDialogTarget {
    Account(UsageDialogDestination),
    UnresolvedGrant {
        configured_account_id: String,
        surface_id: String,
    },
}

impl UsageDialogTarget {
    pub(crate) fn canonical_destination(&self) -> Option<&UsageDialogDestination> {
        match self {
            Self::Account(account) => Some(account),
            Self::UnresolvedGrant { .. } => None,
        }
    }

    pub(crate) fn unresolved_grant<'a>(
        &self,
        projection: &'a UsageProjectionV2,
    ) -> Option<&'a UsageUnresolvedGrantV2> {
        let Self::UnresolvedGrant {
            configured_account_id,
            surface_id,
        } = self
        else {
            return None;
        };
        projection.unresolved_grants.iter().find(|grant| {
            grant.configured_account_id == *configured_account_id && grant.surface_id == *surface_id
        })
    }

    fn exists(&self, projection: &UsageProjectionV2) -> bool {
        match self {
            Self::Account(account) => account.account(projection).is_some(),
            Self::UnresolvedGrant { .. } => self.unresolved_grant(projection).is_some(),
        }
    }
}

/// Owned canonical rendering snapshot. Labels never encode state or geometry.
#[derive(Debug, Clone)]
pub(crate) struct UsageDialogState {
    pub projection: Option<Box<UsageProjectionV2>>,
    pub destination: Option<UsageDialogTarget>,
    pub notice: Option<String>,
    pub transport_error: Option<String>,
    pub refresh_unavailable: bool,
    pub scroll: termrock::scroll::DialogScroll,
}

pub(crate) fn usage_destinations(projection: &UsageProjectionV2) -> Vec<UsageDialogTarget> {
    projection
        .providers
        .iter()
        .flat_map(|provider| {
            provider.accounts.iter().map(|account| {
                UsageDialogTarget::Account(UsageDialogDestination {
                    provider_id: provider.provider_id.clone(),
                    canonical_account_id: account.canonical_account_id.clone(),
                })
            })
        })
        .chain(projection.unresolved_grants.iter().map(|grant| {
            UsageDialogTarget::UnresolvedGrant {
                configured_account_id: grant.configured_account_id.clone(),
                surface_id: grant.surface_id.clone(),
            }
        }))
        .collect()
}

impl Dialog {
    pub(crate) fn usage_state(&self) -> Option<UsageDialogState> {
        let Self::Usage {
            projection,
            selected,
            destination,
            notice,
            transport_error,
            refresh_unavailable,
            scroll,
            ..
        } = self
        else {
            return None;
        };
        Some(UsageDialogState {
            projection: projection.clone(),
            destination: (*selected == UsageDialogTab::Provider)
                .then(|| destination.clone())
                .flatten(),
            notice: notice.clone(),
            transport_error: transport_error.clone(),
            refresh_unavailable: *refresh_unavailable,
            scroll: scroll.clone(),
        })
    }

    pub(super) fn usage_tab_index_at(
        projection: Option<&UsageProjectionV2>,
        destination: Option<&UsageDialogTarget>,
        selected: UsageDialogTab,
        area: ratatui::layout::Rect,
        row: u16,
        col: u16,
    ) -> Option<usize> {
        let inner = crate::tui::components::dialog_widgets::usage_dialog_inner_area(area);
        let tabs = crate::tui::components::dialog_widgets::usage_tab_strip_labels(
            projection,
            destination,
            selected,
        );
        let tab_area = crate::tui::components::dialog_widgets::usage_tab_strip_area(inner, &tabs);
        if row != tab_area.y {
            return None;
        }
        crate::tui::components::dialog_widgets::usage_tab_strip_index_at(&tabs, tab_area, col)
    }

    pub(super) fn usage_provider_tab_target(
        &mut self,
        step: isize,
    ) -> Option<UsageDialogDestination> {
        let Self::Usage {
            projection,
            selected,
            destination,
            notice,
            refresh_unavailable,
            scroll,
            ..
        } = self
        else {
            return None;
        };
        let targets = projection
            .as_deref()
            .map(usage_destinations)
            .unwrap_or_default();
        if targets.is_empty() {
            return None;
        }
        let current = if *selected == UsageDialogTab::Provider {
            destination
                .as_ref()
                .and_then(|destination| targets.iter().position(|target| target == destination))
                .map(|index| index + 1)
                .unwrap_or(0)
        } else {
            0
        };
        let count = targets.len() + 1;
        let next = if step >= 0 {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        *notice = None;
        *scroll = termrock::scroll::DialogScroll::new();
        if next == 0 {
            *selected = UsageDialogTab::Overview;
            *destination = None;
            *refresh_unavailable = false;
            None
        } else {
            match targets.get(next - 1).cloned()? {
                UsageDialogTarget::Account(account) => Some(account),
                target @ UsageDialogTarget::UnresolvedGrant { .. } => {
                    *selected = UsageDialogTab::Provider;
                    *destination = Some(target);
                    *refresh_unavailable = true;
                    None
                }
            }
        }
    }

    pub(super) fn select_usage_tab_target(
        &mut self,
        target: UsageDialogTarget,
    ) -> super::DialogAction {
        match target {
            UsageDialogTarget::Account(account) => super::DialogAction::SwitchUsageProvider {
                provider_id: account.provider_id,
                canonical_account_id: account.canonical_account_id,
            },
            target @ UsageDialogTarget::UnresolvedGrant { .. } => {
                self.select_usage_target(target);
                super::DialogAction::Redraw
            }
        }
    }

    /// Select only an exact currently published canonical account.
    pub fn select_usage_destination(&mut self, target: UsageDialogDestination) -> bool {
        self.select_usage_target(UsageDialogTarget::Account(target))
    }

    pub fn select_usage_target(&mut self, target: UsageDialogTarget) -> bool {
        let Self::Usage {
            projection,
            selected,
            destination,
            notice,
            refresh_unavailable,
            scroll,
            ..
        } = self
        else {
            return false;
        };
        *refresh_unavailable = false;
        if !projection
            .as_deref()
            .is_some_and(|projection| target.exists(projection))
        {
            *selected = UsageDialogTab::Overview;
            *destination = None;
            *notice = Some("Selected account unavailable; showing Overview".to_owned());
            *scroll = termrock::scroll::DialogScroll::new();
            return false;
        }
        *selected = UsageDialogTab::Provider;
        *refresh_unavailable = matches!(target, UsageDialogTarget::UnresolvedGrant { .. });
        *destination = Some(target);
        *notice = None;
        *scroll = termrock::scroll::DialogScroll::new();
        true
    }

    /// A successful read replaces canonical data and clears a prior transport error.
    pub fn apply_usage_projection(&mut self, publication: UsageProjectionV2) -> bool {
        self.apply_usage_snapshot(Some(publication), None)
    }

    /// Apply one complete daemon read state atomically. Retained data plus an
    /// unchanged transport error must not oscillate between recovered/failed.
    pub fn apply_usage_snapshot(
        &mut self,
        publication: Option<UsageProjectionV2>,
        error: Option<String>,
    ) -> bool {
        let Self::Usage {
            projection,
            selected,
            destination,
            notice,
            transport_error,
            refresh_unavailable,
            scroll,
            hovered_tab,
            ..
        } = self
        else {
            return false;
        };
        let error_changed = *transport_error != error;
        *transport_error = error;
        let Some(publication) = publication else {
            return error_changed;
        };
        if projection.as_deref() == Some(&publication) {
            return error_changed;
        }
        let removed = destination
            .as_ref()
            .is_some_and(|destination| !destination.exists(&publication));
        *projection = Some(Box::new(publication));
        if removed {
            *refresh_unavailable = false;
            *destination = None;
            *selected = UsageDialogTab::Overview;
            *notice = Some("Previously selected account unavailable; showing Overview".to_owned());
        }
        *hovered_tab = None;
        *scroll = termrock::scroll::DialogScroll::new();
        true
    }

    /// Transient read failure preserves authorized last-good inventory.
    pub fn apply_usage_error(&mut self, error: String) -> bool {
        let Self::Usage {
            transport_error, ..
        } = self
        else {
            return false;
        };
        if transport_error.as_ref() == Some(&error) {
            return false;
        }
        *transport_error = Some(error);
        true
    }

    /// Manual refresh route feedback is independent of selection and read failures.
    pub fn apply_usage_refresh_unavailable(&mut self, unavailable: bool) -> bool {
        let Self::Usage {
            refresh_unavailable,
            ..
        } = self
        else {
            return false;
        };
        if *refresh_unavailable == unavailable {
            return false;
        }
        *refresh_unavailable = unavailable;
        true
    }

    pub fn apply_usage_notice(&mut self, message: String) -> bool {
        let Self::Usage { notice, .. } = self else {
            return false;
        };
        if notice.as_ref() == Some(&message) {
            return false;
        }
        *notice = Some(message);
        true
    }

    /// Revoked authorization invalidates the retained inventory itself.
    pub fn revoke_usage_projection(&mut self, error: String) -> bool {
        let Self::Usage {
            projection,
            destination,
            selected,
            transport_error,
            refresh_unavailable,
            notice,
            hovered_tab,
            scroll,
            ..
        } = self
        else {
            return false;
        };
        let changed = projection.is_some()
            || destination.is_some()
            || *selected != UsageDialogTab::Overview
            || *refresh_unavailable
            || transport_error.as_ref() != Some(&error);
        *projection = None;
        *refresh_unavailable = false;
        *destination = None;
        *selected = UsageDialogTab::Overview;
        *notice = None;
        *transport_error = Some(error);
        *hovered_tab = None;
        *scroll = termrock::scroll::DialogScroll::new();
        changed
    }

    pub fn usage_destination(&self) -> Option<&UsageDialogDestination> {
        let Self::Usage {
            selected: UsageDialogTab::Provider,
            destination,
            ..
        } = self
        else {
            return None;
        };
        destination.as_ref()?.canonical_destination()
    }

    #[cfg(test)]
    pub(crate) fn usage_selected_tab(&self) -> Option<UsageDialogTab> {
        let Self::Usage { selected, .. } = self else {
            return None;
        };
        Some(*selected)
    }

    #[must_use]
    pub fn new_usage(projection: Option<UsageProjectionV2>) -> Self {
        Self::new_usage_with_destination(projection, None)
    }

    #[must_use]
    pub fn new_usage_with_destination(
        projection: Option<UsageProjectionV2>,
        destination: Option<UsageDialogDestination>,
    ) -> Self {
        let mut dialog = Self::Usage {
            projection: projection.map(Box::new),
            selected: UsageDialogTab::Overview,
            destination: None,
            notice: None,
            transport_error: None,
            refresh_unavailable: false,
            tab_bar_focused: true,
            hovered_tab: None,
            scroll: termrock::scroll::DialogScroll::new(),
        };
        if let Some(destination) = destination {
            dialog.select_usage_destination(destination);
        }
        dialog
    }

    #[cfg(test)]
    pub(crate) fn new_usage_with_tab(
        projection: Option<UsageProjectionV2>,
        selected: UsageDialogTab,
    ) -> Self {
        let destination = if selected == UsageDialogTab::Provider {
            projection.as_ref().and_then(|projection| {
                usage_destinations(projection)
                    .into_iter()
                    .find_map(|target| match target {
                        UsageDialogTarget::Account(account) => Some(account),
                        UsageDialogTarget::UnresolvedGrant { .. } => None,
                    })
            })
        } else {
            None
        };
        Self::new_usage_with_destination(projection, destination)
    }
}
