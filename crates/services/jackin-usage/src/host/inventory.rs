// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `HostUsageRuntime` overview and desktop inventory.

use super::{
    AccountLifecycle, HostDesktopInventory, HostDesktopProjection, HostDesktopProviderGroup,
    HostDesktopProviderProjection, HostDesktopProviderState, HostOverviewRow,
    HostProviderGlanceRow, HostSurfaceId, HostUsageRuntime, account_descriptor, accounts,
    build_provider_glance_row, drive_label_prefix, driving_bucket_from_view, view_is_auto_detected,
    worst_severity_label,
};

use std::time::Duration;

use jackin_protocol::control::FocusedUsageView;

use crate::usage::{
    compact_duration_label, exact_reset_parenthetical, percent_headline, provider_display_label,
    reset_label_with_prefs, usage_display_status_label, usage_identity_presentation,
    usage_status_storage_label,
};

impl HostUsageRuntime {
    /// Next network refresh relative to the floor (`Next update in …` / due).
    #[must_use]
    pub fn next_refresh_label(&self) -> String {
        match self.last_refresh {
            None => "Next update due".to_owned(),
            Some(last) => {
                let floor = Duration::from_secs(self.refresh_floor_secs);
                let elapsed = last.elapsed();
                if elapsed >= floor {
                    "Next update due".to_owned()
                } else {
                    let remain = floor.saturating_sub(elapsed);
                    let secs = i64::try_from(remain.as_secs()).unwrap_or(i64::MAX);
                    format!("Next update in {}", compact_duration_label(secs.max(0)))
                }
            }
        }
    }

    /// Overview rows for every **enabled** surface in `ALL` order.
    pub fn overview_rows(&mut self) -> Result<Vec<HostOverviewRow>, String> {
        self.require_open()?;
        let prefs = self.format_prefs;
        let now = chrono::Utc::now().timestamp();
        let mut rows = Vec::new();
        for surface in HostSurfaceId::ALL.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            let view = self.snapshot(surface.id())?;
            let status_word = usage_status_storage_label(view.status).to_owned();
            let severity = worst_severity_label(&view);
            let display_label = provider_display_label(surface.label()).to_owned();

            let mut headline = String::new();
            let mut reset_label = None;
            let mut exact_reset = None;
            if let Some(drive) = driving_bucket_from_view(&view) {
                // Optional model-scoped bucket name prefix (Fable, Sonnet, …).
                if let Some(prefix) = drive_label_prefix(&view, drive.remaining) {
                    headline.push_str(prefix);
                    headline.push(' ');
                }
                headline.push_str(&percent_headline(drive.remaining, prefs));
                if let Some(at) = drive.resets_at {
                    reset_label = Some(reset_label_with_prefs(at, now, prefs));
                    exact_reset = Some(exact_reset_parenthetical(at));
                }
            }

            rows.push(HostOverviewRow {
                surface_id: surface.id().to_owned(),
                display_label,
                headline,
                reset_label,
                exact_reset,
                status_word,
                severity,
            });
        }
        Ok(rows)
    }

    /// One atomic, Rust-owned grouped account projection for jackin❯ desktop.
    pub fn desktop_inventory(&mut self) -> Result<HostDesktopInventory, String> {
        self.require_open()?;
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, HostSurfaceId::DESKTOP_PROVIDER_ORDER)?;
        self.desktop_inventory_for_catalog(&catalog)
    }

    pub(crate) fn desktop_inventory_for_catalog(
        &mut self,
        catalog: &accounts::AccountCatalog,
    ) -> Result<HostDesktopInventory, String> {
        let now = chrono::Utc::now().timestamp();
        let prefs = self.format_prefs;
        let mut groups = Vec::new();
        for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            }
            let entries = catalog.entries_for_surface(surface);
            let has_current = entries
                .iter()
                .any(|entry| entry.lifecycle == AccountLifecycle::Current);
            let provider_state = catalog.provider_state(surface);
            let detected = if self.selected_account_missing(catalog, surface)
                || has_current
                || provider_state.is_some_and(view_is_auto_detected)
            {
                self.desktop_detected_surfaces
                    .insert(surface.id().to_owned());
                true
            } else if provider_state.is_some_and(FocusedUsageView::is_refreshing_placeholder) {
                self.desktop_detected_surfaces.contains(surface.id())
            } else {
                self.desktop_detected_surfaces.remove(surface.id());
                false
            };
            if !detected {
                continue;
            }
            let selected = self.selected_accounts.get(surface.id()).map(String::as_str);
            let accounts = entries
                .into_iter()
                .map(|entry| {
                    account_descriptor(
                        surface,
                        entry,
                        selected == Some(entry.account_key.as_str()),
                        now,
                        prefs,
                    )
                })
                .collect::<Vec<_>>();
            let empty_state = accounts.is_empty().then(|| {
                let view = self
                    .selected_view_for_catalog(catalog, surface)
                    .unwrap_or_else(|| {
                        self.cache
                            .focused_snapshot(Some(surface.agent_slug()), surface.provider_label())
                    });
                let is_refreshing = view.is_refreshing_placeholder();
                HostDesktopProviderState {
                    status_word: usage_status_storage_label(view.status).to_owned(),
                    status_label: usage_display_status_label(view.status).to_owned(),
                    updated_label: view.updated_label,
                    last_error: view.last_error,
                    is_refreshing,
                }
            });
            let display_label = provider_display_label(surface.label()).to_owned();
            let plan_or_status_label = empty_state
                .as_ref()
                .filter(|state| state.status_word != "fresh")
                .map_or_else(|| "—".to_owned(), |state| state.status_label.clone());
            let accessibility_label = empty_state.as_ref().map_or_else(
                || display_label.clone(),
                |state| format!("{display_label}, {}", state.status_label),
            );
            groups.push(HostDesktopProviderGroup {
                surface_id: surface.id().to_owned(),
                display_label,
                icon_key: surface.id().to_owned(),
                fallback_glyph: surface.fallback_glyph().to_owned(),
                usage_url: surface.usage_url().map(str::to_owned),
                account_column_label: "—".to_owned(),
                plan_or_status_label,
                remaining_label: "—".to_owned(),
                reset_display_label: "—".to_owned(),
                accessibility_label,
                accounts,
                empty_state,
            });
        }
        Ok(HostDesktopInventory { groups })
    }

    /// Build the complete native Desktop model from one uninterrupted runtime
    /// snapshot. The `boltffi` bridge holds the runtime mutex for this whole call,
    /// so no broker generation can interleave partial provider/account state.
    pub fn desktop_projection(
        &mut self,
        status_bar_max: u32,
    ) -> Result<HostDesktopProjection, String> {
        self.require_open()?;
        let surfaces = self.list_surfaces()?;
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, HostSurfaceId::DESKTOP_PROVIDER_ORDER)?;
        let inventory = self.desktop_inventory_for_catalog(&catalog)?;
        let mut providers = Vec::with_capacity(inventory.groups.len());
        for group in inventory.groups {
            let surface = HostSurfaceId::from_id(&group.surface_id)
                .ok_or_else(|| format!("unknown desktop provider: {}", group.surface_id))?;
            let (selected_account_route, selected_usage) =
                self.selected_route_and_view_for_catalog(&catalog, surface);
            let selected_usage = selected_usage.unwrap_or_else(|| {
                self.cache
                    .focused_snapshot(Some(surface.agent_slug()), surface.provider_label())
            });
            let selected_usage = self.with_discovery_diagnostic(surface, selected_usage);
            let is_updating = self.surface_refresh_in_progress(surface.id());
            let identity =
                usage_identity_presentation(&group.display_label, &selected_usage, is_updating);
            providers.push(HostDesktopProviderProjection {
                group,
                selected_account_route,
                selected_usage,
                identity,
                is_updating,
            });
        }
        let glance_rows = self.provider_glance_rows()?;
        let status_bar_glance_rows = self.status_bar_provider_glance_rows(status_bar_max)?;
        let diagnostics = self.discovery_diagnostics()?;
        let global_messages = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.surface_id.is_none())
            .map(|diagnostic| {
                format!(
                    "{}: {}",
                    diagnostic.scope_label,
                    diagnostic.issue.display_message()
                )
            })
            .collect::<Vec<_>>();
        Ok(HostDesktopProjection {
            generation: self.next_seq,
            refresh_in_progress: self.broker_refresh_in_progress(),
            error_message: (!global_messages.is_empty()).then(|| global_messages.join("\n")),
            next_refresh_label: self.next_refresh_label(),
            surfaces,
            providers,
            glance_rows,
            status_bar_glance_rows,
            diagnostics,
        })
    }

    /// Detected providers in the canonical Desktop model order, each a
    /// selected-account-aware glance row. Iterates only
    /// [`HostSurfaceId::DESKTOP_PROVIDER_ORDER`], materializes account sources
    /// once, resolves exact selected-account ownership, and re-evaluates
    /// detection on every call. Affirmative evidence inserts membership, a
    /// non-refreshing view without evidence removes it, and the cold refreshing
    /// placeholder alone reuses prior membership so refresh cannot drop a row.
    /// Returns an empty vector for zero detected providers.
    #[must_use = "the glance rows are the Desktop surface source"]
    pub fn provider_glance_rows(&mut self) -> Result<Vec<HostProviderGlanceRow>, String> {
        self.require_open()?;
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, HostSurfaceId::DESKTOP_PROVIDER_ORDER)?;
        let prefs = self.format_prefs;
        let now = chrono::Utc::now().timestamp();
        let mut rows = Vec::new();
        for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            }
            let Some(view) = self.selected_view_for_catalog(&catalog, surface) else {
                self.desktop_detected_surfaces.remove(surface.id());
                continue;
            };
            let detected = if self.selected_account_missing(&catalog, surface)
                || view_is_auto_detected(&view)
            {
                self.desktop_detected_surfaces
                    .insert(surface.id().to_owned());
                true
            } else if view.is_refreshing_placeholder() {
                self.desktop_detected_surfaces.contains(surface.id())
            } else {
                self.desktop_detected_surfaces.remove(surface.id());
                false
            };
            if detected {
                rows.push(build_provider_glance_row(
                    surface,
                    &view,
                    self.surface_refresh_in_progress(surface.id()),
                    now,
                    prefs,
                ));
            }
        }
        Ok(rows)
    }
}
