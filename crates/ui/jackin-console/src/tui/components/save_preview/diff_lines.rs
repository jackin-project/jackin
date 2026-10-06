// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings diff line rendering.

use super::{MountPreviewRow, SettingsEnvPreview, SettingsSavePreview, TrustPreviewRow};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn settings_mount_diff_lines(
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

pub(crate) fn mount_map(
    rows: &[MountPreviewRow],
) -> BTreeMap<(Option<String>, String), &MountPreviewRow> {
    rows.iter()
        .map(|row| ((row.scope.clone(), row.name.clone()), row))
        .collect()
}

pub(crate) fn mount_row_summary(row: &MountPreviewRow) -> String {
    let scope = row
        .scope
        .as_deref()
        .map(|s| format!("[{s}] "))
        .unwrap_or_default();
    let ro = if row.readonly { " (ro)" } else { "" };
    format!("{scope}{} \u{2192} {}{ro}", row.src, row.dst)
}

pub(crate) fn settings_env_diff_lines(
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
    let credential_keys = crate::tui::auth_config::auth_credential_env_keys();
    for (k, v) in pending {
        if credential_keys.contains(k.as_str()) {
            continue;
        }
        match original.get(k) {
            Some(ov) if ov == v => {}
            _ => out.push(Line::from(Span::styled(
                format!("{prefix}  + {k} = {v}"),
                value,
            ))),
        }
    }
    for k in original.keys() {
        if credential_keys.contains(k.as_str()) {
            continue;
        }
        if !pending.contains_key(k) {
            out.push(Line::from(Span::styled(format!("{prefix}  - {k}"), dim)));
        }
    }
}

pub(crate) fn settings_default_account_lines(
    preview: &SettingsSavePreview,
    heading: Style,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    if preview.bindings_original != preview.bindings_pending {
        out.push(Line::from(Span::styled("Default accounts:", heading)));
        for agent in jackin_core::Agent::ALL {
            let before = preview.bindings_original.get(agent);
            let after = preview.bindings_pending.get(agent);
            if before != after {
                out.push(Line::from(format!(
                    "  {}: {} → {}",
                    agent.label(),
                    before.map_or("automatic", String::as_str),
                    after.map_or("automatic", String::as_str)
                )));
            }
        }
        out.push(Line::raw(""));
    }

    out
}

pub(crate) fn settings_auth_diff_lines(
    original: &BTreeMap<String, jackin_config::AccountConfig>,
    pending: &BTreeMap<String, jackin_config::AccountConfig>,
    add_style: Style,
    remove_style: Style,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for (id, account) in pending {
        if original.get(id) != Some(account) {
            let verb = if original.contains_key(id) {
                "updated"
            } else {
                "added"
            };
            out.push(Line::from(Span::styled(
                format!(
                    "  + {} [{id}] ({verb}; {}; credential hidden)",
                    account.name,
                    if account.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                add_style,
            )));
        }
    }
    for (id, account) in original {
        if !pending.contains_key(id) {
            out.push(Line::from(Span::styled(
                format!("  - {} [{id}]", account.name),
                remove_style,
            )));
        }
    }
    out
}

pub(crate) fn settings_trust_diff_lines(
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
