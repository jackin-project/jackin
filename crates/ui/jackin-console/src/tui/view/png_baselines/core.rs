// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `BaselineCase` rendering and base states.
#![cfg(test)]

use crate::tui::{
    state::ManagerState,
    view::{prepare_for_render, render},
};
use jackin_config::AppConfig;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};
use std::path::{Path, PathBuf};
use termrock::style::RolePalette;

/// One baselined screen: stable kebab-case id plus a headless constructor for
/// its canonical state.
pub(crate) struct BaselineCase {
    pub(crate) id: &'static str,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) build: fn() -> (ManagerState<'static>, AppConfig, PathBuf),
}

pub(crate) fn test_cwd() -> PathBuf {
    PathBuf::from("/workspace")
}

pub(crate) fn render_case(case: &BaselineCase) -> Vec<u8> {
    let (mut state, config, cwd) = (case.build)();
    let buffer = render_manager_buffer(&mut state, &config, &cwd, case.width, case.height);
    termrock_raster::render_png(&buffer, &RolePalette::default())
        .expect("baselined screen must rasterize")
}

pub(crate) fn render_manager_buffer(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &Path,
    width: u16,
    height: u16,
) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    prepare_for_render(state, config, cwd, area);
    terminal
        .draw(|frame| render(frame, area, state, config, cwd))
        .unwrap();
    terminal.backend().buffer().clone()
}

// ── Stage-view constructors ────────────────────────────────────────────────

pub(crate) fn plain() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let state = ManagerState::from_config(&config, &cwd);
    (state, config, cwd)
}

pub(crate) fn populated_config() -> AppConfig {
    toml::from_str(
        r#"
[roles."chainargos/agent-smith"]
git = "https://example.invalid/agent-smith.git"

[docker.mounts]
cache = { src = "/cache", dst = "/cache", readonly = false }

[workspaces.alpha]
workdir = "/workspace"
allowed_roles = ["chainargos/agent-smith"]

[[workspaces.alpha.mounts]]
src = "/workspace"
dst = "/workspace"
readonly = false

[workspaces.beta]
workdir = "/beta"
"#,
    )
    .expect("valid populated-list config")
}
