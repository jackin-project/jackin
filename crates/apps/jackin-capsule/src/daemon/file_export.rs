// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use jackin_protocol::attach::{
    FileExportChunk, FileExportEnd, FileExportStart, MAX_FILE_EXPORT_CHUNK_BYTES, ServerFrame,
};
use tokio::task::JoinHandle;

use super::Multiplexer;
use crate::client_writer::AttachmentIdentity;
use crate::tui::pane_snapshot::RowSnapshot;
use crate::tui::selection::{selection_text, word_bounds_in_row};

mod source;
use source::{PrepareError, PreparedExport};

const EXPORT_PREPARATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub(crate) struct FileExportState {
    pending: Option<PendingExport>,
}

#[derive(Debug)]
struct PendingExport {
    task: JoinHandle<std::result::Result<PreparedExport, PrepareError>>,
    attachment: AttachmentIdentity,
    session_id: Option<u64>,
    intent: ExportIntent,
    reveal_after_export: bool,
    open_after_export: bool,
    cancelled: Arc<AtomicBool>,
}

impl Drop for PendingExport {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Clone, Debug)]
enum ExportIntent {
    Explicit,
    ModifiedClick,
}

impl Multiplexer {
    pub(super) fn export_file_to_host(
        &mut self,
        requested_path: String,
        reveal_after_export: bool,
        open_after_export: bool,
        session_id: Option<u64>,
    ) {
        if !self.start_file_export(
            requested_path,
            reveal_after_export,
            open_after_export,
            ExportIntent::Explicit,
            session_id,
        ) {
            self.set_clipboard_image_notice(
                "File export rejected: no attached client or another export is in progress"
                    .to_owned(),
            );
        }
    }

    fn start_file_export(
        &mut self,
        requested_path: String,
        reveal_after_export: bool,
        open_after_export: bool,
        intent: ExportIntent,
        session_id: Option<u64>,
    ) -> bool {
        if self.file_exports.pending.is_some() {
            return false;
        }
        let Some(attachment) = self.client_registry.client.attachment_identity() else {
            return false;
        };
        let workdir = self.launch_env.workdir.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let deadline = Instant::now() + EXPORT_PREPARATION_TIMEOUT;
        let task = jackin_telemetry::spawn::joined_blocking(move || {
            source::prepare_export(&requested_path, &workdir, &worker_cancelled, deadline)
        });
        self.file_exports.pending = Some(PendingExport {
            task,
            attachment,
            session_id,
            intent,
            reveal_after_export,
            open_after_export,
            cancelled,
        });
        true
    }

    pub(super) async fn poll_file_export(&mut self) {
        let Some(pending) = self.file_exports.pending.as_mut() else {
            return;
        };
        let current = self
            .client_registry
            .client
            .matches_attachment(&pending.attachment)
            && pending
                .session_id
                .is_none_or(|id| self.session_supervisor.sessions.get(id).is_some());
        if !current {
            pending.cancelled.store(true, Ordering::Release);
        }
        if !pending.task.is_finished() {
            return;
        }
        let outcome = (&mut pending.task).await;
        let cancelled = pending.cancelled.load(Ordering::Acquire);
        let intent = pending.intent.clone();
        let reveal = pending.reveal_after_export;
        let open = pending.open_after_export;
        if current && !cancelled {
            match outcome {
                Ok(Ok(prepared)) => match self.emit_prepared_export(prepared, reveal, open) {
                    Ok(file_name) => {
                        let action = if open {
                            "File export and open queued"
                        } else if reveal {
                            "File export and reveal queued"
                        } else {
                            "File export queued"
                        };
                        self.set_clipboard_image_notice(format!("{action}: {file_name}"));
                    }
                    Err(error) => self.reject_file_export(error),
                },
                Ok(Err(PrepareError::CandidateRejected(error))) => match intent {
                    ExportIntent::Explicit => self.reject_file_export(error),
                    ExportIntent::ModifiedClick => {
                        jackin_telemetry::record_error(
                            jackin_telemetry::schema::enums::ErrorType::IoError,
                        )
                        .unwrap_or(());
                        // Selection candidates exclude mouse-reporting panes. Their
                        // original fallback is empty; never recompute it after
                        // focus, geometry, or protocol changes while awaiting I/O.
                    }
                },
                Ok(Err(PrepareError::ReadFailed(error))) => self.reject_file_export(error),
                Err(error) => self.reject_file_export(error.into()),
            }
        }
        drop(self.file_exports.pending.take());
    }

    fn reject_file_export(&mut self, error: anyhow::Error) {
        jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError)
            .unwrap_or(());
        self.set_clipboard_image_notice(format!("File export rejected: {error:#}"));
    }

    fn emit_prepared_export(
        &mut self,
        prepared: PreparedExport,
        reveal_after_export: bool,
        open_after_export: bool,
    ) -> Result<String> {
        let transfer_id = next_transfer_id();
        let size = u64::try_from(prepared.bytes.len()).context("export size overflow")?;
        self.send_protocol_frame(ServerFrame::FileExportStart(FileExportStart {
            transfer_id,
            source_path: prepared.source.display().to_string(),
            file_name: prepared.file_name.clone(),
            size,
            reveal_after_export,
            open_after_export,
        }));
        let mut offset = 0u64;
        for bytes in prepared.bytes.chunks(MAX_FILE_EXPORT_CHUNK_BYTES) {
            let next = offset
                .checked_add(u64::try_from(bytes.len()).context("export chunk length overflow")?)
                .context("export offset overflow")?;
            self.send_protocol_frame(ServerFrame::FileExportChunk(FileExportChunk {
                transfer_id,
                offset,
                bytes: bytes.to_vec(),
            }));
            offset = next;
        }
        self.send_protocol_frame(ServerFrame::FileExportEnd(FileExportEnd {
            transfer_id,
            sha256: prepared.sha256,
        }));
        Ok(prepared.file_name)
    }

    pub(super) fn export_file_under_cursor_to_host(
        &mut self,
        reveal_after_export: bool,
        open_after_export: bool,
    ) -> bool {
        let Some(requested_path) = self.export_path_under_cursor() else {
            return false;
        };
        self.export_file_to_host(
            requested_path,
            reveal_after_export,
            open_after_export,
            self.active_focused_id(),
        );
        true
    }

    pub(super) fn export_selected_file_to_host(
        &mut self,
        reveal_after_export: bool,
        open_after_export: bool,
    ) -> bool {
        let Some(selection) = self.clipboard.selection else {
            return false;
        };
        let Some(session) = self.session_supervisor.sessions.get(selection.session_id) else {
            return false;
        };
        let rows = session.render_content_snapshot(selection.inner.cols);
        let requested_path = selection_text(&rows, &selection).trim().to_owned();
        if requested_path.is_empty() {
            return false;
        }
        self.export_file_to_host(
            requested_path,
            reveal_after_export,
            open_after_export,
            Some(selection.session_id),
        );
        true
    }

    pub(super) fn export_visible_file_at(&mut self, row: u16, col: u16) -> bool {
        let Some((session_id, requested_path)) = self.export_path_at_mouse_cell(row, col) else {
            return false;
        };
        self.start_file_export(
            requested_path,
            false,
            false,
            ExportIntent::ModifiedClick,
            Some(session_id),
        )
    }

    fn export_path_under_cursor(&self) -> Option<String> {
        let session_id = self.active_focused_id()?;
        let inner = self.active_focused_inner_rect()?;
        let session = self.session_supervisor.sessions.get(session_id)?;
        if session.scrollback_offset() != 0 {
            return None;
        }
        let (cursor_row, cursor_col) = session.shadow_grid.cursor_position();
        let rows = session.render_content_snapshot(inner.cols);
        let row = rows.get(usize::from(cursor_row))?;
        word_token(row, cursor_col)
    }

    fn export_path_at_mouse_cell(&self, row: u16, col: u16) -> Option<(u64, String)> {
        let candidate = self.detect_selection_start(row, col)?;
        let session = self.session_supervisor.sessions.get(candidate.session_id)?;
        let rows = session.render_content_snapshot(candidate.inner.cols);
        let row = rows.get(candidate.anchor_row)?;
        word_token(row, candidate.anchor_col).map(|path| (candidate.session_id, path))
    }

    #[cfg(test)]
    pub(super) fn has_pending_file_export_for_test(&self) -> bool {
        self.file_exports.pending.is_some()
    }

    #[cfg(test)]
    pub(super) async fn finish_pending_file_export_for_test(&mut self) -> Result<()> {
        tokio::time::timeout(EXPORT_PREPARATION_TIMEOUT, async {
            while self.file_exports.pending.is_some() {
                self.poll_file_export().await;
                tokio::task::yield_now().await;
            }
        })
        .await
        .context("file export worker did not complete")
    }
}

fn word_token(row: &RowSnapshot, col: u16) -> Option<String> {
    let (start_col, end_col) = word_bounds_in_row(row, col)?;
    let token = row.text_range(start_col, end_col).trim().to_owned();
    (!token.is_empty()).then_some(token)
}

fn next_transfer_id() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_nanos().try_into().unwrap_or(duration.as_secs())
        })
}

#[cfg(test)]
mod tests;
