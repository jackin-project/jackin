// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings save preview line rendering.

use super::{
    MountPreviewRow, SettingsEnvPreview, SettingsGeneralPreview, SettingsSavePreview,
    TrustPreviewRow, mount_map, settings_auth_diff_lines, settings_default_account_lines,
    settings_env_diff_lines, settings_mount_diff_lines, settings_trust_diff_lines,
};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::collections::{BTreeMap, BTreeSet};

#[must_use]
pub fn settings_save_lines(preview: &SettingsSavePreview) -> Vec<Line<'static>> {
    let heading = termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong);
    let add_style = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::Accent)
        .fg
        .unwrap_or_default());
    let remove_style = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::TextMuted)
        .fg
        .unwrap_or_default());
    let sep_style = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::ScrollTrack)
        .fg
        .unwrap_or_default());

    let mut out: Vec<Line<'static>> = Vec::new();

    out.push(Line::from(Span::styled("Save settings", heading)));
    out.push(Line::raw(""));

    let general_stats = settings_general_stats(preview.general);
    let mount_stats = settings_mount_stats(&preview.mounts_original, &preview.mounts_pending);
    let env_stats = settings_env_stats(&preview.env_original, &preview.env_pending);
    let auth_stats = settings_auth_stats(&preview.auth_original, &preview.auth_pending);
    let trust_stats = settings_trust_stats(&preview.trust_original, &preview.trust_pending);

    if let Some(s) = general_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  General:      ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = mount_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Mounts:       ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = env_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Environments: ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = auth_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Accounts:     ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = trust_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Trust:        ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }

    out.push(Line::raw(""));
    out.push(Line::from(Span::styled("  \u{2500}".repeat(30), sep_style)));
    out.push(Line::raw(""));

    if general_stats.is_some() {
        out.push(Line::from(Span::styled("General:", heading)));
        let arrow = "\u{2192}";

        if preview.general.pending_toggles.coauthor_trailer
            != preview.general.original_toggles.coauthor_trailer
        {
            let from = enabled_label(preview.general.original_toggles.coauthor_trailer);
            let to = enabled_label(preview.general.pending_toggles.coauthor_trailer);
            out.push(Line::from(vec![
                Span::styled("  co-author trailer: ", heading),
                Span::styled(from, remove_style),
                Span::styled(format!(" {arrow} "), Style::default()),
                Span::styled(to, add_style),
            ]));
        }

        if preview.general.pending_toggles.dco != preview.general.original_toggles.dco {
            let from = enabled_label(preview.general.original_toggles.dco);
            let to = enabled_label(preview.general.pending_toggles.dco);
            out.push(Line::from(vec![
                Span::styled("  dco: ", heading),
                Span::styled(from, remove_style),
                Span::styled(format!(" {arrow} "), Style::default()),
                Span::styled(to, add_style),
            ]));
        }

        out.push(Line::raw(""));
    }

    let mount_lines = settings_mount_diff_lines(
        &preview.mounts_original,
        &preview.mounts_pending,
        add_style,
        remove_style,
    );
    if !mount_lines.is_empty() {
        out.push(Line::from(Span::styled("Mounts:", heading)));
        out.extend(mount_lines);
        out.push(Line::raw(""));
    }

    let env_lines = settings_env_diff_lines(
        &preview.env_original,
        &preview.env_pending,
        add_style,
        remove_style,
    );
    if !env_lines.is_empty() {
        out.push(Line::from(Span::styled("Environments:", heading)));
        out.extend(env_lines);
        out.push(Line::raw(""));
    }

    let auth_lines = settings_auth_diff_lines(
        &preview.auth_original,
        &preview.auth_pending,
        add_style,
        remove_style,
    );
    if !auth_lines.is_empty() {
        out.push(Line::from(Span::styled("Accounts:", heading)));
        out.extend(auth_lines);
        out.push(Line::raw(""));
    }

    out.extend(settings_default_account_lines(preview, heading));

    if preview.github_original != preview.github_pending {
        out.push(Line::from(Span::styled("GitHub authentication:", heading)));
        out.push(Line::from(Span::styled(
            format!(
                "  mode: {:?} → {:?} (credential hidden)",
                preview.github_original.auth_forward, preview.github_pending.auth_forward
            ),
            add_style,
        )));
        out.push(Line::raw(""));
    }

    let trust_lines = settings_trust_diff_lines(
        &preview.trust_original,
        &preview.trust_pending,
        add_style,
        remove_style,
    );
    if !trust_lines.is_empty() {
        out.push(Line::from(Span::styled("Trust:", heading)));
        out.extend(trust_lines);
        out.push(Line::raw(""));
    }

    while out
        .last()
        .is_some_and(|l| l.spans.is_empty() || l.spans.iter().all(|s| s.content.trim().is_empty()))
    {
        out.pop();
    }

    out
}

pub(crate) fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

pub(crate) fn settings_general_stats(state: SettingsGeneralPreview) -> Option<String> {
    let count = state.change_count();
    if count == 0 {
        return None;
    }
    Some(if count == 1 {
        "1 change".to_owned()
    } else {
        format!("{count} changes")
    })
}

pub(crate) fn settings_mount_stats(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
) -> Option<String> {
    let (added, removed, modified) = mount_diff_counts(original, pending);
    summarize_diff_counts(added, removed, modified)
}

pub(crate) fn settings_env_stats(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
) -> Option<String> {
    let (added, removed, modified) = env_config_diff_counts(original, pending);
    summarize_diff_counts(added, removed, modified)
}

pub(crate) fn summarize_diff_counts(
    added: usize,
    removed: usize,
    modified: usize,
) -> Option<String> {
    if added + removed + modified == 0 {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if added > 0 {
        parts.push(format!("{added} added"));
    }
    if removed > 0 {
        parts.push(format!("{removed} removed"));
    }
    if modified > 0 {
        parts.push(format!("{modified} modified"));
    }
    Some(parts.join(", "))
}

pub(crate) fn settings_auth_stats(
    original: &BTreeMap<String, jackin_config::AccountConfig>,
    pending: &BTreeMap<String, jackin_config::AccountConfig>,
) -> Option<String> {
    let ids: BTreeSet<_> = original.keys().chain(pending.keys()).collect();
    let changed = ids
        .into_iter()
        .filter(|id| original.get(*id) != pending.get(*id))
        .count();
    (changed > 0).then(|| format!("{changed} changed"))
}

pub(crate) fn settings_trust_stats(
    original: &[TrustPreviewRow],
    pending: &[TrustPreviewRow],
) -> Option<String> {
    let changed = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a.trusted != b.trusted)
        .count();
    if changed == 0 {
        return None;
    }
    Some(format!("{changed} changed"))
}

pub(crate) fn mount_diff_counts(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
) -> (usize, usize, usize) {
    let orig_map = mount_map(original);
    let pend_map = mount_map(pending);
    let added = pend_map
        .keys()
        .filter(|k| !orig_map.contains_key(*k))
        .count();
    let removed = orig_map
        .keys()
        .filter(|k| !pend_map.contains_key(*k))
        .count();
    let modified = pend_map
        .iter()
        .filter(|(k, prow)| orig_map.get(*k).is_some_and(|orow| orow != *prow))
        .count();
    (added, removed, modified)
}

pub(crate) fn env_config_diff_counts(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
) -> (usize, usize, usize) {
    let (ga, gr, gm) = env_map_diff_counts(&original.env, &pending.env);
    let all_roles: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = BTreeMap::default();
    let (ra, rr, rm) = all_roles.into_iter().fold((0, 0, 0), |(a, r, m), role| {
        let oe = original.roles.get(role).unwrap_or(&empty);
        let pe = pending.roles.get(role).unwrap_or(&empty);
        let (da, dr, dm) = env_map_diff_counts(oe, pe);
        (a + da, r + dr, m + dm)
    });
    (ga + ra, gr + rr, gm + rm)
}

pub(crate) fn env_map_diff_counts(
    original: &BTreeMap<String, String>,
    pending: &BTreeMap<String, String>,
) -> (usize, usize, usize) {
    let added = pending
        .keys()
        .filter(|k| !original.contains_key(*k))
        .count();
    let removed = original
        .keys()
        .filter(|k| !pending.contains_key(*k))
        .count();
    let modified = pending
        .iter()
        .filter(|(k, v)| original.get(*k).is_some_and(|ov| ov != *v))
        .count();
    (added, removed, modified)
}
