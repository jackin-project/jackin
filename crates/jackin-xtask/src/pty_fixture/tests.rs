use std::fs;

use super::{Extraction, capsule_log_paths, extract_feed_pty_bytes, extract_from_run_or_log};

#[test]
fn extracts_matching_label_from_raw_log_line() {
    let line = "[jackin-capsule debug] session feed_pty bytes: agent=Some(\"codex\") label=Codex len=4 bytes=[1b, 5b, 32, 4a]";
    assert_eq!(
        extract_feed_pty_bytes(line, "Codex"),
        Some(vec![0x1b, 0x5b, 0x32, 0x4a])
    );
}

#[test]
fn rejects_other_labels_and_prefix_collisions() {
    let line =
        "[jackin-capsule debug] session feed_pty bytes: agent=None label=Shell len=1 bytes=[41]";
    assert_eq!(extract_feed_pty_bytes(line, "Codex"), None);
    assert_eq!(extract_feed_pty_bytes(line, "Shel"), None);
}

#[test]
fn rejects_lines_without_marker_or_bytes() {
    assert_eq!(extract_feed_pty_bytes("render: kind=full", "Codex"), None);
    let truncated = "session feed_pty bytes: label=Codex len=1 bytes=[41";
    assert_eq!(extract_feed_pty_bytes(truncated, "Codex"), None);
}

#[test]
fn follows_capsule_log_pointer_from_run_jsonl() {
    let temp = tempfile::tempdir().unwrap();
    let mux_log = temp.path().join("multiplexer.log");
    fs::write(
        &mux_log,
        "[jackin-capsule debug] session feed_pty bytes: agent=Some(\"codex\") label=Codex len=2 bytes=[41, 42]\n",
    )
    .unwrap();
    let detail = serde_json::json!({
        "container_name": "jk-test",
        "capsule_log": mux_log.display().to_string(),
    })
    .to_string();
    let run_jsonl = serde_json::json!({
        "kind": "container_started",
        "message": "container started",
        "detail": detail,
    })
    .to_string();

    assert_eq!(
        extract_from_run_or_log(&run_jsonl, "Codex").unwrap(),
        Extraction {
            out: vec![0x41, 0x42],
            chunks: 1,
        }
    );
}

#[test]
fn inline_feed_bytes_take_precedence_over_capsule_log_pointer() {
    let temp = tempfile::tempdir().unwrap();
    let mux_log = temp.path().join("multiplexer.log");
    fs::write(
        &mux_log,
        "[jackin-capsule debug] session feed_pty bytes: agent=Some(\"codex\") label=Codex len=1 bytes=[42]\n",
    )
    .unwrap();
    let detail = serde_json::json!({
        "container_name": "jk-test",
        "capsule_log": mux_log.display().to_string(),
    })
    .to_string();
    let run_jsonl = format!(
        "{}\n{}",
        serde_json::json!({
            "kind": "debug",
            "message": "[jackin-capsule debug] session feed_pty bytes: agent=Some(\"codex\") label=Codex len=1 bytes=[41]",
        }),
        serde_json::json!({
            "kind": "container_started",
            "message": "container started",
            "detail": detail,
        })
    );

    assert_eq!(
        extract_from_run_or_log(&run_jsonl, "Codex").unwrap(),
        Extraction {
            out: vec![0x41],
            chunks: 1,
        }
    );
}

#[test]
fn extracts_capsule_log_paths_from_container_started_details() {
    let run_jsonl = serde_json::json!({
        "kind": "container_started",
        "detail": "{\"capsule_log\":\"/tmp/jk/state/multiplexer.log\"}",
    })
    .to_string();

    assert_eq!(
        capsule_log_paths(&run_jsonl),
        vec![std::path::PathBuf::from("/tmp/jk/state/multiplexer.log")]
    );
}
