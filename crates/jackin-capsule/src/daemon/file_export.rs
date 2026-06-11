use std::fs::File;
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

const JACKIN_RUN_DIR: &str = "/jackin/run";
const MAX_EXPORT_FILE_BYTES: u64 = 64 * 1024 * 1024;

impl Multiplexer {
    pub(super) fn export_file_to_host(&mut self, requested_path: String) {
        match self.send_file_export_frames(&requested_path) {
            Ok(file_name) => {
                self.set_clipboard_image_notice(format!("File export queued: {file_name}"));
            }
            Err(err) => {
                crate::clog!("file-export: rejected {requested_path:?}: {err:#}");
                self.set_clipboard_image_notice(format!("File export rejected: {err:#}"));
            }
        }
    }

    fn send_file_export_frames(&mut self, requested_path: &str) -> Result<String> {
        let source = self.resolve_export_source(requested_path)?;
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
        let transfer_id = next_transfer_id();
        self.send_protocol_frame(ServerFrame::FileExportStart(FileExportStart {
            transfer_id,
            source_path: source.display().to_string(),
            file_name: file_name.clone(),
            size: metadata.len(),
        }));

        #[expect(
            clippy::disallowed_methods,
            reason = "file export is an explicit bounded operator action, not render emission"
        )]
        let mut file =
            File::open(&source).with_context(|| format!("opening {}", source.display()))?;
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
        crate::clog!(
            "file-export: queued {} bytes from {}",
            metadata.len(),
            source.display()
        );
        Ok(file_name)
    }

    fn resolve_export_source(&self, requested_path: &str) -> Result<PathBuf> {
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
            return Ok(source);
        }
        bail!("path must be inside the workspace or {JACKIN_RUN_DIR}");
    }
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
            .send_file_export_frames("report.txt")
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

    #[test]
    fn export_rejects_directory() {
        let temp = tempfile::tempdir().unwrap();
        let workdir = temp.path().join("workspace");
        std::fs::create_dir(&workdir).unwrap();
        std::fs::create_dir(workdir.join("dir")).unwrap();
        let mut mux = test_mux(&workdir);

        let err = mux
            .send_file_export_frames("dir")
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
            .send_file_export_frames("missing.png")
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

        mux.export_file_to_host("missing.png".to_owned());
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
            .send_file_export_frames("large.bin")
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
            .send_file_export_frames("escape.txt")
            .expect_err("symlink escapes are not exported");

        assert!(format!("{err:#}").contains("workspace or /jackin/run"));
    }
}
