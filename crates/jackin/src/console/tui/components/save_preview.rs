//! Save-confirm preview line builders.
//!
//! Input handlers decide when a save preview opens; this module owns the
//! Ratatui line composition for the preview dialogs.

#[cfg(test)]
use jackin_console::tui::auth_config::env_display_map;

#[cfg(test)]
mod tests;

/// Append `+ KEY = VALUE` / `- KEY` lines to `out` for the diff between
/// two env maps. `indent` (`None` or `Some("  ")`) controls per-role
/// sub-indent — workspace-level lines use two spaces to match existing
/// diff styling; per-role lines nest one extra level.
#[cfg(test)]
pub(crate) fn append_env_map_diff_lines(
    out: &mut Vec<ratatui::text::Line<'static>>,
    indent: Option<&str>,
    original: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pending: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    value: ratatui::style::Style,
    dim: ratatui::style::Style,
) {
    let original = env_display_map(original);
    let pending = env_display_map(pending);
    jackin_console::tui::components::save_preview::append_env_map_diff_lines(
        out, indent, &original, &pending, value, dim,
    );
}

pub(crate) fn collapse_section_lines(
    collapses: &[crate::workspace::Removal],
) -> Vec<ratatui::text::Line<'static>> {
    let display_pairs: Vec<_> = collapses
        .iter()
        .map(|r| {
            (
                crate::tui::shorten_home(&r.child.src),
                crate::tui::shorten_home(&r.covered_by.src),
            )
        })
        .collect();
    jackin_console::tui::components::save_preview::collapse_section_lines(&display_pairs)
}
