use super::extract_feed_pty_bytes;

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
