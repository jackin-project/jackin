// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! PTY output processing and terminal mode queries.

use super::{OSC_EVIDENCE_MAX_CHARS, Session, osc8_uri_is_safe, parse_osc7};

/// Parse the xterm modifyOtherKeys level from a `CSI > 4 ; <n> m`
/// sequence's raw bytes. Returns the level only for that exact shape;
/// any other CSI returns `None`.
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
    /// True when the session's program has enabled any mouse protocol
    /// mode. Used by the daemon to decide whether selection gestures
    /// belong to jackin or to the pane. Actual PTY mouse forwarding
    /// also consults `mouse_protocol_mode()` so press-only programs
    /// do not receive motion events.
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

    /// True when the session enabled DEC private mode `?1004` (focus
    /// event reporting).
    #[must_use]
    pub fn focus_events_enabled(&self) -> bool {
        self.shadow_grid.focus_events()
    }

    /// True when the terminal is in the alternate screen.
    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        self.shadow_grid.alternate_screen()
    }

    /// True when the foreground program has bracketed-paste enabled.
    #[must_use]
    pub fn bracketed_paste(&self) -> bool {
        self.shadow_grid.bracketed_paste()
    }

    /// True when the foreground program has application-cursor-keys mode on.
    #[must_use]
    pub fn application_cursor(&self) -> bool {
        self.shadow_grid.application_cursor()
    }

    /// Feed PTY bytes into the grid and update activity timestamps.
    pub fn feed_pty(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.received_output = true;
        }
        jackin_diagnostics::incr_terminal_bytes_received(bytes.len() as u64);

        // Single batch feed — the grid's persistent vte parser handles
        // sequences split across PTY read boundaries internally.
        let was_alternate = self.shadow_grid.alternate_screen();
        let was_scrolled = self.scrollback_offset() != 0;
        let scrollback_before = self.shadow_grid.scrollback_len();
        self.shadow_grid.process(bytes);
        let is_alternate = self.shadow_grid.alternate_screen();
        if was_alternate && !is_alternate {
            self.clear_transient_keyboard_modes();
        }

        // The grid records semantic scroll operations, but the scroll-region
        // (DECSTBM) emission optimizer that would consume them is deferred (see
        // the Ratatui modernization roadmap). Clear each chunk so they cannot
        // grow unbounded on a long scroll-heavy session (retaining capacity to
        // avoid per-chunk reallocation); the optimizer will consume them at
        // frame compose when it lands.
        self.shadow_grid.clear_scroll_ops();

        self.apply_passthrough_policy();

        if was_scrolled {
            // Anchor the view to content: rows evicted into scrollback during
            // this feed grow the tail-relative offset by the same amount, so
            // the rows under the reader hold still while the agent streams
            // (D3). An ED3 during the feed already reset the grid's offset to
            // 0; the guard keeps the view live in that case. At scrollback
            // capacity the delta is 0 and the view slides — clamping at
            // `filled` (inside `set_scrollback`) is the existing contract.
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

        // PTY output updates recency evidence only. It never authors state
        // (the old flap bug: any byte flipped Idle→Working, and a blocked
        // dialog repaint flipped Blocked→Working). State comes from evidence
        // arbitration over the rule pack / OSC / authority / physics.
        self.last_output_at = std::time::Instant::now();

        // Status evidence has its own persistent framing because the grid
        // does not surface OSC 133 or typed OSC 9;4 events. Apply every complete
        // event in wire order; PTY packet boundaries carry no protocol meaning.
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
                    // Progress-active is never working-proof; arbitration
                    // treats the clear edge as a hint only.
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

    /// Drain the grid's typed `PassthroughEvent`s, apply the session's
    /// `OscPolicy`, retain title / cwd / icon, and queue forwardable
    /// bytes in `pending_passthrough`.
    ///
    /// OSC 7 (cwd) is parsed for the pane-title surface and then
    /// dropped: forwarding it would let the operator's outer terminal
    /// remember the container's path, breaking `Cmd+T new tab` on the
    /// host (host-state pollution, forbidden by CLAUDE.md "Never mutate
    /// the host machine silently"). OSC 8 hyperlinks are gated through
    /// `osc8_uri_is_safe` so a compromised agent cannot smuggle a
    /// `javascript:` or `file://` URI to the host terminal.
    pub(crate) fn apply_passthrough_policy(&mut self) {
        use termpane::PassthroughEvent;
        let events = self.shadow_grid.drain_passthrough();
        for event in events {
            match event {
                PassthroughEvent::TitleChanged(ref title) => {
                    self.title = Some(title.clone());
                    // Agent-status evidence: retain the title (capped — OSC
                    // content is untrusted model output). The rule pack's
                    // `osc_title` virtual region reads this.
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
                    // Plain OSC 9 desktop notification is forwarded to the host
                    // per policy. OSC 9;4 progress is decoded separately from the
                    // persistent status decoder — termpane does not surface it
                    // here.
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
                // BEL is deliberately absorbed: the grid never forwarded a
                // byte for it before the event became typed (it was
                // swallowed), and the capsule owns every byte that reaches
                // the outer terminal. Tests assert on the event instead.
                PassthroughEvent::Bell => {}
                // Device/mode query the emulator answered itself. The reply
                // goes back to the agent's own PTY stdin — never the outer
                // terminal — so the agent's capability detection reflects the
                // grid, not the host. (Root fix for the alt-screen corruption:
                // the host was answering DA/DSR/DECRQM with its own caps.)
                PassthroughEvent::Reply(bytes) => {
                    drop(self.input_tx.send(bytes));
                }
                // ScrollbackClear is a grid-internal instruction with no
                // outer-terminal byte form; the grid already cleared its
                // own scrollback in `erase_display`. Reset the view offset.
                PassthroughEvent::ScrollbackClear => {
                    self.reset_scrollback_view();
                }
                // Mode toggles (focus, application cursor, bracketed paste)
                // round-trip to the outer terminal verbatim. The agent's
                // `?2026` toggles are absorbed in the grid — the capsule's
                // own frame brackets supersede them.
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

    /// Forward an allowlisted CSI the grid passed through. Only the
    /// documented allowlist arrives here — kitty keyboard push/pop
    /// (`\x1b[>{n}u` / `\x1b[<{n}u`, tracked by the grid and re-asserted by
    /// the per-frame mode reconciliation) and xterm modifyOtherKeys
    /// (`\x1b[>4;{n}m`, tracked so alternate-screen exit can reset it).
    /// Everything else is default-denied in the grid (§3.6).
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

    /// Drain the OSC / unhandled-CSI byte sequences captured during the
    /// last `feed_pty` call. The daemon forwards these to the attached
    /// client only when this session owns the focused pane in the active
    /// tab — backgrounded panes' notifications, clipboard writes, and
    /// titles must not reach the operator's outer terminal.
    pub fn drain_passthrough(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending_passthrough)
    }

    #[must_use]
    pub fn allow_frame_hyperlinks(&self) -> bool {
        self.osc_policy.allow_hyperlink()
    }
}
