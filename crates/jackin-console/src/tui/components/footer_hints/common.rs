//! Shared footer fragments: tab-bar + content footer builders + the
//! `append_save_and_escape` / `append_open_in_github` helpers used by
//! every screen footer.

use jackin_tui::HintSpan;

use crate::tui::keymap::{
    EDITOR_CONTENT_KEYMAP, EDITOR_GLOBAL_KEYMAP, EDITOR_TAB_BAR_KEYMAP, EditorContentAction,
    EditorGlobalAction, EditorTabBarAction, WORKSPACE_LIST_KEYMAP, WorkspaceListAction,
};

#[must_use]
pub fn tab_bar_footer_items(
    save_label: &'static str,
    enter_content: bool,
    dirty_change_count: Option<usize>,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(multi-key-display-group): combined prev/next tab display; EDITOR_TAB_BAR_KEYMAP splits these into separate PrevTab (←/⇤) and NextTab (→) entries.
        HintSpan::Key("←/→"),
        HintSpan::Text("switch tab"),
    ];
    if enter_content {
        items.extend([
            HintSpan::GroupSep,
            // Both EDITOR_TAB_BAR_KEYMAP and SETTINGS_TAB_BAR_KEYMAP use the same glyph.
            HintSpan::Key(EDITOR_TAB_BAR_KEYMAP.glyph_for(EditorTabBarAction::FocusContent)),
            HintSpan::Text("enter content"),
        ]);
    }
    append_save_and_escape(&mut items, save_label, dirty_change_count);
    items
}

#[must_use]
pub fn content_footer_items(
    save_label: &'static str,
    row_items: Vec<HintSpan<'static>>,
    dirty_change_count: Option<usize>,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // Both EDITOR_CONTENT_KEYMAP and SETTINGS_*_TAB_KEYMAP use the same ↑↓ glyph.
        HintSpan::Key(EDITOR_CONTENT_KEYMAP.glyph_for(EditorContentAction::MoveUp)),
        HintSpan::Text("navigate"),
    ];

    if !row_items.is_empty() {
        items.push(HintSpan::GroupSep);
        items.extend(row_items);
    }

    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key(EDITOR_CONTENT_KEYMAP.glyph_for(EditorContentAction::FocusTabBar)),
        HintSpan::Text("tab bar"),
        HintSpan::GroupSep,
    ]);
    append_save_and_escape(&mut items, save_label, dirty_change_count);
    items
}

pub(super) fn append_open_in_github(items: &mut Vec<HintSpan<'static>>, has_github_url: bool) {
    if has_github_url {
        items.extend([
            HintSpan::Sep,
            // UNREGISTERABLE(workspace-mount-no-keymap): used by workspace-mount rows which have no backing keymap; global-mount callers use SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP directly.
            HintSpan::Key("O"),
            HintSpan::Text("open in GitHub"),
        ]);
    }
}

pub(super) fn append_save_and_escape(
    items: &mut Vec<HintSpan<'static>>,
    save_label: &'static str,
    dirty_change_count: Option<usize>,
) {
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key(EDITOR_GLOBAL_KEYMAP.glyph_for(EditorGlobalAction::Save)),
        HintSpan::Text(save_label),
    ]);
    if let Some(count) = dirty_change_count {
        items.push(HintSpan::Dyn(format!("({count} changes)")));
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key(EDITOR_GLOBAL_KEYMAP.glyph_for(EditorGlobalAction::Escape)),
        HintSpan::Text(if dirty_change_count.is_some() {
            "discard"
        } else {
            "back"
        }),
        HintSpan::Sep,
        HintSpan::Key(WORKSPACE_LIST_KEYMAP.glyph_for(WorkspaceListAction::Quit)),
        HintSpan::Text("quit"),
    ]);
}
