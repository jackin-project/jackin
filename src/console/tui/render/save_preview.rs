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
pub(crate) fn build_settings_save_lines(
    settings: &crate::console::tui::state::SettingsState<'_>,
) -> Vec<ratatui::text::Line<'static>> {
    jackin_console::tui::components::save_preview::settings_save_lines(
        &settings_save_preview(settings),
    )
}

fn settings_save_preview(
    settings: &crate::console::tui::state::SettingsState<'_>,
) -> jackin_console::tui::components::save_preview::SettingsSavePreview {
    use jackin_console::tui::components::save_preview::{
        AuthPreviewRow, SettingsGeneralPreview, SettingsSavePreview, TrustPreviewRow,
    };

    SettingsSavePreview {
        general: SettingsGeneralPreview {
            original_coauthor_trailer: settings.general.original_coauthor_trailer,
            pending_coauthor_trailer: settings.general.pending_coauthor_trailer,
            original_dco: settings.general.original_dco,
            pending_dco: settings.general.pending_dco,
        },
        mounts_original: settings
            .mounts
            .original
            .iter()
            .map(global_mount_preview_row)
            .collect(),
        mounts_pending: settings
            .mounts
            .pending
            .iter()
            .map(global_mount_preview_row)
            .collect(),
        env_original: settings_env_preview(&settings.env.original),
        env_pending: settings_env_preview(&settings.env.pending),
        auth_original: settings
            .auth
            .original
            .iter()
            .map(|row| AuthPreviewRow {
                label: row.kind.label().to_string(),
                mode: row.mode.as_str().to_string(),
            })
            .collect(),
        auth_pending: settings
            .auth
            .pending
            .iter()
            .map(|row| AuthPreviewRow {
                label: row.kind.label().to_string(),
                mode: row.mode.as_str().to_string(),
            })
            .collect(),
        auth_github_env_original: env_display_map(&settings.auth.original_github_env),
        auth_github_env_pending: env_display_map(&settings.auth.github_env),
        trust_original: settings
            .trust
            .original
            .iter()
            .map(|row| TrustPreviewRow {
                role: row.role.clone(),
                trusted: row.trusted,
            })
            .collect(),
        trust_pending: settings
            .trust
            .pending
            .iter()
            .map(|row| TrustPreviewRow {
                role: row.role.clone(),
                trusted: row.trusted,
            })
            .collect(),
    }
}

fn global_mount_preview_row(
    row: &crate::config::GlobalMountRow,
) -> jackin_console::tui::components::save_preview::MountPreviewRow {
    jackin_console::tui::components::save_preview::MountPreviewRow {
        scope: row.scope.clone(),
        name: row.name.clone(),
        src: crate::tui::shorten_home(&row.mount.src),
        dst: crate::tui::shorten_home(&row.mount.dst),
        readonly: row.mount.readonly,
    }
}

fn settings_env_preview(
    config: &crate::console::tui::state::SettingsEnvConfig,
) -> jackin_console::tui::components::save_preview::SettingsEnvPreview {
    jackin_console::tui::components::save_preview::SettingsEnvPreview {
        env: env_display_map(&config.env),
        roles: config
            .roles
            .iter()
            .map(|(role, env)| (role.clone(), env_display_map(env)))
            .collect(),
    }
}

fn env_display_map(
    values: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
) -> std::collections::BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.clone(), value.as_display_str().to_string()))
        .collect()
}
