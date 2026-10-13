// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn click_when_no_git_prompt_is_active_returns_false() {
    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("parent");
    std::fs::create_dir(&parent).unwrap();
    let state = state_rooted_at(tmp.path().to_path_buf(), parent);
    assert!(state.pending_git_prompt.is_none());

    let modal = manufactured_modal_area();
    let url_rect = git_prompt_url_row_rect(modal, false).unwrap();
    let opened = state.url_to_open_on_click(modal, url_rect.x + url_rect.width / 2, url_rect.y);
    assert!(
        opened.is_none(),
        "click without active git prompt should be inert"
    );
}
