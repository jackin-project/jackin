//! Shared footer hint fragments for modal pickers and confirmations.

use jackin_tui::HintSpan;

#[must_use]
pub fn mount_destination_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("M"),
        HintSpan::Text("mount"),
        HintSpan::GroupSep,
        HintSpan::Key("E"),
        HintSpan::Text("edit"),
        HintSpan::GroupSep,
        HintSpan::Key("\u{2190}/\u{2192}"),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn segmented_choice_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("\u{2190}/\u{2192}"),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn pick_list_footer_items(commit_label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text(commit_label),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn filtered_picker_footer_items(include_refresh: bool) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        HintSpan::Key("type"),
        HintSpan::Text("filter"),
    ];
    if include_refresh {
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("R"),
            HintSpan::Text("refresh"),
        ]);
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

#[must_use]
pub fn confirm_save_footer_items(scrollable: bool) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ];
    if scrollable {
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("scroll"),
        ]);
    }
    items
}

#[must_use]
pub fn yes_no_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("Y"),
        HintSpan::Text("yes"),
        HintSpan::GroupSep,
        HintSpan::Key("N/Esc"),
        HintSpan::Text("no"),
    ]
}
