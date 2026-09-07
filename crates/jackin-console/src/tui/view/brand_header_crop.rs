// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `BrandHeader` PNG crop baselines (plan 007, spec `console-brand-header.md`):
//! for every NON-modal stage view in plan 005's inventory (modal states paint
//! the backdrop over row 0 and are excluded), the full-screen buffer is
//! rendered exactly as the full-screen harness renders it, row 0 is extracted
//! into a fresh one-row `Buffer`, and the crop is compared against a committed
//! baseline at zero tolerance.
//!
//! The crops are the brand look's parity gate, isolated from surrounding
//! chrome: a full-screen re-bless can never touch them. Bless mode is a
//! SEPARATE env var (`JACKIN_BLESS_BRAND_PNGS=1`) from the full-screen
//! `JACKIN_BLESS_PNGS`; the crop directory lives inside plan 005's baseline
//! root as a `brand/` subdirectory the full-screen bless cannot address (that
//! bless writes only the per-screen filenames it enumerates at the root).
//! Licensing mirrors plan 005: the repository-wide aggregate annotation in
//! `REUSE.toml` covers the PNGs, no dedicated entry.
//!
//! Compare mode (default) NEVER writes; a crop diff outside an intended brand
//! change is a parity break — STOP for operator review, never re-bless.

#![cfg(test)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use ratatui::{buffer::Buffer, layout::Rect};
use termrock::style::RolePalette;

use super::png_baselines::{BaselineCase, baselines_dir, inventory, render_manager_buffer};
use crate::tui::view::{modal_overlay_state_for_route, modal_overlay_visible};

/// The brand bless env var — deliberately distinct from the full-screen
/// `JACKIN_BLESS_PNGS` so a chrome re-bless can never rewrite a brand crop.
const BLESS_ENV: &str = "JACKIN_BLESS_BRAND_PNGS";

/// One non-modal stage view: brand visible on row 0, hence croppable.
fn is_non_modal(case: &BaselineCase) -> bool {
    let (state, _config, _cwd) = (case.build)();
    !modal_overlay_visible(modal_overlay_state_for_route(
        state.stage.route(),
        state.status_overlay.is_some(),
        state.list_modal.is_some(),
        state.stage.modal_facts(),
    ))
}

fn non_modal_cases() -> Vec<BaselineCase> {
    inventory().into_iter().filter(is_non_modal).collect()
}

fn crop_dir() -> PathBuf {
    baselines_dir().join("brand")
}

fn crop_path(id: &str) -> PathBuf {
    crop_dir().join(format!("{id}.png"))
}

/// Row 0 (full width) of the full-screen buffer, copied cell-for-cell into a
/// fresh one-row buffer.
fn extract_row0(buffer: &Buffer, width: u16) -> Buffer {
    let mut row = Buffer::empty(Rect::new(0, 0, width, 1));
    for x in 0..width {
        row[(x, 0)] = buffer[(x, 0)].clone();
    }
    row
}

fn render_crop(case: &BaselineCase) -> Vec<u8> {
    let (mut state, config, cwd) = (case.build)();
    let buffer = render_manager_buffer(&mut state, &config, &cwd, case.width, case.height);
    let row = extract_row0(&buffer, case.width);
    termrock_raster::render_png(&row, &RolePalette::default()).expect("brand crop must rasterize")
}

fn check_crop(case: &BaselineCase, bless: bool, rendered: &[u8]) -> Result<(), String> {
    let path = crop_path(case.id);
    if bless {
        fs::write(&path, rendered).map_err(|e| format!("{}: write failed: {e}", case.id))?;
        return Ok(());
    }
    match fs::read(&path) {
        Err(_) => Err(format!(
            "{}: no brand-crop baseline at {} — bless via `{BLESS_ENV}=1` (plan 007 only)",
            case.id,
            path.display()
        )),
        Ok(committed) => termrock_raster::compare_png_pixels(rendered, &committed)
            .map_err(|diff| format!("{}: {diff}", case.id)),
    }
}

#[cfg(test)]
mod tests;

/// Export unapproved brand candidates alongside full-screen review frames.
pub(super) fn export_review_crops(directory: &Path) {
    let target = directory.join("brand");
    fs::create_dir(&target).expect("new review crop directory");
    for case in non_modal_cases() {
        fs::write(target.join(format!("{}.png", case.id)), render_crop(&case))
            .expect("write candidate brand crop");
    }
}
