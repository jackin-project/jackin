use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use jackin_protocol::attach::{
    FileExportChunk, FileExportEnd, FileExportStart, MAX_FILE_EXPORT_CHUNK_BYTES,
    MAX_FILE_EXPORT_NAME_BYTES, MAX_FILE_EXPORT_PATH_BYTES, ServerFrame,
};
use sha2::{Digest, Sha256};

use super::Multiplexer;
use crate::tui::render::RowSnapshot;
use crate::tui::selection::{selection_text, word_bounds_in_row};

const JACKIN_RUN_DIR: &str = "/jackin/run";
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

fn requested_export_path_category(requested_path: &str) -> &'static str {
    let trimmed = requested_path.trim();
    if trimmed.starts_with("/jackin/run/") || trimmed == "/jackin/run" {
        return "jackin-run";
    }
    if trimmed.starts_with("/jackin/") || trimmed == "/jackin" {
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
mod tests {
    use std::collections::BTreeMap;

    use jackin_protocol::{
        CapsuleConfig,
        attach::{ServerFrame, read_server_frame},
    };
    use tokio::sync::mpsc;

    use super::*;

    fn test_mux(workdir: &Path) -> Multiplexer {
        let mut mux = Multiplexer::new(
            24,
            80,
            CapsuleConfig {
                role: "test-role".to_owned(),
                workdir: workdir.display().to_string(),
                agents: Vec::new(),
                models: BTreeMap::new(),
                provider_models: BTreeMap::new(),
                initial_provider: None,
                claude_marketplaces: Vec::new(),
                claude_plugins: Vec::new(),
                dirty_exit_policy: None,
                isolated_worktrees: Vec::new(),
            },
        )
        .expect("test multiplexer");
        mux.workdir = workdir.to_path_buf();
        mux
    }

    fn attach_export_receiver(mux: &mut Multiplexer) -> mpsc::UnboundedReceiver<Vec<u8>> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        mux.client.attach(tx);
        mux.client.flush_out_of_band();
        while rx.try_recv().is_ok() {}
        rx
    }

    async fn decode_server_frames(bytes: Vec<u8>) -> Vec<ServerFrame> {
        let mut frames = Vec::new();
        let mut stream = bytes.as_slice();
        while !stream.is_empty() {
            let mut tag = [0u8; 1];
            tokio::io::AsyncReadExt::read_exact(&mut stream, &mut tag)
                .await
                .expect("read frame tag");
            let frame = read_server_frame(&mut stream, tag[0])
                .await
                .expect("decode server frame")
                .expect("server frame");
            frames.push(frame);
        }
        frames
    }

    #[tokio::test]
    async fn send_file_export_frames_streams_regular_workspace_file() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        let path = workdir.join("report.txt");
        std::fs::write(&path, b"hello export").unwrap();
        let mut mux = test_mux(&workdir);
        let mut rx = attach_export_receiver(&mut mux);

        let file_name = mux
            .send_file_export_frames("report.txt", false, false)
            .expect("regular file should export");
        mux.client.flush_out_of_band();

        assert_eq!(file_name, "report.txt");
        let mut bytes = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            bytes.extend(chunk);
        }
        let frames = decode_server_frames(bytes).await;
        assert_eq!(frames.len(), 3);
        let start = frames[0].clone();
        let chunk = frames[1].clone();
        let end = frames[2].clone();
        let ServerFrame::FileExportStart(start) = start else {
            panic!("expected export start");
        };
        assert_eq!(start.file_name, "report.txt");
        assert_eq!(start.size, 12);
        assert!(!start.reveal_after_export);
        assert!(!start.open_after_export);
        let ServerFrame::FileExportChunk(chunk) = chunk else {
            panic!("expected export chunk");
        };
        assert_eq!(chunk.transfer_id, start.transfer_id);
        assert_eq!(chunk.offset, 0);
        assert_eq!(chunk.bytes, b"hello export");
        let ServerFrame::FileExportEnd(end) = end else {
            panic!("expected export end");
        };
        assert_eq!(end.transfer_id, start.transfer_id);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn send_file_export_frames_carries_reveal_request() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();
        let mut mux = test_mux(&workdir);
        let mut rx = attach_export_receiver(&mut mux);

        mux.send_file_export_frames("report.txt", true, false)
            .expect("regular file should export");
        mux.client.flush_out_of_band();

        let mut bytes = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            bytes.extend(chunk);
        }
        let frames = decode_server_frames(bytes).await;
        let ServerFrame::FileExportStart(start) = frames[0].clone() else {
            panic!("expected export start");
        };
        assert!(start.reveal_after_export);
        assert!(!start.open_after_export);
    }

    #[tokio::test]
    async fn send_file_export_frames_carries_open_request() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();
        let mut mux = test_mux(&workdir);
        let mut rx = attach_export_receiver(&mut mux);

        mux.send_file_export_frames("report.txt", false, true)
            .expect("regular file should export");
        mux.client.flush_out_of_band();

        let mut bytes = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            bytes.extend(chunk);
        }
        let frames = decode_server_frames(bytes).await;
        let ServerFrame::FileExportStart(start) = frames[0].clone() else {
            panic!("expected export start");
        };
        assert!(!start.reveal_after_export);
        assert!(start.open_after_export);
    }

    #[test]
    fn export_file_to_host_reports_reveal_queue() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();
        let mut mux = test_mux(&workdir);
        let _rx = attach_export_receiver(&mut mux);

        mux.export_file_to_host("report.txt".to_owned(), true, false);
        mux.client.flush_out_of_band();

        assert!(
            mux.clipboard_image_notice
                .as_deref()
                .is_some_and(|notice| notice.contains("File export and reveal queued: report.txt"))
        );
    }

    #[test]
    fn export_file_to_host_reports_open_queue() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        std::fs::write(workdir.join("report.txt"), b"hello export").unwrap();
        let mut mux = test_mux(&workdir);
        let _rx = attach_export_receiver(&mut mux);

        mux.export_file_to_host("report.txt".to_owned(), false, true);
        mux.client.flush_out_of_band();

        assert!(
            mux.clipboard_image_notice
                .as_deref()
                .is_some_and(|notice| notice.contains("File export and open queued: report.txt"))
        );
    }

    #[test]
    fn export_rejects_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        std::fs::create_dir(workdir.join("dir")).unwrap();
        let mut mux = test_mux(&workdir);

        let err = mux
            .send_file_export_frames("dir", false, false)
            .expect_err("directories are not exported");

        assert!(format!("{err:#}").contains("only regular files"));
    }

    #[test]
    fn export_rejects_missing_path() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        let mut mux = test_mux(&workdir);

        let err = mux
            .send_file_export_frames("missing.png", false, false)
            .expect_err("missing paths are not exported");

        assert!(format!("{err:#}").contains("resolving"));
        assert!(format!("{err:#}").contains("missing.png"));
    }

    #[test]
    fn export_file_to_host_reports_missing_path_without_frames() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        let mut mux = test_mux(&workdir);
        let mut rx = attach_export_receiver(&mut mux);

        mux.export_file_to_host("missing.png".to_owned(), false, false);
        mux.client.flush_out_of_band();

        assert!(rx.try_recv().is_err());
        assert!(
            mux.clipboard_image_notice
                .as_deref()
                .is_some_and(|notice| notice.contains("File export rejected:"))
        );
    }

    #[test]
    fn export_rejects_oversize_file() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        let path = workdir.join("large.bin");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_EXPORT_FILE_BYTES + 1).unwrap();
        drop(file);
        let mut mux = test_mux(&workdir);

        let err = mux
            .send_file_export_frames("large.bin", false, false)
            .expect_err("oversize files are not exported");

        assert!(format!("{err:#}").contains("current export cap"));
    }

    #[cfg(unix)]
    #[test]
    fn export_rejects_symlink_escape_from_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, b"outside").unwrap();
        std::os::unix::fs::symlink(&outside, workdir.join("escape.txt")).unwrap();
        let mut mux = test_mux(&workdir);

        let err = mux
            .send_file_export_frames("escape.txt", false, false)
            .expect_err("symlink escapes are not exported");

        assert!(format!("{err:#}").contains("workspace or /jackin/run"));
    }

    #[test]
    fn export_source_category_names_workspace_and_jackin_run() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        let report = workdir.join("report.txt");
        std::fs::write(&report, b"report").unwrap();
        let canonical_workdir = workdir.canonicalize().unwrap();

        assert_eq!(
            export_source_category(&report.canonicalize().unwrap(), &canonical_workdir),
            "workspace"
        );
        assert_eq!(
            export_source_category(
                Path::new("/jackin/run/clipboard/image.png"),
                &canonical_workdir
            ),
            "jackin-run"
        );
    }

    #[test]
    fn compact_export_queue_log_omits_full_paths() {
        let line = file_export_queue_compact_line("workspace", "report.md", 123, true, false);

        assert_eq!(
            line,
            "file-export: queued source_category=workspace basename=\"report.md\" bytes=123 reveal_after_export=true open_after_export=false"
        );
        assert!(!line.contains("/workspace"));
        assert!(!line.contains("/jackin/run"));
    }

    #[test]
    fn compact_export_rejection_helpers_avoid_requested_path() {
        let err = anyhow::anyhow!("resolving /workspace/private/report.md: missing");

        assert_eq!(
            requested_export_path_category("private/report.md"),
            "container-relative"
        );
        assert_eq!(compact_export_error_reason(&err), "not-found");
    }
}
