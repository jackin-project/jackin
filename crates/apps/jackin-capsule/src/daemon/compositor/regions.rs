// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Pane hyperlink and SGR region caching for hit-testing.

use std::collections::HashSet;

use crate::tui::socket_backend::SgrMetadata;

use super::{PaneRegionCache, PaneRegionCacheKey, PaneRegions};

pub(crate) fn cached_pane_regions(
    cache: &mut std::collections::HashMap<u64, PaneRegionCache>,
    panes: &[crate::tui::model::VisiblePane],
    pane_screens: &[(u64, crate::tui::view::PaneScreen<'_>)],
    sessions: &super::super::SessionRegistry,
    damaged_panes: &HashSet<u64>,
    focused_id: Option<u64>,
) -> PaneRegions {
    let visible: HashSet<u64> = panes.iter().map(|pane| pane.id).collect();
    cache.retain(|id, _| visible.contains(id));

    let mut hyperlinks = Vec::new();
    let mut sgr = Vec::new();
    for pane in panes {
        let Some(session) = sessions.get(pane.id) else {
            cache.remove(&pane.id);
            continue;
        };
        let key = PaneRegionCacheKey {
            inner: ratatui::layout::Rect {
                x: pane.inner.col,
                y: pane.inner.row,
                width: pane.inner.cols,
                height: pane.inner.rows,
            },
            scrollback_offset: session.scrollback_offset(),
            focused: focused_id == Some(pane.id),
            allow_hyperlinks: session.allow_frame_hyperlinks(),
        };
        let rebuild = damaged_panes.contains(&pane.id)
            || cache.get(&pane.id).is_none_or(|cached| cached.key != key);
        if rebuild {
            let pane_slice = std::slice::from_ref(pane);
            let hyperlinks_for_pane = pane_hyperlink_regions(pane_slice, pane_screens, sessions);
            let sgr_for_pane = pane_sgr_regions(pane_slice, pane_screens);
            cache.insert(
                pane.id,
                PaneRegionCache {
                    key,
                    hyperlinks: hyperlinks_for_pane,
                    sgr: sgr_for_pane,
                },
            );
        }
        if let Some(cached) = cache.get(&pane.id) {
            hyperlinks.extend(cached.hyperlinks.iter().cloned());
            sgr.extend(cached.sgr.iter().copied());
        }
    }
    (hyperlinks, sgr)
}

/// Run-length-encode per-row cell metadata into single-row `Rect`s. For each
/// allowed pane's visible `View`, groups horizontally-adjacent cells sharing
/// the same run value and emits one `Rect` per run (offset into the pane's
/// inner area). Shared by the hyperlink and SGR-metadata region builders so the
/// boundary arithmetic and clamping live in one place.
///
/// `probe` opens a run and produces its owned value once at the run's first
/// cell; `same_run` extends the run by comparing each later cell against that
/// value. They are split so the (allocating) owned form is built once per run
/// while extension stays allocation-free — the hyperlink path would otherwise
/// allocate a `String` per cell.
fn pane_cell_runs<T>(
    panes: &[crate::tui::model::VisiblePane],
    pane_screens: &[(u64, crate::tui::view::PaneScreen<'_>)],
    allow_pane: impl Fn(u64) -> bool,
    probe: impl Fn(&termpane::Cell) -> Option<T>,
    same_run: impl Fn(&termpane::Cell, &T) -> bool,
) -> Vec<(ratatui::layout::Rect, T)> {
    let mut regions = Vec::new();
    for pane in panes {
        if !allow_pane(pane.id) {
            continue;
        }
        let Some((_, crate::tui::view::PaneScreen::View(view))) =
            pane_screens.iter().find(|(id, _)| *id == pane.id)
        else {
            continue;
        };
        let max_rows = pane.inner.rows.min(view.rows);
        let max_cols = pane.inner.cols.min(view.cols);
        for row in 0..max_rows {
            let mut col = 0;
            while col < max_cols {
                let Some(value) = view.cell(row, col).and_then(&probe) else {
                    col += 1;
                    continue;
                };
                let start = col;
                col += 1;
                while col < max_cols
                    && view
                        .cell(row, col)
                        .is_some_and(|cell| same_run(cell, &value))
                {
                    col += 1;
                }
                regions.push((
                    ratatui::layout::Rect {
                        x: pane.inner.col + start,
                        y: pane.inner.row + row,
                        width: col - start,
                        height: 1,
                    },
                    value,
                ));
            }
        }
    }
    regions
}

/// The cell's hyperlink target if it carries one that passes the OSC 8 safety
/// filter. Borrows from the cell — no allocation — so run extension can compare
/// targets without owning a `String` per cell.
fn cell_safe_uri(cell: &termpane::Cell) -> Option<&str> {
    cell.hyperlink
        .as_ref()
        .map(|link| link.uri.as_str())
        .filter(|uri| crate::session::osc8_uri_is_safe(uri))
}

fn pane_hyperlink_regions(
    panes: &[crate::tui::model::VisiblePane],
    pane_screens: &[(u64, crate::tui::view::PaneScreen<'_>)],
    sessions: &super::super::SessionRegistry,
) -> Vec<(ratatui::layout::Rect, String)> {
    pane_cell_runs(
        panes,
        pane_screens,
        |id| {
            sessions
                .get(id)
                .is_some_and(crate::session::Session::allow_frame_hyperlinks)
        },
        |cell| cell_safe_uri(cell).map(str::to_owned),
        |cell, uri| cell_safe_uri(cell) == Some(uri.as_str()),
    )
}

pub(crate) fn pane_sgr_regions(
    panes: &[crate::tui::model::VisiblePane],
    pane_screens: &[(u64, crate::tui::view::PaneScreen<'_>)],
) -> Vec<(ratatui::layout::Rect, SgrMetadata)> {
    pane_cell_runs(
        panes,
        pane_screens,
        |_| true,
        |cell| {
            let metadata = cell_sgr_metadata(cell);
            (metadata != SgrMetadata::default()).then_some(metadata)
        },
        |cell, metadata| cell_sgr_metadata(cell) == *metadata,
    )
}

fn cell_sgr_metadata(cell: &termpane::Cell) -> SgrMetadata {
    SgrMetadata {
        underline_style: match cell.attrs.underline_style {
            termpane::UnderlineStyle::Single => termpane::UnderlineStyle::None,
            other => other,
        },
        underline_color: cell.attrs.underline_color,
        overline: cell.attrs.overline,
    }
}
