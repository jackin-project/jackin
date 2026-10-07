// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `HostUsageRuntime` status bar labels.

use super::{
    DrivingBucket, HostProviderGlanceRow, HostSurfaceId, HostUsageRuntime, STATUS_BAR_MAX_CHIPS,
    build_provider_glance_row, driving_bucket_from_view, glance_bucket, status_bar_rank_key,
    view_is_auto_detected,
};

use jackin_usage_provider_core::{UsageFormatPrefs, compact_duration_label, estimate_caption};

impl HostUsageRuntime {
    /// Compact bar label for one enabled surface, if known.
    pub fn status_bar_label(&mut self, surface_id: &str) -> Result<Option<String>, String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled.contains(surface.id()) {
            return Ok(None);
        }
        Ok(Some(self.snapshot(surface_id)?.status_bar_label))
    }

    /// Merged compact bar text from enabled surfaces that have labels.
    pub fn merged_status_bar_label(&mut self) -> Result<String, String> {
        self.require_open()?;
        let mut parts = Vec::new();
        for surface in HostSurfaceId::ALL {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            let label = self.snapshot(surface.id())?.status_bar_label;
            // Skip pure loading noise when other surfaces already contribute.
            if label == "refreshing" && !parts.is_empty() {
                continue;
            }
            parts.push(format!("{}: {label}", surface.label()));
        }
        if parts.is_empty() {
            Ok("jackin❯ usage".to_owned())
        } else {
            Ok(parts.join(" · "))
        }
    }

    /// Presentation-time format prefs (defaults match shipped Capsule strings).
    pub fn set_format_prefs(&mut self, prefs: UsageFormatPrefs) -> Result<(), String> {
        self.require_open()?;
        self.format_prefs = prefs;
        Ok(())
    }

    /// Current presentation-time format prefs.
    #[must_use]
    pub fn format_prefs(&self) -> UsageFormatPrefs {
        self.format_prefs
    }

    /// Short status-item label: enabled surface with the **least remaining**
    /// (lowest `remaining_percent` across its buckets). Default
    /// [`PercentStyle::Left`] shows remaining (e.g. `Cl 37%`);
    /// [`PercentStyle::Used`] shows used percent (e.g. `Cl 63%`).
    ///
    /// Never invents percentages — only uses Rust-provided `remaining_percent`.
    /// Empty when no enabled surface has a numeric remaining value (all
    /// unavailable / disabled / still refreshing without last-good data).
    /// Ties keep the earlier surface in [`HostSurfaceId::ALL`] order.
    /// Depleted (`remaining == 0`) with `resets_at` renders `Cl resets 1h 21m`.
    pub fn compact_status_bar_label(&mut self) -> Result<String, String> {
        self.require_open()?;
        let mut best: Option<(u8, HostSurfaceId, Option<i64>)> = None;
        for surface in HostSurfaceId::ALL.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            let Some(drive) = self.driving_bucket_for(surface) else {
                continue;
            };
            match best {
                Some((best_remaining, _, _)) if drive.remaining >= best_remaining => {}
                _ => best = Some((drive.remaining, surface, drive.resets_at)),
            }
        }
        let prefs = self.format_prefs;
        Ok(match best {
            Some((remaining, surface, resets_at)) => {
                Self::format_compact_entry(surface, remaining, resets_at, prefs)
            }
            None => String::new(),
        })
    }

    /// Pinned-surface compact label (e.g. `Cx 59%` remaining / depleted form).
    /// `None` when disabled or no numeric remaining.
    pub fn compact_status_bar_label_for(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<String>, String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled.contains(surface.id()) {
            return Ok(None);
        }
        let Some(drive) = self.driving_bucket_for(surface) else {
            return Ok(None);
        };
        Ok(Some(Self::format_compact_entry(
            surface,
            drive.remaining,
            drive.resets_at,
            self.format_prefs,
        )))
    }

    /// Burn-first multi-surface strip (SB-3/14/17/19), capped ≤3, joined with ` · `.
    ///
    /// Eligible surfaces only (numeric remaining **> 0**). Order: **soonest
    /// reset first**, then **higher remaining %** (SB-17). Never more than
    /// [`STATUS_BAR_MAX_CHIPS`] tokens.
    pub fn compact_status_bar_strip(&mut self, max: u32) -> Result<String, String> {
        self.require_open()?;
        let cap = (max as usize).clamp(1, STATUS_BAR_MAX_CHIPS);
        let prefs = self.format_prefs;
        let mut rows: Vec<(u8, HostSurfaceId, Option<i64>)> = Vec::new();
        for surface in HostSurfaceId::ALL.iter().copied() {
            if !self.enabled.contains(surface.id()) {
                continue;
            }
            if let Some(drive) = self.driving_bucket_for(surface) {
                // SB-19: depleted never appears on the burn-first bar.
                if drive.remaining == 0 {
                    continue;
                }
                rows.push((drive.remaining, surface, drive.resets_at));
            }
        }
        rows.sort_by_key(|(remaining, surface, resets_at)| {
            let (time_key, rem_key) = status_bar_rank_key(*remaining, *resets_at);
            (
                time_key,
                rem_key,
                HostSurfaceId::ALL
                    .iter()
                    .position(|s| *s == *surface)
                    .unwrap_or(usize::MAX),
            )
        });
        let parts: Vec<String> = rows
            .into_iter()
            .take(cap)
            .map(|(remaining, surface, resets_at)| {
                Self::format_compact_entry(surface, remaining, resets_at, prefs)
            })
            .collect();
        Ok(parts.join(" · "))
    }

    /// Desktop **status-bar** provider chips only (SB-3/14/17/19).
    ///
    /// Unlike [`Self::provider_glance_rows`] (full inventory for popover/Usage),
    /// this drops 0% rows, ranks soonest-then-remaining, and hard-caps at
    /// [`STATUS_BAR_MAX_CHIPS`]. `max` is clamped into `1…STATUS_BAR_MAX_CHIPS`.
    #[must_use = "status-bar rows are the multi-item NSStatusItem source"]
    pub fn status_bar_provider_glance_rows(
        &mut self,
        max: u32,
    ) -> Result<Vec<HostProviderGlanceRow>, String> {
        self.require_open()?;
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, HostSurfaceId::DESKTOP_PROVIDER_ORDER)?;
        let cap = (max as usize).clamp(1, STATUS_BAR_MAX_CHIPS);
        let prefs = self.format_prefs;
        let now = chrono::Utc::now().timestamp();
        let mut candidates: Vec<(u8, Option<i64>, HostProviderGlanceRow)> = Vec::new();
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
            if !detected {
                continue;
            }
            let glance = glance_bucket(surface, &view);
            let remaining = glance.and_then(|b| b.remaining_percent);
            // SB-19: no numeric remaining or 0% → out of bar membership.
            let Some(rem) = remaining else {
                continue;
            };
            if rem == 0 {
                continue;
            }
            let resets_at = glance.and_then(|b| b.resets_at);
            let row = build_provider_glance_row(
                surface,
                &view,
                self.surface_refresh_in_progress(surface.id()),
                now,
                prefs,
            );
            candidates.push((rem, resets_at, row));
        }
        candidates.sort_by_key(|(remaining, resets_at, row)| {
            let (time_key, rem_key) = status_bar_rank_key(*remaining, *resets_at);
            (
                time_key,
                rem_key,
                HostSurfaceId::DESKTOP_PROVIDER_ORDER
                    .iter()
                    .position(|s| s.id() == row.surface_id)
                    .unwrap_or(usize::MAX),
            )
        });
        Ok(candidates
            .into_iter()
            .take(cap)
            .map(|(_, _, row)| row)
            .collect())
    }

    /// Estimate honesty caption for one surface snapshot (presentation-time).
    pub fn estimate_caption_for(&mut self, surface_id: &str) -> Result<Option<String>, String> {
        let view = self.snapshot(surface_id)?;
        Ok(estimate_caption(&view))
    }

    pub(crate) fn driving_bucket_for(&mut self, surface: HostSurfaceId) -> Option<DrivingBucket> {
        let view = self.snapshot(surface.id()).ok()?;
        driving_bucket_from_view(&view)
    }

    /// Compact status token: prefix + percent matching format prefs.
    ///
    /// Default [`PercentStyle::Left`] uses **remaining** (OpenUsage/CodexBar
    /// dual-bucket stack semantics). [`PercentStyle::Used`] flips to used %.
    /// Depleted with `resets_at` keeps the countdown form; depleted without
    /// reset is `Cl 0%` (remaining) or `Cl 100%` (used).
    pub(crate) fn format_compact_entry(
        surface: HostSurfaceId,
        remaining: u8,
        resets_at: Option<i64>,
        prefs: UsageFormatPrefs,
    ) -> String {
        if remaining == 0 {
            if let Some(at) = resets_at {
                let now = chrono::Utc::now().timestamp();
                let secs = at.saturating_sub(now).max(0);
                return format!(
                    "{} resets {}",
                    surface.compact_prefix(),
                    compact_duration_label(secs)
                );
            }
            return match prefs.percent_style {
                jackin_usage_provider_core::PercentStyle::Left => {
                    format!("{} 0%", surface.compact_prefix())
                }
                jackin_usage_provider_core::PercentStyle::Used => {
                    format!("{} 100%", surface.compact_prefix())
                }
            };
        }
        let pct = match prefs.percent_style {
            jackin_usage_provider_core::PercentStyle::Left => remaining,
            jackin_usage_provider_core::PercentStyle::Used => 100u8.saturating_sub(remaining),
        };
        format!("{} {pct}%", surface.compact_prefix())
    }
}
