// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::protocol::attach::{ServerFrame, encode_server};

const SYNC_BEGIN: &[u8] = b"\x1b[?2026h";
const SYNC_END: &[u8] = b"\x1b[?2026l";

#[derive(Clone, Debug)]
pub(crate) struct AttachmentIdentity(Arc<()>);

#[derive(Debug, Default)]
pub(crate) struct ClientWriter {
    tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    attachment: Option<AttachmentIdentity>,
    completion_tx: Option<mpsc::UnboundedSender<crate::attach_protocol::AttachResponseCompletion>>,
    out_of_band: Vec<Vec<u8>>,
}

impl ClientWriter {
    pub(crate) fn attach(&mut self, tx: mpsc::UnboundedSender<Vec<u8>>) {
        self.tx = Some(tx);
        self.attachment = Some(AttachmentIdentity(Arc::new(())));
        self.completion_tx = None;
        self.out_of_band.clear();
    }

    pub(crate) fn attach_with_completions(
        &mut self,
        tx: mpsc::UnboundedSender<Vec<u8>>,
        completion_tx: mpsc::UnboundedSender<crate::attach_protocol::AttachResponseCompletion>,
    ) {
        self.attach(tx);
        self.completion_tx = Some(completion_tx);
    }

    pub(crate) fn take(&mut self) -> Option<mpsc::UnboundedSender<Vec<u8>>> {
        self.attachment = None;
        self.out_of_band.clear();
        self.completion_tx = None;
        self.tx.take()
    }

    pub(crate) fn attachment_identity(&self) -> Option<AttachmentIdentity> {
        if self
            .tx
            .as_ref()
            .is_none_or(mpsc::UnboundedSender::is_closed)
        {
            return None;
        }
        self.attachment.clone()
    }

    pub(crate) fn matches_attachment(&self, identity: &AttachmentIdentity) -> bool {
        self.attachment
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.0, &identity.0))
            && self.tx.as_ref().is_some_and(|tx| !tx.is_closed())
    }

    pub(crate) fn is_attached(&self) -> bool {
        self.tx.is_some()
    }

    pub(crate) fn has_out_of_band(&self) -> bool {
        !self.out_of_band.is_empty()
    }

    pub(crate) fn enqueue_out_of_band(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        self.out_of_band.push(bytes);
    }

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
        Self::log_emission(&buf);
        self.send_encoded(encode_server(ServerFrame::Output(buf)));
    }

    pub(crate) fn flush_out_of_band(&mut self) {
        if self.out_of_band.is_empty() {
            return;
        }
        let mut buf = Vec::new();
        for oob in self.out_of_band.drain(..) {
            buf.extend_from_slice(&oob);
        }
        Self::log_emission(&buf);
        self.send_encoded(encode_server(ServerFrame::Output(buf)));
    }

    pub(crate) fn send_protocol_frame(&mut self, frame: ServerFrame) {
        self.flush_out_of_band();
        self.send_encoded(encode_server(frame));
    }

    pub(crate) fn send_attach_response(
        &mut self,
        response: jackin_protocol::attach::AttachControlResponse,
        completion: crate::attach_protocol::AttachResponseCompletion,
    ) {
        self.flush_out_of_band();
        let bytes = encode_server(ServerFrame::AttachControlResponse(response));
        let Some(tx) = &self.tx else {
            completion.complete_delivery_failure();
            return;
        };
        let Some(completion_tx) = &self.completion_tx else {
            completion.complete_delivery_failure();
            return;
        };
        if tx.send(bytes).is_err() {
            completion.complete_delivery_failure();
            return;
        }
        if let Err(error) = completion_tx.send(completion) {
            error.0.complete_delivery_failure();
        }
    }

    fn send_encoded(&mut self, bytes: Vec<u8>) {
        if let Some(tx) = &self.tx {
            drop(tx.send(bytes));
        }
    }

    fn log_emission(bytes: &[u8]) {
        let metrics = scan_emitted_frame(bytes);
        jackin_diagnostics::record_frame(
            metrics.bytes as u64,
            metrics.cursor_moves as u64,
            metrics.painted_cells as u64,
        );
    }
}

#[cfg(test)]
mod tests;

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

pub(crate) fn scan_emitted_frame(bytes: &[u8]) -> EmittedFrameMetrics {
    scan_emitted_frame_with_geometry(bytes, None)
}

#[expect(
    clippy::excessive_nesting,
    reason = "OSC 8 hyperlink + cursor + sync geometry splitter walker: nested \
              byte-slice slicing + equality check + payload inspection for the \
              full OSC 8 + DECSUBSR + DECSCUSR + ?2026 protocol. The nesting is \
              the bytewise state-machine."
)]
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
