// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn file_browser_state_at(path: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        path.clone(),
        path,
    ))
}

pub(super) fn file_browser_state() -> FileBrowserState {
    let dir = tempfile::tempdir().unwrap();
    file_browser_state_at(dir.keep())
}

pub(super) fn labels(items: Vec<HintSpan<'static>>) -> Vec<String> {
    items
        .into_iter()
        .filter_map(|span| match span {
            HintSpan::Key(value) | HintSpan::Text(value) => Some(value.to_owned()),
            HintSpan::Dyn(value) | HintSpan::DynKey(value) => Some(value),
            HintSpan::Sep | HintSpan::GroupSep => None,
        })
        .collect()
}

pub(super) fn assert_file_browser_hints(items: Vec<HintSpan<'static>>) {
    let labels = labels(items);
    for expected in [
        "\u{2191}\u{2193}/j/k",
        "navigate",
        glyph::PGUP_PGDN,
        "page",
        "S",
        "select",
    ] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing {expected:?} in {labels:?}"
        );
    }
}
