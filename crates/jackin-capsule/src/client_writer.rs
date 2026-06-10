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

    fn send_encoded(&mut self, bytes: Vec<u8>) {
        if let Some(tx) = &self.tx
            && tx.send(bytes).is_err()
            && !self.dead_logged
        {
            self.dead_logged = true;
            crate::clog!(
                "client write: receiver dropped; output discarded (this attach is dead)"
            );
        }
    }

    fn log_emission(&self, bytes: &[u8]) {
        if !crate::logging::debug_enabled() {
            return;
        }
        let (moves, max_row, max_col, erases) = scan_emitted_frame(bytes);
        crate::cdebug!(
            "send: bytes={} cursor_moves={} max_row_addressed={} max_col_addressed={} erases={}",
            bytes.len(),
            moves,
            max_row,
            max_col,
            erases,
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

/// Scan an emitted frame for the diagnostic fingerprint a render bug leaves:
/// how many absolute cursor moves it contains, the largest row/col it
/// addresses (1-based, from `CSI row;col H`), and how many full-screen erases
/// (`CSI 2 J`) it carries. A `max_row_addressed` greater than the terminal
/// rows is the signature of a geometry the capsule and the outer terminal
/// disagree on. The scan is over our own trusted output, so the few lines of
/// hand parsing are cheaper than a dependency.
pub(crate) fn scan_emitted_frame(bytes: &[u8]) -> (usize, u16, u16, usize) {
    let mut moves = 0usize;
    let mut erases = 0usize;
    let mut max_row = 0u16;
    let mut max_col = 0u16;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == 0x1b && bytes[i + 1] == b'[' {
            let params_start = i + 2;
            let mut j = params_start;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b';') {
                j += 1;
            }
            if j < bytes.len() {
                let final_byte = bytes[j];
                let params = &bytes[params_start..j];
                match final_byte {
                    b'H' | b'f' => {
                        moves += 1;
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
                    b'J' if params == b"2" => erases += 1,
                    _ => {}
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    (moves, max_row, max_col, erases)
}
