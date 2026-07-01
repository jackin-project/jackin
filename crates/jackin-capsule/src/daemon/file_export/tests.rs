#[cfg(test)]
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
            exec_bindings: Vec::new(),
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
