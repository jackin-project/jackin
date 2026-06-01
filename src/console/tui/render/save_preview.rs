//! Save-confirm preview line builders.
//!
//! Input handlers decide when a save preview opens; this module owns the
//! Ratatui line composition for the preview dialogs.

use crate::config::AppConfig;
use crate::console::tui::state::{EditorMode, EditorState};

#[allow(clippy::too_many_lines)]
pub(crate) fn build_confirm_save_lines(
    editor: &EditorState<'_>,
    config: &AppConfig,
    collapse_lines: &[ratatui::text::Line<'static>],
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let heading = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let value = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
    let dim = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);

    let mut out: Vec<Line<'static>> = Vec::new();

    match &editor.mode {
        EditorMode::Create => {
            let name = editor
                .pending_name
                .clone()
                .unwrap_or_else(|| "(unnamed)".into());
            out.push(Line::from(vec![
                Span::styled("Create workspace: ", heading),
                Span::styled(name, value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Working directory: ", heading),
                Span::styled(crate::tui::shorten_home(&editor.pending.workdir), value),
            ]));
            if !editor.pending.mounts.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled(
                    format!("Mounts ({}):", editor.pending.mounts.len()),
                    heading,
                )));
                for m in &editor.pending.mounts {
                    out.push(Line::from(Span::styled(
                        format!("  \u{2022} {}", mount_summary(m, &editor.mount_info_cache)),
                        value,
                    )));
                }
            }
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Allowed roles: ", heading),
                Span::styled(allowed_agents_summary(editor, config), value),
            ]));
            out.push(Line::raw(""));
            out.push(Line::from(vec![
                Span::styled("Default role: ", heading),
                Span::styled(
                    editor
                        .pending
                        .default_role
                        .clone()
                        .unwrap_or_else(|| "(none)".into()),
                    value,
                ),
            ]));
            if editor.pending.keep_awake.enabled {
                out.push(Line::raw(""));
                out.push(Line::from(vec![
                    Span::styled("Keep awake: ", heading),
                    Span::styled("enabled", value),
                ]));
            }
            if editor.pending.git_pull_on_entry {
                out.push(Line::raw(""));
                out.push(Line::from(vec![
                    Span::styled("Git pull: ", heading),
                    Span::styled("enabled", value),
                ]));
            }
            let env_lines = env_diff_lines(&editor.original, &editor.pending, value, dim);
            if !env_lines.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Env vars:", heading)));
                out.extend(env_lines);
            }
        }
        EditorMode::Edit { name } => {
            let display_name = editor.pending_name.clone().unwrap_or_else(|| name.clone());
            out.push(Line::from(vec![
                Span::styled("Edit workspace: ", heading),
                Span::styled(display_name, value),
            ]));

            if let Some(new_name) = &editor.pending_name
                && new_name != name
            {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Rename:", heading)));
                out.push(Line::from(Span::styled(format!("  - {name}"), dim)));
                out.push(Line::from(Span::styled(format!("  + {new_name}"), value)));
            }

            if editor.pending.workdir != editor.original.workdir {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Working directory:", heading)));
                out.push(Line::from(Span::styled(
                    format!("  - {}", crate::tui::shorten_home(&editor.original.workdir)),
                    dim,
                )));
                out.push(Line::from(Span::styled(
                    format!("  + {}", crate::tui::shorten_home(&editor.pending.workdir)),
                    value,
                )));
            }

            let mount_diffs = crate::console::tui::state::classify_mount_diffs(
                &editor.original.mounts,
                &editor.pending.mounts,
            );
            let any_diff = mount_diffs
                .iter()
                .any(|d| !matches!(d, crate::console::tui::state::MountDiff::Unchanged(_)));
            if any_diff {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Mounts:", heading)));
                for diff in &mount_diffs {
                    match diff {
                        crate::console::tui::state::MountDiff::Added(m) => {
                            out.push(Line::from(Span::styled(
                                format!("  + {}", mount_summary(m, &editor.mount_info_cache)),
                                value,
                            )));
                        }
                        crate::console::tui::state::MountDiff::Removed(m) => {
                            out.push(Line::from(Span::styled(
                                format!("  - {}", mount_summary(m, &editor.mount_info_cache)),
                                dim,
                            )));
                        }
                        crate::console::tui::state::MountDiff::Modified { original, pending } => {
                            // Modified row: show the new state (`~`) with a
                            // dimmed `was:` follow-up so the operator can
                            // see exactly what changed without reading a
                            // remove + add pair.
                            out.push(Line::from(Span::styled(
                                format!("  ~ {}", mount_summary(pending, &editor.mount_info_cache)),
                                value,
                            )));
                            out.push(Line::from(Span::styled(
                                format!(
                                    "      was: {}",
                                    mount_summary(original, &editor.mount_info_cache)
                                ),
                                dim,
                            )));
                        }
                        crate::console::tui::state::MountDiff::Unchanged(_) => {}
                    }
                }
            }

            let added_agents: Vec<_> = editor
                .pending
                .allowed_roles
                .iter()
                .filter(|a| !editor.original.allowed_roles.contains(a))
                .collect();
            let removed_agents: Vec<_> = editor
                .original
                .allowed_roles
                .iter()
                .filter(|a| !editor.pending.allowed_roles.contains(a))
                .collect();
            if !added_agents.is_empty() || !removed_agents.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Allowed roles:", heading)));
                for a in &added_agents {
                    out.push(Line::from(Span::styled(format!("  + {a}"), value)));
                }
                for a in &removed_agents {
                    out.push(Line::from(Span::styled(format!("  - {a}"), dim)));
                }
            }

            if editor.pending.default_role != editor.original.default_role {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Default role:", heading)));
                if let Some(old) = &editor.original.default_role {
                    out.push(Line::from(Span::styled(format!("  - {old}"), dim)));
                }
                if let Some(new) = &editor.pending.default_role {
                    out.push(Line::from(Span::styled(format!("  + {new}"), value)));
                } else {
                    out.push(Line::from(Span::styled("  + (none)", value)));
                }
            }

            if editor.pending.keep_awake.enabled != editor.original.keep_awake.enabled {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Keep awake:", heading)));
                let old_label = if editor.original.keep_awake.enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                let new_label = if editor.pending.keep_awake.enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                out.push(Line::from(Span::styled(format!("  - {old_label}"), dim)));
                out.push(Line::from(Span::styled(format!("  + {new_label}"), value)));
            }

            if editor.pending.git_pull_on_entry != editor.original.git_pull_on_entry {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Git pull:", heading)));
                let old_label = if editor.original.git_pull_on_entry {
                    "enabled"
                } else {
                    "disabled"
                };
                let new_label = if editor.pending.git_pull_on_entry {
                    "enabled"
                } else {
                    "disabled"
                };
                out.push(Line::from(Span::styled(format!("  - {old_label}"), dim)));
                out.push(Line::from(Span::styled(format!("  + {new_label}"), value)));
            }

            let env_lines = env_diff_lines(&editor.original, &editor.pending, value, dim);
            if !env_lines.is_empty() {
                out.push(Line::raw(""));
                out.push(Line::from(Span::styled("Env vars:", heading)));
                out.extend(env_lines);
            }
        }
    }

    if !collapse_lines.is_empty() {
        out.push(Line::raw(""));
        out.push(Line::from(Span::styled(
            "Mount collapse required:",
            heading,
        )));
        out.extend(collapse_lines.iter().cloned());
    }

    out
}

fn mount_summary(
    m: &crate::workspace::MountConfig,
    cache: &jackin_console::mount_info_cache::MountInfoCache,
) -> String {
    let dst = crate::tui::shorten_home(&m.dst);
    let rw = if m.readonly { "ro" } else { "rw" };
    let isolation = m.isolation.as_str();
    let host = if m.src == m.dst {
        String::new()
    } else {
        format!("  host: {}", crate::tui::shorten_home(&m.src))
    };
    format!("{dst}{host}  ({rw}, {isolation}, {})", cache.label(&m.src))
}

fn allowed_agents_summary(editor: &EditorState<'_>, config: &AppConfig) -> String {
    if jackin_console::workspace::allows_all_agents(&editor.pending) {
        return format!("any ({} roles)", config.roles.len());
    }
    editor.pending.allowed_roles.join(", ")
}

/// Per-role sections are prefixed with `  <role>:` so a single
/// "Env vars:" heading hosts both workspace and override deltas.
fn env_diff_lines(
    original: &crate::workspace::WorkspaceConfig,
    pending: &crate::workspace::WorkspaceConfig,
    value: ratatui::style::Style,
    dim: ratatui::style::Style,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
    let mut out: Vec<Line<'static>> = Vec::new();

    append_env_map_diff_lines(&mut out, None, &original.env, &pending.env, value, dim);

    let agent_keys: std::collections::BTreeSet<&String> =
        original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = std::collections::BTreeMap::<String, crate::operator_env::EnvValue>::new();
    for role in agent_keys {
        let orig_env = original.roles.get(role).map_or(&empty, |o| &o.env);
        let pend_env = pending.roles.get(role).map_or(&empty, |p| &p.env);
        // Pre-check if there are any deltas for this role; only emit
        // the role header when there are.
        let mut probe: Vec<Line<'static>> = Vec::new();
        append_env_map_diff_lines(&mut probe, None, orig_env, pend_env, value, dim);
        if !probe.is_empty() {
            out.push(Line::from(Span::styled(format!("  role {role}:"), value)));
            append_env_map_diff_lines(&mut out, Some("  "), orig_env, pend_env, value, dim);
        }
    }
    out
}

/// Append `+ KEY = VALUE` / `- KEY` lines to `out` for the diff between
/// two env maps. `indent` (`None` or `Some("  ")`) controls per-role
/// sub-indent — workspace-level lines use two spaces to match existing
/// diff styling; per-role lines nest one extra level.
pub(crate) fn append_env_map_diff_lines(
    out: &mut Vec<ratatui::text::Line<'static>>,
    indent: Option<&str>,
    original: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pending: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    value: ratatui::style::Style,
    dim: ratatui::style::Style,
) {
    use ratatui::text::{Line, Span};
    let prefix = indent.unwrap_or("");
    for (k, v) in pending {
        match original.get(k) {
            Some(ov) if ov == v => {}
            _ => out.push(Line::from(Span::styled(
                format!("{prefix}  + {k} = {}", v.as_display_str()),
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

pub(crate) fn collapse_section_lines(
    collapses: &[crate::workspace::Removal],
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::Style;
    use ratatui::text::{Line, Span};
    let style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    collapses
        .iter()
        .map(|r| {
            let child = crate::tui::shorten_home(&r.child.src);
            let parent = crate::tui::shorten_home(&r.covered_by.src);
            Line::from(Span::styled(
                format!("  {child} will be subsumed under {parent}"),
                style,
            ))
        })
        .collect()
}

// ── Settings save preview ─────────────────────────────────────────────────────

/// Build the diff preview lines for the settings save confirmation dialog.
/// Mirrors the format of `build_confirm_save_lines` for the workspace editor.
/// Shows a summary section (counts per category) followed by per-category diffs.
#[must_use]
#[allow(clippy::too_many_lines)]
pub(crate) fn build_settings_save_lines(
    settings: &crate::console::tui::state::SettingsState<'_>,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let heading = Style::default()
        .fg(jackin_tui::theme::WHITE)
        .add_modifier(Modifier::BOLD);
    let add_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN);
    let remove_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM);
    let sep_style = Style::default().fg(jackin_tui::theme::PHOSPHOR_DARK);

    let mut out: Vec<Line<'static>> = Vec::new();

    // ── Summary ───────────────────────────────────────────────────────
    out.push(Line::from(Span::styled("Save settings", heading)));
    out.push(Line::raw(""));

    let general_stats = settings_general_stats(&settings.general);
    let mount_stats = settings_mount_stats(&settings.mounts.original, &settings.mounts.pending);
    let env_stats = settings_env_stats(&settings.env.original, &settings.env.pending);
    let auth_stats = settings_auth_stats(
        &settings.auth.original,
        &settings.auth.pending,
        &settings.auth.original_github_env,
        &settings.auth.github_env,
    );
    let trust_stats = settings_trust_stats(&settings.trust.original, &settings.trust.pending);

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
            Span::styled("  Auth:         ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }
    if let Some(s) = trust_stats.as_deref() {
        out.push(Line::from(vec![
            Span::styled("  Trust:        ", heading),
            Span::styled(s.to_owned(), add_style),
        ]));
    }

    // Separator before details
    out.push(Line::raw(""));
    out.push(Line::from(Span::styled("  \u{2500}".repeat(30), sep_style)));
    out.push(Line::raw(""));

    // ── General details ───────────────────────────────────────────────
    if general_stats.is_some() {
        out.push(Line::from(Span::styled("General:", heading)));
        let arrow = "\u{2192}";

        if settings.general.pending_coauthor_trailer != settings.general.original_coauthor_trailer {
            let from = if settings.general.original_coauthor_trailer {
                "enabled"
            } else {
                "disabled"
            };
            let to = if settings.general.pending_coauthor_trailer {
                "enabled"
            } else {
                "disabled"
            };
            out.push(Line::from(vec![
                Span::styled("  co-author trailer: ", heading),
                Span::styled(from, remove_style),
                Span::styled(format!(" {arrow} "), Style::default()),
                Span::styled(to, add_style),
            ]));
        }

        if settings.general.pending_dco != settings.general.original_dco {
            let from = if settings.general.original_dco {
                "enabled"
            } else {
                "disabled"
            };
            let to = if settings.general.pending_dco {
                "enabled"
            } else {
                "disabled"
            };
            out.push(Line::from(vec![
                Span::styled("  dco: ", heading),
                Span::styled(from, remove_style),
                Span::styled(format!(" {arrow} "), Style::default()),
                Span::styled(to, add_style),
            ]));
        }

        out.push(Line::raw(""));
    }

    // ── Mount details ─────────────────────────────────────────────────
    let mount_lines = settings_mount_diff_lines(
        &settings.mounts.original,
        &settings.mounts.pending,
        add_style,
        remove_style,
    );
    if !mount_lines.is_empty() {
        out.push(Line::from(Span::styled("Mounts:", heading)));
        out.extend(mount_lines);
        out.push(Line::raw(""));
    }

    // ── Environment details ───────────────────────────────────────────
    let env_lines = settings_env_diff_lines(
        &settings.env.original,
        &settings.env.pending,
        add_style,
        remove_style,
    );
    if !env_lines.is_empty() {
        out.push(Line::from(Span::styled("Environments:", heading)));
        out.extend(env_lines);
        out.push(Line::raw(""));
    }

    // ── Auth details ──────────────────────────────────────────────────
    let auth_lines = settings_auth_diff_lines(
        &settings.auth.original,
        &settings.auth.pending,
        &settings.auth.original_github_env,
        &settings.auth.github_env,
        add_style,
        remove_style,
    );
    if !auth_lines.is_empty() {
        out.push(Line::from(Span::styled("Auth:", heading)));
        out.extend(auth_lines);
        out.push(Line::raw(""));
    }

    // ── Trust details ─────────────────────────────────────────────────
    let trust_lines = settings_trust_diff_lines(
        &settings.trust.original,
        &settings.trust.pending,
        add_style,
        remove_style,
    );
    if !trust_lines.is_empty() {
        out.push(Line::from(Span::styled("Trust:", heading)));
        out.extend(trust_lines);
        out.push(Line::raw(""));
    }

    // Strip trailing blank lines.
    while out.last().is_some_and(|l: &Line| {
        l.spans.is_empty() || l.spans.iter().all(|s| s.content.trim().is_empty())
    }) {
        out.pop();
    }

    out
}

// ── Summary stats helpers ─────────────────────────────────────────────────────

fn settings_mount_stats(
    original: &[crate::config::GlobalMountRow],
    pending: &[crate::config::GlobalMountRow],
) -> Option<String> {
    let (added, removed, modified) = mount_diff_counts(original, pending);
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

fn settings_env_stats(
    original: &crate::console::tui::state::SettingsEnvConfig,
    pending: &crate::console::tui::state::SettingsEnvConfig,
) -> Option<String> {
    let (added, removed, modified) = env_config_diff_counts(original, pending);
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

fn settings_general_stats(state: &crate::console::tui::state::SettingsGeneralState) -> Option<String> {
    let count = state.change_count();
    if count == 0 {
        return None;
    }
    Some(if count == 1 {
        "1 change".to_string()
    } else {
        format!("{count} changes")
    })
}

fn settings_auth_stats(
    original: &[crate::console::tui::state::SettingsAuthRow],
    pending: &[crate::console::tui::state::SettingsAuthRow],
    orig_github_env: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pend_github_env: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
) -> Option<String> {
    let row_changes = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a.mode != b.mode)
        .count();
    let (env_added, env_removed, env_modified) =
        env_map_diff_counts(orig_github_env, pend_github_env);
    let total = row_changes + env_added + env_removed + env_modified;
    if total == 0 {
        return None;
    }
    Some(format!("{total} changed"))
}

fn settings_trust_stats(
    original: &[crate::console::tui::state::SettingsTrustRow],
    pending: &[crate::console::tui::state::SettingsTrustRow],
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

// ── Count helpers ─────────────────────────────────────────────────────────────

fn mount_diff_counts(
    original: &[crate::config::GlobalMountRow],
    pending: &[crate::config::GlobalMountRow],
) -> (usize, usize, usize) {
    use std::collections::BTreeMap;
    let orig_map: BTreeMap<(Option<String>, String), &crate::config::GlobalMountRow> = original
        .iter()
        .map(|r| ((r.scope.clone(), r.name.clone()), r))
        .collect();
    let pend_map: BTreeMap<(Option<String>, String), &crate::config::GlobalMountRow> = pending
        .iter()
        .map(|r| ((r.scope.clone(), r.name.clone()), r))
        .collect();
    let added = pend_map
        .keys()
        .filter(|k| !orig_map.contains_key(k))
        .count();
    let removed = orig_map
        .keys()
        .filter(|k| !pend_map.contains_key(k))
        .count();
    let modified = pend_map
        .iter()
        .filter(|(k, prow)| orig_map.get(k).is_some_and(|orow| orow.mount != prow.mount))
        .count();
    (added, removed, modified)
}

fn env_config_diff_counts(
    original: &crate::console::tui::state::SettingsEnvConfig,
    pending: &crate::console::tui::state::SettingsEnvConfig,
) -> (usize, usize, usize) {
    let (ga, gr, gm) = env_map_diff_counts(&original.env, &pending.env);
    let all_roles: std::collections::BTreeSet<&String> =
        original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = std::collections::BTreeMap::default();
    let (ra, rr, rm) = all_roles.into_iter().fold((0, 0, 0), |(a, r, m), role| {
        let oe = original.roles.get(role).unwrap_or(&empty);
        let pe = pending.roles.get(role).unwrap_or(&empty);
        let (da, dr, dm) = env_map_diff_counts(oe, pe);
        (a + da, r + dr, m + dm)
    });
    (ga + ra, gr + rr, gm + rm)
}

fn env_map_diff_counts(
    original: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pending: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
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

// ── Detail diff-line helpers ──────────────────────────────────────────────────

fn settings_mount_diff_lines(
    original: &[crate::config::GlobalMountRow],
    pending: &[crate::config::GlobalMountRow],
    add_style: ratatui::style::Style,
    remove_style: ratatui::style::Style,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
    use std::collections::BTreeMap;

    let orig_map: BTreeMap<(Option<String>, String), &crate::config::GlobalMountRow> = original
        .iter()
        .map(|r| ((r.scope.clone(), r.name.clone()), r))
        .collect();
    let pend_map: BTreeMap<(Option<String>, String), &crate::config::GlobalMountRow> = pending
        .iter()
        .map(|r| ((r.scope.clone(), r.name.clone()), r))
        .collect();

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
            && orow.mount != prow.mount
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

fn mount_row_summary(row: &crate::config::GlobalMountRow) -> String {
    let scope = row
        .scope
        .as_deref()
        .map(|s| format!("[{s}] "))
        .unwrap_or_default();
    let src = crate::tui::shorten_home(&row.mount.src);
    let dst = crate::tui::shorten_home(&row.mount.dst);
    let ro = if row.mount.readonly { " (ro)" } else { "" };
    format!("{scope}{src} → {dst}{ro}")
}

fn settings_env_diff_lines(
    original: &crate::console::tui::state::SettingsEnvConfig,
    pending: &crate::console::tui::state::SettingsEnvConfig,
    add_style: ratatui::style::Style,
    remove_style: ratatui::style::Style,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
    let mut out: Vec<Line<'static>> = Vec::new();
    append_env_map_diff_lines(
        &mut out,
        None,
        &original.env,
        &pending.env,
        add_style,
        remove_style,
    );
    let all_roles: std::collections::BTreeSet<&String> =
        original.roles.keys().chain(pending.roles.keys()).collect();
    let empty = std::collections::BTreeMap::default();
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

fn settings_auth_diff_lines(
    original: &[crate::console::tui::state::SettingsAuthRow],
    pending: &[crate::console::tui::state::SettingsAuthRow],
    orig_github_env: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pend_github_env: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    add_style: ratatui::style::Style,
    remove_style: ratatui::style::Style,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
    let mut out: Vec<Line<'static>> = Vec::new();
    for (orig_row, pend_row) in original.iter().zip(pending.iter()) {
        if orig_row.mode != pend_row.mode {
            out.push(Line::from(Span::styled(
                format!(
                    "  ~ {}  {} \u{2192} {}",
                    pend_row.kind.label(),
                    orig_row.mode.as_str(),
                    pend_row.mode.as_str(),
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
    original: &[crate::console::tui::state::SettingsTrustRow],
    pending: &[crate::console::tui::state::SettingsTrustRow],
    add_style: ratatui::style::Style,
    remove_style: ratatui::style::Style,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
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
