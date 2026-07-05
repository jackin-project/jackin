use std::collections::{BTreeMap, BTreeSet};

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::settings::{
    AuthPreviewRow, MountPreviewRow, SettingsEnvPreview, SettingsGeneralPreview,
    SettingsSavePreview, TrustPreviewRow,
};

#[must_use]
pub fn settings_save_lines(preview: &SettingsSavePreview) -> Vec<Line<'static>> {
    let heading = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let add_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
    let remove_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let sep_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DARK);

    let mut out: Vec<Line<'static>> = Vec::new();
    out.push(Line::from(Span::styled("Save settings", heading)));
    out.push(Line::raw(""));

    let stats = SettingsPreviewStats::new(preview);
    append_settings_summary(&mut out, &stats, heading, add_style);

    out.push(Line::raw(""));
    out.push(Line::from(Span::styled("  \u{2500}".repeat(30), sep_style)));
    out.push(Line::raw(""));

    append_settings_details(&mut out, preview, &stats, heading, add_style, remove_style);

    while out
        .last()
        .is_some_and(|l| l.spans.is_empty() || l.spans.iter().all(|s| s.content.trim().is_empty()))
    {
        out.pop();
    }

    out
}

#[derive(Debug, Clone)]
struct SettingsPreviewStats {
    general: Option<String>,
    mounts: Option<String>,
    env: Option<String>,
    auth: Option<String>,
    trust: Option<String>,
}

impl SettingsPreviewStats {
    fn new(preview: &SettingsSavePreview) -> Self {
        Self {
            general: settings_general_stats(preview.general),
            mounts: settings_mount_stats(&preview.mounts_original, &preview.mounts_pending),
            env: settings_env_stats(&preview.env_original, &preview.env_pending),
            auth: settings_auth_stats(
                &preview.auth_original,
                &preview.auth_pending,
                &preview.auth_github_env_original,
                &preview.auth_github_env_pending,
            ),
            trust: settings_trust_stats(&preview.trust_original, &preview.trust_pending),
        }
    }
}

fn append_settings_summary(
    out: &mut Vec<Line<'static>>,
    stats: &SettingsPreviewStats,
    heading: Style,
    add_style: Style,
) {
    for (label, value) in [
        ("  General:      ", stats.general.as_deref()),
        ("  Mounts:       ", stats.mounts.as_deref()),
        ("  Environments: ", stats.env.as_deref()),
        ("  Auth:         ", stats.auth.as_deref()),
        ("  Trust:        ", stats.trust.as_deref()),
    ] {
        if let Some(value) = value {
            out.push(Line::from(vec![
                Span::styled(label, heading),
                Span::styled(value.to_owned(), add_style),
            ]));
        }
    }
}

fn append_settings_details(
    out: &mut Vec<Line<'static>>,
    preview: &SettingsSavePreview,
    stats: &SettingsPreviewStats,
    heading: Style,
    add_style: Style,
    remove_style: Style,
) {
    append_settings_general_lines(out, preview, stats, heading, add_style, remove_style);
    append_settings_section(
        out,
        "Mounts:",
        settings_mount_diff_lines(
            &preview.mounts_original,
            &preview.mounts_pending,
            add_style,
            remove_style,
        ),
        heading,
    );
    append_settings_section(
        out,
        "Environments:",
        settings_env_diff_lines(
            &preview.env_original,
            &preview.env_pending,
            add_style,
            remove_style,
        ),
        heading,
    );
    append_settings_section(
        out,
        "Auth:",
        settings_auth_diff_lines(
            &preview.auth_original,
            &preview.auth_pending,
            &preview.auth_github_env_original,
            &preview.auth_github_env_pending,
            add_style,
            remove_style,
        ),
        heading,
    );
    append_settings_section(
        out,
        "Trust:",
        settings_trust_diff_lines(
            &preview.trust_original,
            &preview.trust_pending,
            add_style,
            remove_style,
        ),
        heading,
    );
}

fn append_settings_general_lines(
    out: &mut Vec<Line<'static>>,
    preview: &SettingsSavePreview,
    stats: &SettingsPreviewStats,
    heading: Style,
    add_style: Style,
    remove_style: Style,
) {
    if stats.general.is_none() {
        return;
    }
    out.push(Line::from(Span::styled("General:", heading)));
    append_settings_toggle_line(
        out,
        "  co-author trailer: ",
        preview.general.original_toggles.coauthor_trailer,
        preview.general.pending_toggles.coauthor_trailer,
        heading,
        add_style,
        remove_style,
    );
    append_settings_toggle_line(
        out,
        "  dco: ",
        preview.general.original_toggles.dco,
        preview.general.pending_toggles.dco,
        heading,
        add_style,
        remove_style,
    );
    out.push(Line::raw(""));
}

fn append_settings_toggle_line(
    out: &mut Vec<Line<'static>>,
    label: &str,
    original: bool,
    pending: bool,
    heading: Style,
    add_style: Style,
    remove_style: Style,
) {
    if pending == original {
        return;
    }
    let arrow = "\u{2192}";
    out.push(Line::from(vec![
        Span::styled(label.to_owned(), heading),
        Span::styled(enabled_label(original), remove_style),
        Span::styled(format!(" {arrow} "), Style::default()),
        Span::styled(enabled_label(pending), add_style),
    ]));
}

fn append_settings_section(
    out: &mut Vec<Line<'static>>,
    title: &str,
    lines: Vec<Line<'static>>,
    heading: Style,
) {
    if lines.is_empty() {
        return;
    }
    out.push(Line::from(Span::styled(title.to_owned(), heading)));
    out.extend(lines);
    out.push(Line::raw(""));
}

pub(super) fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

fn settings_general_stats(state: SettingsGeneralPreview) -> Option<String> {
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

fn settings_mount_stats(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
) -> Option<String> {
    let orig_map = mount_map(original);
    let pend_map = mount_map(pending);
    let (added, removed, modified) = diff_counts(&orig_map, &pend_map);
    summarize_change_counts(added, removed, modified)
}

fn settings_env_stats(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
) -> Option<String> {
    let (ga, gr, gm) = diff_counts(&original.env, &pending.env);
    let all_roles: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = BTreeMap::default();
    let (ra, rr, rm) = all_roles.into_iter().fold((0, 0, 0), |(a, r, m), role| {
        let oe = original.roles.get(role).unwrap_or(&empty);
        let pe = pending.roles.get(role).unwrap_or(&empty);
        let (da, dr, dm) = diff_counts(oe, pe);
        (a + da, r + dr, m + dm)
    });
    let (added, removed, modified) = (ga + ra, gr + rr, gm + rm);
    summarize_change_counts(added, removed, modified)
}

fn summarize_change_counts(added: usize, removed: usize, modified: usize) -> Option<String> {
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

fn settings_auth_stats(
    original: &[AuthPreviewRow],
    pending: &[AuthPreviewRow],
    orig_github_env: &BTreeMap<String, String>,
    pend_github_env: &BTreeMap<String, String>,
) -> Option<String> {
    let row_changes = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a.mode != b.mode)
        .count();
    let (env_added, env_removed, env_modified) = diff_counts(orig_github_env, pend_github_env);
    let total = row_changes + env_added + env_removed + env_modified;
    if total == 0 {
        return None;
    }
    Some(format!("{total} changed"))
}

fn settings_trust_stats(
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

fn diff_counts<K, V>(original: &BTreeMap<K, V>, pending: &BTreeMap<K, V>) -> (usize, usize, usize)
where
    K: Ord,
    V: PartialEq,
{
    let added = pending
        .keys()
        .filter(|key| !original.contains_key(*key))
        .count();
    let removed = original
        .keys()
        .filter(|key| !pending.contains_key(*key))
        .count();
    let modified = pending
        .iter()
        .filter(|(key, pending)| {
            original
                .get(*key)
                .is_some_and(|original| original != *pending)
        })
        .count();
    (added, removed, modified)
}

fn settings_mount_diff_lines(
    original: &[MountPreviewRow],
    pending: &[MountPreviewRow],
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let orig_map = mount_map(original);
    let pend_map = mount_map(pending);

    let mut out: Vec<Line<'static>> = Vec::new();
    for (key, row) in &pend_map {
        if !orig_map.contains_key(key) {
            out.push(Line::from(Span::styled(
                format!("  + {}", mount_row_summary(row)),
                add_style,
            )));
        }
    }
    for (key, row) in &orig_map {
        if !pend_map.contains_key(key) {
            out.push(Line::from(Span::styled(
                format!("  - {}", mount_row_summary(row)),
                remove_style,
            )));
        }
    }
    for (key, prow) in &pend_map {
        if let Some(orow) = orig_map.get(key)
            && orow != prow
        {
            out.push(Line::from(Span::styled(
                format!("  ~ {}", mount_row_summary(prow)),
                add_style,
            )));
            out.push(Line::from(Span::styled(
                format!("      was: {}", mount_row_summary(orow)),
                remove_style,
            )));
        }
    }
    out
}

fn mount_map(rows: &[MountPreviewRow]) -> BTreeMap<(Option<String>, String), &MountPreviewRow> {
    rows.iter()
        .map(|row| ((row.scope.clone(), row.name.clone()), row))
        .collect()
}

fn mount_row_summary(row: &MountPreviewRow) -> String {
    let scope = row
        .scope
        .as_deref()
        .map(|s| format!("[{s}] "))
        .unwrap_or_default();
    let ro = if row.readonly { " (ro)" } else { "" };
    format!("{scope}{} \u{2192} {}{ro}", row.src, row.dst)
}

pub(super) fn settings_env_diff_lines(
    original: &SettingsEnvPreview,
    pending: &SettingsEnvPreview,
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    append_env_map_diff_lines(
        &mut out,
        None,
        &original.env,
        &pending.env,
        add_style,
        remove_style,
    );
    let all_roles: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = BTreeMap::default();
    for role in all_roles {
        let oe = original.roles.get(role).unwrap_or(&empty);
        let pe = pending.roles.get(role).unwrap_or(&empty);
        let mut probe: Vec<Line<'static>> = Vec::new();
        append_env_map_diff_lines(&mut probe, None, oe, pe, add_style, remove_style);
        if !probe.is_empty() {
            out.push(Line::from(Span::styled(
                format!("  role {role}:"),
                add_style,
            )));
            append_env_map_diff_lines(&mut out, Some("  "), oe, pe, add_style, remove_style);
        }
    }
    out
}

pub fn append_env_map_diff_lines(
    out: &mut Vec<Line<'static>>,
    indent: Option<&str>,
    original: &BTreeMap<String, String>,
    pending: &BTreeMap<String, String>,
    value: Style,
    dim: Style,
) {
    let prefix = indent.unwrap_or("");
    for (k, v) in pending {
        match original.get(k) {
            Some(ov) if ov == v => {}
            _ => out.push(Line::from(Span::styled(
                format!("{prefix}  + {k} = {v}"),
                value,
            ))),
        }
    }
    for k in original.keys() {
        if !pending.contains_key(k) {
            out.push(Line::from(Span::styled(format!("{prefix}  - {k}"), dim)));
        }
    }
}

fn settings_auth_diff_lines(
    original: &[AuthPreviewRow],
    pending: &[AuthPreviewRow],
    orig_github_env: &BTreeMap<String, String>,
    pend_github_env: &BTreeMap<String, String>,
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for (orig_row, pend_row) in original.iter().zip(pending.iter()) {
        if orig_row.mode != pend_row.mode {
            out.push(Line::from(Span::styled(
                format!(
                    "  ~ {}  {} \u{2192} {}",
                    pend_row.label, orig_row.mode, pend_row.mode
                ),
                add_style,
            )));
        }
    }
    append_env_map_diff_lines(
        &mut out,
        None,
        orig_github_env,
        pend_github_env,
        add_style,
        remove_style,
    );
    out
}

fn settings_trust_diff_lines(
    original: &[TrustPreviewRow],
    pending: &[TrustPreviewRow],
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for (orig_row, pend_row) in original.iter().zip(pending.iter()) {
        if orig_row.trusted != pend_row.trusted {
            let (label, style) = if pend_row.trusted {
                (format!("  + {}  trusted", pend_row.role), add_style)
            } else {
                (format!("  - {}  untrusted", pend_row.role), remove_style)
            };
            out.push(Line::from(Span::styled(label, style)));
        }
    }
    out
}
