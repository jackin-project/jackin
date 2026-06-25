//! The only writer to the attach socket.
//!
//! Every byte that reaches the attached client flows through this type:
//! composed frames via [`ClientWriter::write_frame`] (wrapped in `?2026`
//! synchronized-output brackets so the outer terminal applies them
//! atomically), and out-of-band sequences — OSC passthrough, clipboard
//! writes, pointer shapes, window titles, mode prefaces — via
//! [`ClientWriter::enqueue_out_of_band`], which buffers them and flushes only
//! at frame boundaries, never mid-frame. Protocol frames (`Welcome`,
//! `Shutdown`) go through [`ClientWriter::send_frame`]. Nothing else may hold
//! the socket sender; the interleaving class of stale-cell defects is gone by
//! construction (invariants I2 and I3 of the capsule rendering plan).

use tokio::sync::mpsc;

use crate::protocol::attach::{ServerFrame, encode_server};

/// Begin synchronized update — the outer terminal buffers everything until
/// the matching end so the frame applies atomically. Terminals that do not
/// support mode 2026 ignore both markers by spec.
const SYNC_BEGIN: &[u8] = b"\x1b[?2026h";
const SYNC_END: &[u8] = b"\x1b[?2026l";

#[derive(Debug, Default)]
pub(crate) struct ClientWriter {
    tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    /// Latched true on the first failed send after `attach`: once the
    /// receiver drops mid-attach every subsequent send fails too, and one
    /// log line beats one per frame.
    dead_logged: bool,
    /// Sequences waiting for the next frame boundary.
    out_of_band: Vec<Vec<u8>>,
}

impl ClientWriter {
    /// Wire a freshly attached client. Clears the dead-send latch; queued
    /// out-of-band bytes from the previous client are dropped — they were
    /// addressed to a terminal that no longer exists.
    pub(crate) fn attach(&mut self, tx: mpsc::UnboundedSender<Vec<u8>>) {
        self.tx = Some(tx);
        self.dead_logged = false;
        self.out_of_band.clear();
    }

    /// Drop the sender, returning it so detach paths can send their final
    /// `Shutdown` on a writer-free channel.
    pub(crate) fn take(&mut self) -> Option<mpsc::UnboundedSender<Vec<u8>>> {
        self.out_of_band.clear();
        self.tx.take()
    }

    pub(crate) fn is_attached(&self) -> bool {
        self.tx.is_some()
    }

    pub(crate) fn mark_dead_logged(&mut self) {
        self.dead_logged = true;
    }

    pub(crate) fn has_out_of_band(&self) -> bool {
        !self.out_of_band.is_empty()
    }

    /// Queue bytes that are not cell content for the next frame boundary.
    pub(crate) fn enqueue_out_of_band(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        self.out_of_band.push(bytes);
    }

    /// Send a composed frame: queued out-of-band bytes first, then the frame
    /// wrapped in `?2026` brackets, all in one socket write so nothing can
    /// interleave. An empty frame degenerates to an out-of-band flush.
    pub(crate) fn write_frame(&mut self, frame: Vec<u8>) {
        if frame.is_empty() {
            self.flush_out_of_band();
            return;
        }
        let mut buf = Vec::with_capacity(
            self.out_of_band.iter().map(Vec::len).sum::<usize>()
                + SYNC_BEGIN.len()
                + frame.len()
                + SYNC_END.len(),
        );
        for oob in self.out_of_band.drain(..) {
            buf.extend_from_slice(&oob);
        }
        buf.extend_from_slice(SYNC_BEGIN);
        buf.extend_from_slice(&frame);
        buf.extend_from_slice(SYNC_END);
        self.log_emission(&buf);
        self.send_encoded(encode_server(ServerFrame::Output(buf)));
    }

    /// Flush queued out-of-band bytes without a frame.
    pub(crate) fn flush_out_of_band(&mut self) {
        if self.out_of_band.is_empty() {
            return;
        }
        let mut buf = Vec::new();
        for oob in self.out_of_band.drain(..) {
            buf.extend_from_slice(&oob);
        }
        self.log_emission(&buf);
        self.send_encoded(encode_server(ServerFrame::Output(buf)));
    }

    /// Send a non-terminal protocol frame. Pending out-of-band terminal bytes
    /// flush first so OSC/mode side effects keep their original ordering.
    pub(crate) fn send_protocol_frame(&mut self, frame: ServerFrame) {
        self.flush_out_of_band();
        self.send_encoded(encode_server(frame));
    }

    fn send_encoded(&mut self, bytes: Vec<u8>) {
        if let Some(tx) = &self.tx
            && tx.send(bytes).is_err()
            && !self.dead_logged
        {
            self.dead_logged = true;
            crate::clog!("client write: receiver dropped; output discarded (this attach is dead)");
        }
    }

    fn log_emission(&self, bytes: &[u8]) {
        if !crate::logging::debug_enabled() {
            return;
        }
        let metrics = scan_emitted_frame(bytes);
        crate::cdebug!(
            "send: bytes={} cursor_moves={} sgr_resets={} osc8_opens={} osc8_closes={} max_row_addressed={} max_col_addressed={} full_screen_erases={} painted_cells={} full_frame_repaint={}",
            metrics.bytes,
            metrics.cursor_moves,
            metrics.sgr_resets,
            metrics.osc8_opens,
            metrics.osc8_closes,
            metrics.max_row_addressed,
            metrics.max_col_addressed,
            metrics.full_screen_erases,
            metrics.painted_cells,
            metrics.full_frame_repaint,
        );
        // Verbatim dump of only the smallest emissions (chrome / out-of-band
        // only). Capped tight so a steady-state run can't balloon the log.
        if bytes.len() <= 1200 {
            crate::cdebug!("send-bytes: {}", escape_for_log(bytes));
        }
    }
}

/// Render a frame's bytes as a single readable line: ESC as `\e`, other
/// control bytes as `\xNN`, printable ASCII verbatim. Used only behind the
/// debug flag to dump small frames for triage.
fn escape_for_log(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        match b {
            0x1b => out.push_str("\\e"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out
}

/// Emitted-byte counters used to catch render regressions now that Ratatui's
/// diff is no longer forced into a full repaint every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct EmittedFrameMetrics {
    pub(crate) bytes: usize,
    pub(crate) cursor_moves: usize,
    pub(crate) sgr_resets: usize,
    pub(crate) osc8_opens: usize,
    pub(crate) osc8_closes: usize,
    pub(crate) max_row_addressed: u16,
    pub(crate) max_col_addressed: u16,
    pub(crate) full_screen_erases: usize,
    pub(crate) painted_cells: usize,
    pub(crate) full_frame_repaint: bool,
}

/// Scan an emitted frame for the diagnostic fingerprint a render bug leaves.
/// The scan is over our own trusted output, so the few lines of hand parsing
/// are cheaper than a dependency.
pub(crate) fn scan_emitted_frame(bytes: &[u8]) -> EmittedFrameMetrics {
    scan_emitted_frame_with_geometry(bytes, None)
}

pub(crate) fn scan_emitted_frame_with_geometry(
    bytes: &[u8],
    geometry: Option<(u16, u16)>,
) -> EmittedFrameMetrics {
    let mut metrics = EmittedFrameMetrics {
        bytes: bytes.len(),
        ..EmittedFrameMetrics::default()
    };
    let mut max_row = 0u16;
    let mut max_col = 0u16;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            let params_start = i + 2;
            let mut j = params_start;
            while j < bytes.len()
                && (bytes[j].is_ascii_digit()
                    || matches!(bytes[j], b';' | b':' | b'?' | b'>' | b'<'))
            {
                j += 1;
            }
            if j < bytes.len() {
                let final_byte = bytes[j];
                let params = &bytes[params_start..j];
                match final_byte {
                    b'H' | b'f' => {
                        metrics.cursor_moves += 1;
                        let mut parts = params.split(|&b| b == b';');
                        let row = parts
                            .next()
                            .and_then(|p| std::str::from_utf8(p).ok())
                            .and_then(|s| s.parse::<u16>().ok())
                            .unwrap_or(1);
                        let col = parts
                            .next()
                            .and_then(|p| std::str::from_utf8(p).ok())
                            .and_then(|s| s.parse::<u16>().ok())
                            .unwrap_or(1);
                        max_row = max_row.max(row);
                        max_col = max_col.max(col);
                    }
                    b'J' if params == b"2" => metrics.full_screen_erases += 1,
                    b'm' if params == b"0" => metrics.sgr_resets += 1,
                    _ => {}
                }
                i = j + 1;
                continue;
            }
        } else if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b']') {
            let payload_start = i + 2;
            let mut j = payload_start;
            while j < bytes.len() {
                if bytes[j] == 0x07 {
                    break;
                }
                if bytes[j] == 0x1b && bytes.get(j + 1) == Some(&b'\\') {
                    break;
                }
                j += 1;
            }
            if j < bytes.len() {
                let payload = &bytes[payload_start..j];
                if payload.starts_with(b"8;") {
                    if payload == b"8;;" {
                        metrics.osc8_closes += 1;
                    } else {
                        metrics.osc8_opens += 1;
                    }
                }
                i = if bytes[j] == 0x1b { j + 2 } else { j + 1 };
                continue;
            }
        } else if matches!(bytes[i], 0x20..=0x7e) {
            // Printable ASCII only: a cheap repaint-density proxy, not an exact
            // cell count. Multi-byte glyphs (CJK/emoji/box-drawing borders) are
            // not counted, so `full_frame_repaint` is a heuristic — it can read
            // false on a glyph-heavy full repaint.
            metrics.painted_cells += 1;
        }
        i += 1;
    }
    metrics.max_row_addressed = max_row;
    metrics.max_col_addressed = max_col;
    if let Some((rows, cols)) = geometry {
        let cells = usize::from(rows) * usize::from(cols);
        metrics.full_frame_repaint =
            cells > 0 && metrics.painted_cells >= cells.saturating_mul(4) / 5;
    }
    metrics
}
