// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn pull_request_fixture(number: u64) -> PullRequestInfo {
    PullRequestInfo {
        number,
        title: "Surface PR context in Capsule".to_owned(),
        url: format!("https://github.com/jackin-project/jackin/pull/{number}"),
        is_draft: false,
        checks: None,
    }
}

pub(super) fn widget_bar(
    cols: u16,
    branch: Option<&str>,
    usage_status_label: Option<&str>,
    pull_request: Option<&PullRequestInfo>,
    loading: bool,
    container: &str,
    hover: Option<crate::tui::model::HoverTarget>,
) -> (String, Buffer) {
    use ratatui::widgets::Widget as _;
    let area = Rect::new(0, 0, cols, 24);
    let mut buf = Buffer::empty(area);
    crate::tui::components::chrome::BottomChromeWidget {
        branch,
        usage_status_label,
        pull_request,
        pull_request_loading: loading,
        instance_id_label: container,
        hover_target: hover,
        scrollback_active: false,
        scroll_axes: termrock::scroll::ScrollAxes::none(),
        debug_run_id: None,
        prefix_awaiting: false,
        palette_key: 0x1C,
    }
    .render(area, &mut buf);
    let text: String = (0..cols)
        .map(|x| buf[(x, 23)].symbol().to_owned())
        .collect();
    (text, buf)
}
