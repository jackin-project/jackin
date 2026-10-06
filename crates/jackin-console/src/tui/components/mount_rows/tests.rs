// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `mount_rows`.
use super::*;

fn row(destination: &str, host_source: Option<&str>) -> MountDisplayRow {
    MountDisplayRow {
        destination: destination.to_owned(),
        host_source: host_source.map(str::to_owned),
        mode: "rw",
        isolation: "shared",
        kind: "bind".to_owned(),
    }
}

#[test]
fn mount_lines_render_rows_and_sources() {
    let rows = [row("/workspace", Some("host: ~/repo"))];
    let lines = render_mount_lines(&rows, 12);

    assert_eq!(lines[0].spans[0].content.as_ref(), "  /workspace    ");
    assert_eq!(lines[0].spans[1].content.as_ref(), "rw  ");
    assert_eq!(lines[0].spans[3].content.as_ref(), "shared   ");
    assert_eq!(lines[1].spans[0].content.as_ref(), "  host: ~/repo");
}

#[test]
fn global_mount_lines_render_header_and_rows() {
    let rows = [row("/cache", None)];
    let header = render_global_mount_header(12);
    let lines = render_global_mount_lines(&rows, 12);

    assert_eq!(header.spans[0].content.as_ref(), "  Destination   Mode");
    assert_eq!(lines[0].spans[0].content.as_ref(), "  /cache        ");
    assert_eq!(lines[0].spans[1].content.as_ref(), "rw");
}
