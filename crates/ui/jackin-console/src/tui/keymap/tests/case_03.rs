// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn visibility_editor_tab_bar_alias_stays_hidden() {
    let keys = hint_keys(EDITOR_TAB_BAR_KEYMAP.hint_spans());
    assert!(keys.iter().any(|key| key == "⇥/↓"), "{keys:?}");
    // j/J FocusContent alias is HiddenAlias: no standalone glyph appears.
    assert!(!keys.iter().any(|key| key == "J"), "{keys:?}");
}
