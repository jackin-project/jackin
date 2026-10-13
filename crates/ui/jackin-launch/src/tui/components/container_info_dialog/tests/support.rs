// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
}

pub(super) fn view_with_identity() -> LaunchView {
    let mut view = initial_view();
    view.frame = 30;
    view.status = "building docker image".to_owned();
    view.identity = Some(LaunchIdentity {
        role: "the-architect".to_owned(),
        agent: "codex".to_owned(),
        target_kind: LaunchTargetKind::Directory,
        target_label: "/workspace/jackin".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: Some("jk-2y0t4aw6-thearchitect".to_owned()),
    });
    view
}
