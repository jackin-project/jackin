// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn labels(spans: &[HintSpan<'_>]) -> String {
    spans
        .iter()
        .filter_map(|span| match span {
            HintSpan::Key(text) | HintSpan::Text(text) => Some((*text).to_owned()),
            HintSpan::DynKey(text) => Some(text.clone()),
            HintSpan::Dyn(text) => Some(text.clone()),
            HintSpan::Sep | HintSpan::GroupSep => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
