// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Spawn environment, palette entry, terminal geometry, and capacity guards.

use super::super::{
    Dialog, MAX_SESSIONS, MAX_TABS, Multiplexer, PaletteCloseLabel, Result, SESSION_ENV_PASSTHROUGH,
};

impl Multiplexer {
    pub(crate) fn env_for_spawn(&self, overrides: &[(String, String)]) -> Vec<(String, String)> {
        let mut env = self.launch_env.env_passthrough.clone();
        for (key, value) in overrides {
            if !SESSION_ENV_PASSTHROUGH.iter().any(|allowed| allowed == key) {
                continue;
            }
            if let Some((_, existing)) =
                env.iter_mut().find(|(existing_key, _)| existing_key == key)
            {
                *existing = value.clone();
            } else {
                env.push((key.clone(), value.clone()));
            }
        }
        env
    }

    pub(crate) fn open_command_palette(&mut self) {
        let close_label = PaletteCloseLabel::for_pane_count(self.active_tab_pane_count());
        self.dialog_push(Dialog::new_command_palette(close_label));
    }

    /// Terminal geometry + identity for a new session's grid. The single
    /// construction point for `SessionTerminal` so both spawn paths (new tab,
    /// split) carry the attach client's reported colors.
    pub(crate) fn session_terminal(&self, rows: u16, cols: u16) -> crate::session::SessionTerminal {
        crate::session::SessionTerminal {
            rows,
            cols,
            row_arena: self.render.terminal_row_arena.clone(),
            default_fg: self.client_registry.attached_terminal.default_fg,
            default_bg: self.client_registry.attached_terminal.default_bg,
        }
    }

    /// Re-apply the attached client's terminal colors to every live grid.
    /// Called on (re)attach: a container can be reattached from a terminal
    /// with a different palette, and agents that query OSC 10/11 later must
    /// see the current client's colors. A client that could not read its
    /// palette reports `None`, which keeps each grid's previous colors —
    /// the last known answer beats resetting to the baked-in default.
    pub(crate) fn apply_client_colors_to_sessions(&mut self) {
        let fg = self.client_registry.attached_terminal.default_fg;
        let bg = self.client_registry.attached_terminal.default_bg;
        for session in self.session_supervisor.sessions.values_mut() {
            session.shadow_grid.set_reported_colors(fg, bg);
        }
    }

    /// Bound the per-container surface for any path that allocates a
    /// new PTY (top-level spawn, split, etc.). All such paths must
    /// route through here so `MAX_TABS` / `MAX_SESSIONS` are enforced
    /// uniformly — runaway-mis-click defence. `add_tab=true` enforces
    /// both caps; `add_tab=false` enforces only `MAX_SESSIONS` because
    /// the caller is reusing an existing tab.
    pub(crate) fn ensure_capacity_for_new_session(&self, add_tab: bool) -> Result<()> {
        if add_tab && self.session_supervisor.tabs.len() >= MAX_TABS {
            anyhow::bail!(crate::tui::view::tab_limit_failure_message(MAX_TABS));
        }
        if self.session_supervisor.sessions.len() >= MAX_SESSIONS {
            anyhow::bail!(crate::tui::view::pane_limit_failure_message(MAX_SESSIONS));
        }
        Ok(())
    }

    /// True when there are no sessions left.
    /// `sessions.is_empty()` covers the operator-explicitly-killed-all
    /// case; `all !alive` covers the natural-exit case (every agent /
    /// shell process closed its PTY).
    pub(crate) fn no_live_sessions(&self) -> bool {
        self.session_supervisor.sessions.is_empty()
    }
}
