// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn debug_info_hints_have_no_local_artifact_actions() {
    let hints = debug_info_hint_spans(termrock::components::ScrollAxes::default());
    let text = hints
        .iter()
        .filter_map(|hint| match hint {
            termrock::HintSpan::Key(value) | termrock::HintSpan::Text(value) => Some(*value),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("copy value"));
    assert!(!text.contains("R/O"));
    assert!(!text.contains("reveal diagnostics"));
}
