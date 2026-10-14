// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use ratatui::{buffer::Buffer, layout::Rect};
use termrock::style::RolePalette;

use super::png_baselines::{BaselineCase, baselines_dir, render_manager_buffer, stage_views};
use crate::tui::view::{modal_overlay_state_for_route, modal_overlay_visible};

const BLESS_ENV: &str = "JACKIN_BLESS_BRAND_PNGS";

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
    stage_views().into_iter().filter(is_non_modal).collect()
}

fn crop_dir() -> PathBuf {
    baselines_dir().join("brand")
}

fn crop_path(id: &str) -> PathBuf {
    crop_dir().join(format!("{id}.png"))
}

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

pub(super) fn export_review_crops(directory: &Path) {
    let target = directory.join("brand");
    fs::create_dir(&target).expect("new review crop directory");
    for case in non_modal_cases() {
        fs::write(target.join(format!("{}.png", case.id)), render_crop(&case))
            .expect("write candidate brand crop");
    }
}
