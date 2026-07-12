use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use jackin_core::container_paths;
use jackin_protocol::attach::{
    FileExportChunk, FileExportEnd, FileExportStart, MAX_FILE_EXPORT_CHUNK_BYTES,
    MAX_FILE_EXPORT_NAME_BYTES, MAX_FILE_EXPORT_PATH_BYTES, ServerFrame,
};
use sha2::{Digest, Sha256};

use super::Multiplexer;
use crate::tui::pane_snapshot::RowSnapshot;
use crate::tui::selection::{selection_text, word_bounds_in_row};

const JACKIN_RUN_DIR: &str = container_paths::RUN_DIR;
const MAX_EXPORT_FILE_BYTES: u64 = 64 * 1024 * 1024;

impl Multiplexer {
    pub(super) fn export_file_to_host(
        &mut self,
        requested_path: String,
        reveal_after_export: bool,
        open_after_export: bool,
    ) {
        match self.send_file_export_frames(&requested_path, reveal_after_export, open_after_export)
        {
            Ok(file_name) => {
                let action = if open_after_export {
                    "File export and open queued"
                } else if reveal_after_export {
                    "File export and reveal queued"
                } else {
                    "File export queued"
                };
                self.set_clipboard_image_notice(format!("{action}: {file_name}"));
            }
            Err(err) => {
                crate::clog!(
                    "file-export: rejected source_category={} reason={}",
                    requested_export_path_category(&requested_path),
                    compact_export_error_reason(&err)
                );
                self.set_clipboard_image_notice(format!("File export rejected: {err:#}"));
            }
        }
    }

    pub(super) fn export_file_under_cursor_to_host(
        &mut self,
        reveal_after_export: bool,
        open_after_export: bool,
    ) -> bool {
        let Some(requested_path) = self.export_path_under_cursor() else {
            return false;
        };
        self.export_file_to_host(requested_path, reveal_after_export, open_after_export);
        true
    }

    pub(super) fn export_selected_file_to_host(
        &mut self,
        reveal_after_export: bool,
        open_after_export: bool,
    ) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        let Some(session) = self.sessions.get(&selection.session_id) else {
            return false;
        };
        let rows = session.render_content_snapshot(selection.inner.cols);
        let requested_path = selection_text(&rows, &selection).trim().to_owned();
        if requested_path.is_empty() {
            return false;
        }
        self.export_file_to_host(requested_path, reveal_after_export, open_after_export);
        true
    }

    pub(super) fn export_visible_file_at(&mut self, row: u16, col: u16) -> bool {
        let Some(requested_path) = self.export_path_at_mouse_cell(row, col) else {
            return false;
        };
        if let Err(err) = self.resolve_export_candidate(&requested_path) {
            crate::cdebug!("file-export: modified-click ignored token={requested_path:?}: {err:#}");
            return false;
        }
        self.export_file_to_host(requested_path, false, false);
        true
    }

    fn export_path_under_cursor(&self) -> Option<String> {
        let session_id = self.active_focused_id()?;
        let inner = self.active_focused_inner_rect()?;
        let session = self.sessions.get(&session_id)?;
        if session.scrollback_offset() != 0 {
            return None;
        }
        let (cursor_row, cursor_col) = session.shadow_grid.cursor_position();
        let rows = session.render_content_snapshot(inner.cols);
        let row = rows.get(usize::from(cursor_row))?;
        word_token(row, cursor_col)
    }

    fn export_path_at_mouse_cell(&self, row: u16, col: u16) -> Option<String> {
        let candidate = self.detect_selection_start(row, col)?;
        let session = self.sessions.get(&candidate.session_id)?;
        let rows = session.render_content_snapshot(candidate.inner.cols);
        let row = rows.get(candidate.anchor_row)?;
        word_token(row, candidate.anchor_col)
    }

    fn send_file_export_frames(
        &mut self,
        requested_path: &str,
        reveal_after_export: bool,
        open_after_export: bool,
    ) -> Result<String> {
        let candidate = self.resolve_export_candidate(requested_path)?;
        let source = candidate.source;
        let metadata = candidate.metadata;
        let file_name = candidate.file_name;
        let canonical_workdir = candidate.canonical_workdir;
        #[expect(
            clippy::disallowed_methods,
            reason = "file export is an explicit bounded operator action, not render emission"
        )]
        let mut file =
            File::open(&source).with_context(|| format!("opening {}", source.display()))?;
        let transfer_id = next_transfer_id();
        self.send_protocol_frame(ServerFrame::FileExportStart(FileExportStart {
            transfer_id,
            source_path: source.display().to_string(),
            file_name: file_name.clone(),
            size: metadata.len(),
            reveal_after_export,
            open_after_export,
        }));
        let mut offset = 0u64;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; MAX_FILE_EXPORT_CHUNK_BYTES];
        loop {
            let n = file
                .read(&mut buffer)
                .with_context(|| format!("reading {}", source.display()))?;
            if n == 0 {
                break;
            }
            let bytes = buffer[..n].to_vec();
            hasher.update(&bytes);
            self.send_protocol_frame(ServerFrame::FileExportChunk(FileExportChunk {
                transfer_id,
                offset,
                bytes,
            }));
            offset = offset
                .checked_add(u64::try_from(n).context("export chunk length overflow")?)
                .ok_or_else(|| anyhow::anyhow!("export offset overflow"))?;
        }
        let sha256: [u8; 32] = hasher.finalize().into();
        self.send_protocol_frame(ServerFrame::FileExportEnd(FileExportEnd {
            transfer_id,
            sha256,
        }));
        let source_category = export_source_category(&source, &canonical_workdir);
        crate::cdebug!(
            "file-export: queued transfer_id={} source_category={} basename={:?} bytes={} sha256={} reveal_after_export={} open_after_export={}",
            transfer_id,
            source_category,
            file_name,
            metadata.len(),
            hex::encode(sha256),
            reveal_after_export,
            open_after_export
        );
        crate::clog!(
            "{}",
            file_export_queue_compact_line(
                source_category,
                &file_name,
                metadata.len(),
                reveal_after_export,
                open_after_export
            )
        );
        Ok(file_name)
    }

    fn resolve_export_candidate(&self, requested_path: &str) -> Result<ExportCandidate> {
        let (source, canonical_workdir) = self.resolve_export_source(requested_path)?;
        let metadata = source
            .metadata()
            .with_context(|| format!("reading metadata for {}", source.display()))?;
        if !metadata.is_file() {
            bail!("only regular files can be exported");
        }
        if metadata.len() > MAX_EXPORT_FILE_BYTES {
            bail!(
                "file is {} bytes; current export cap is {MAX_EXPORT_FILE_BYTES} bytes",
                metadata.len()
            );
        }
        let file_name = export_file_name(&source)?;
        if source.display().to_string().len() > MAX_FILE_EXPORT_PATH_BYTES {
            bail!("resolved path exceeds export protocol cap");
        }
        if file_name.len() > MAX_FILE_EXPORT_NAME_BYTES {
            bail!("file name exceeds export protocol cap");
        }
        Ok(ExportCandidate {
            source,
            metadata,
            file_name,
            canonical_workdir,
        })
    }

    /// Returns the canonicalized export source alongside the canonicalized
    /// workdir it was validated against, so callers reuse the single workdir
    /// `canonicalize()` rather than re-stat it.
    fn resolve_export_source(&self, requested_path: &str) -> Result<(PathBuf, PathBuf)> {
        let trimmed = requested_path.trim();
        if trimmed.is_empty() {
            bail!("path is empty");
        }
        let raw = Path::new(trimmed);
        let candidate = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.workdir.join(raw)
        };
        let source = candidate
            .canonicalize()
            .with_context(|| format!("resolving {}", candidate.display()))?;
        let workdir = self
            .workdir
            .canonicalize()
            .with_context(|| format!("resolving workdir {}", self.workdir.display()))?;
        let jackin_run = Path::new(JACKIN_RUN_DIR);
        if source.starts_with(&workdir) || source.starts_with(jackin_run) {
            return Ok((source, workdir));
        }
        bail!("path must be inside the workspace or {JACKIN_RUN_DIR}");
    }
}

fn export_source_category(source: &Path, canonical_workdir: &Path) -> &'static str {
    if source.starts_with(canonical_workdir) {
        return "workspace";
    }
    if source.starts_with(Path::new(JACKIN_RUN_DIR)) {
        return "jackin-run";
    }
    "unknown"
}

struct ExportCandidate {
    source: PathBuf,
    metadata: Metadata,
    file_name: String,
    canonical_workdir: PathBuf,
}

/// Extract the trimmed word token straddling `col` in `row`, or `None` when no
/// word covers the cell or the token is blank. Shared tail of the cursor-cell
/// and mouse-cell export-path probes.
fn word_token(row: &RowSnapshot, col: u16) -> Option<String> {
    let (start_col, end_col) = word_bounds_in_row(row, col)?;
    let token = row.text_range(start_col, end_col).trim().to_owned();
    (!token.is_empty()).then_some(token)
}

fn export_file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("file has no UTF-8 file name"))
}

fn next_transfer_id() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_nanos().try_into().unwrap_or(duration.as_secs())
        })
}

fn file_export_queue_compact_line(
    source_category: &str,
    file_name: &str,
    bytes: u64,
    reveal_after_export: bool,
    open_after_export: bool,
) -> String {
    format!(
        "file-export: queued source_category={source_category} basename={file_name:?} bytes={bytes} reveal_after_export={reveal_after_export} open_after_export={open_after_export}"
    )
}

/// Categorize an export path for diagnostics (INV-D20).
///
/// Returns one of: `jackin-run`, `jackin-owned`, `container-absolute`,
/// `container-relative`.
pub(crate) fn requested_export_path_category(requested_path: &str) -> &'static str {
    let trimmed = requested_path.trim();
    if container_paths::is_run_owned(trimmed) {
        return "jackin-run";
    }
    if container_paths::is_jackin_owned(trimmed) {
        return "jackin-owned";
    }
    if trimmed.starts_with('/') {
        return "container-absolute";
    }
    "container-relative"
}

fn compact_export_error_reason(err: &anyhow::Error) -> &'static str {
    let text = err.to_string();
    if text.contains("only regular files") {
        "non-regular"
    } else if text.contains("current export cap") {
        "oversize"
    } else if text.contains("workspace or /jackin/run") {
        "path-policy"
    } else if text.contains("exceeds export protocol cap") {
        "protocol-cap"
    } else if text.contains("empty") {
        "empty-path"
    } else if text.contains("resolving") {
        "not-found"
    } else {
        "validation"
    }
}

#[cfg(test)]
#[path = "file_export/tests.rs"]
mod export_category_tests;
