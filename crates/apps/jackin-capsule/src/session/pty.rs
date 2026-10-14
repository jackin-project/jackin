// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{OSC_EVIDENCE_MAX_CHARS, Session, osc8_uri_is_safe, parse_osc7};

pub(crate) fn parse_modify_other_keys(raw: &[u8]) -> Option<u16> {
    let body = raw.strip_prefix(b"\x1b[")?.strip_suffix(b"m")?;
    let body = body.strip_prefix(b">")?;
    let mut parts = body.split(|&b| b == b';');
    let first = parts.next()?;
    if first != b"4" {
        return None;
    }
    let level = parts.next().unwrap_or(b"0");
    std::str::from_utf8(level).ok()?.parse::<u16>().ok()
}

impl Session {
    #[must_use]
    pub fn mouse_enabled(&self) -> bool {
        !matches!(
            self.shadow_grid.mouse_protocol_mode(),
            termpane::MouseProtocolMode::None
        )
    }

    #[must_use]
    pub fn mouse_protocol_encoding(&self) -> termpane::MouseProtocolEncoding {
        self.shadow_grid.mouse_protocol_encoding()
    }

    #[must_use]
    pub fn mouse_protocol_mode(&self) -> termpane::MouseProtocolMode {
        self.shadow_grid.mouse_protocol_mode()
    }

    #[must_use]
    pub fn focus_events_enabled(&self) -> bool {
        self.shadow_grid.focus_events()
    }

    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        self.shadow_grid.alternate_screen()
    }

    #[must_use]
    pub fn bracketed_paste(&self) -> bool {
        self.shadow_grid.bracketed_paste()
    }

    #[must_use]
    pub fn application_cursor(&self) -> bool {
        self.shadow_grid.application_cursor()
    }

    pub fn feed_pty(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.received_output = true;
        }
        jackin_diagnostics::incr_terminal_bytes_received(bytes.len() as u64);

        let was_alternate = self.shadow_grid.alternate_screen();
        let was_scrolled = self.scrollback_offset() != 0;
        let scrollback_before = self.shadow_grid.scrollback_len();
        self.shadow_grid.process(bytes);
        let is_alternate = self.shadow_grid.alternate_screen();
        if was_alternate && !is_alternate {
            self.clear_transient_keyboard_modes();
        }

        self.shadow_grid.clear_scroll_ops();

        self.apply_passthrough_policy();

        if was_scrolled {
            let delta = self
                .shadow_grid
                .scrollback_len()
                .saturating_sub(scrollback_before);
            let current = self.scrollback_offset();
            if current != 0 && delta != 0 {
                self.shadow_grid
                    .set_scrollback(current.saturating_add(delta));
            }
        } else {
            self.scroll_to_live();
        }

        self.last_output_at = std::time::Instant::now();

        for event in self.osc_status_decoder.feed(bytes) {
            use crate::agent_status::evidence::RawAgentState;
            use crate::agent_status::{OscShellMark, OscStatusEvent};
            match event {
                OscStatusEvent::Shell(mark) => {
                    let shell_state = match mark {
                        OscShellMark::PreExec => Some(RawAgentState::Working),
                        OscShellMark::PromptEnd | OscShellMark::CommandFinished { .. } => {
                            Some(RawAgentState::Idle)
                        }
                        OscShellMark::PromptStart => None,
                    };
                    if let Some(state) = shell_state {
                        self.osc.shell_state_marked_at = Some(std::time::Instant::now());
                        self.osc.shell_state = Some(state);
                    }
                }
                OscStatusEvent::Progress(state) => {
                    self.osc.progress_raw = Some(format!("4;{state}"));
                    if state == 0 {
                        self.osc.progress_active = false;
                        self.osc.progress_cleared_at = Some(std::time::Instant::now());
                    } else {
                        self.osc.progress_active = true;
                    }
                }
            }
        }
    }

    pub(crate) fn apply_passthrough_policy(&mut self) {
        use termpane::PassthroughEvent;
        let events = self.shadow_grid.drain_passthrough();
        for event in events {
            match event {
                PassthroughEvent::TitleChanged(ref title) => {
                    self.title = Some(title.clone());

                    let capped: String = title.chars().take(OSC_EVIDENCE_MAX_CHARS).collect();
                    self.osc.title = Some(capped);
                    if self.osc_policy.allow_title()
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::IconNameChanged(ref name) => {
                    self.icon_name = Some(name.clone());
                    if self.osc_policy.allow_title()
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::CwdChanged(uri) => {
                    if let Some(path) = parse_osc7(&uri) {
                        self.cwd = Some(path);
                    }
                }
                PassthroughEvent::ClipboardWrite(_) => {
                    if self.osc_policy.allow_osc52()
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::Notification(_) => {
                    if self.osc_policy.allow_notify()
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::Hyperlink { ref uri, .. } => {
                    if self.osc_policy.allow_hyperlink()
                        && osc8_uri_is_safe(uri)
                        && let Some(bytes) = event.encode()
                    {
                        self.pending_passthrough.push(bytes);
                    }
                }
                PassthroughEvent::UnhandledCsi(ref raw) => {
                    self.handle_unhandled_csi(raw);
                }
                PassthroughEvent::DroppedCsi(_) => {}

                PassthroughEvent::Bell => {}

                PassthroughEvent::Reply(bytes) => {
                    drop(self.input_tx.send(bytes));
                }

                PassthroughEvent::ScrollbackClear => {
                    self.reset_scrollback_view();
                }

                PassthroughEvent::FocusEvents(_)
                | PassthroughEvent::ApplicationCursorKeys(_)
                | PassthroughEvent::BracketedPaste(_) => {
                    if let Some(bytes) = event.encode() {
                        self.pending_passthrough.push(bytes);
                    }
                }
            }
        }
    }

    pub(crate) fn handle_unhandled_csi(&mut self, raw: &[u8]) {
        if let Some(level) = parse_modify_other_keys(raw) {
            self.modify_other_keys = (level != 0).then_some(level);
        }
        self.pending_passthrough.push(raw.to_vec());
    }

    pub(crate) fn clear_transient_keyboard_modes(&mut self) {
        if self.shadow_grid.kitty_kb_flags() != 0 {
            self.shadow_grid.clear_kitty_kb_stack();
            self.pending_passthrough.push(b"\x1b[<u".to_vec());
        }
        if self.modify_other_keys.take().is_some() {
            self.pending_passthrough.push(b"\x1b[>4;0m".to_vec());
        }
    }

    pub fn drain_passthrough(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending_passthrough)
    }

    #[must_use]
    pub fn allow_frame_hyperlinks(&self) -> bool {
        self.osc_policy.allow_hyperlink()
    }
}
