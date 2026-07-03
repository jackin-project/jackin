//! Story definitions and render functions for the jackin-tui lookbook.
//!
//! Each `fn story_*` function renders a component snapshot into the frame area
//! provided. All calls go through the same public API the rest of jackin uses.

use jackin_tui::{
    HintSpan,
    components::{
        ButtonStrip, ButtonStripItem, ConfirmState, DebugInfo, DiffViewState, ErrorPopupState,
        Panel, PanelFocus, SaveDiscardFocus, SaveDiscardState, SelectListState, SinglePaneKind,
        StatusFooterHover, TabStrip, TextInputState, Toast, panel_body_area, render_brand_header,
        render_confirm_dialog, render_container_info, render_diff_view, render_error_dialog,
        render_filter_input,
        render_save_discard_dialog, render_scrollable_block, render_select_list,
        render_status_footer, render_status_popup, render_text_input, render_toast,
        render_wrapped_hint_bar,
    },
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::interactors::{
    ButtonStripInteractor, ConfirmInteractor, SaveDiscardInteractor, ScrollablePanelInteractor,
    SelectListInteractor, StaticStory, StoryInteraction, TabStripInteractor, TextInputInteractor,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Story {
    pub id: &'static str,
    pub title: &'static str,
    pub component: &'static str,
    pub description: &'static str,
    pub width: u16,
    pub height: u16,
    render: fn(&mut Frame<'_>, Rect),
}

impl Story {
    #[must_use]
    pub(crate) const fn new(
        id: &'static str,
        title: &'static str,
        component: &'static str,
        description: &'static str,
        width: u16,
        height: u16,
        render: fn(&mut Frame<'_>, Rect),
    ) -> Self {
        Self {
            id,
            title,
            component,
            description,
            width,
            height,
            render,
        }
    }

    pub(crate) fn render(self, frame: &mut Frame<'_>, area: Rect) {
        (self.render)(frame, area);
    }

    /// Create a stateful interactor for this story. Stories with known
    /// interactive implementations return a live interactor; all others fall
    /// back to a `StaticStory` that simply calls the fn-pointer render.
    #[must_use]
    pub(crate) fn make_interactor(&self) -> Box<dyn StoryInteraction> {
        match self.id {
            "tab-strip/basic" => Box::new(TabStripInteractor::new()),
            "select-list/agent-picker" => Box::new(SelectListInteractor::new()),
            "scrollable-panel/mounts" => Box::new(ScrollablePanelInteractor::new()),
            "confirm/default" => Box::new(ConfirmInteractor::default_story()),
            "confirm/role-trust" => Box::new(ConfirmInteractor::role_trust_story()),
            "text-input/workspace-name" => Box::new(TextInputInteractor::new()),
            "button-strip/basic" => Box::new(ButtonStripInteractor::new()),
            "save-discard/default" => Box::new(SaveDiscardInteractor::new()),
            _ => Box::new(StaticStory {
                render_fn: self.render,
            }),
        }
    }
}

#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "Lookbook story catalog: one entry per component, each constructing \
              its own Story. The flat shape is the canonical story list — \
              extracting per-component helpers would obscure the catalog layout."
)]
pub(crate) fn stories() -> Vec<Story> {
    vec![
        Story::new(
            "brand-header/console",
            "Brand header",
            "BrandHeader",
            "Console row-0 brand pill with current surface label.",
            54,
            1,
            story_brand_header_console,
        ),
        Story::new(
            "panel/focused",
            "Panel focused",
            "Panel",
            "Focused workspace summary panel with realistic rows.",
            54,
            8,
            story_panel_focused,
        ),
        Story::new(
            "button-strip/basic",
            "Button strip",
            "ButtonStrip",
            "Save flow actions. ← → to move focus.",
            54,
            3,
            story_button_strip,
        ),
        Story::new(
            "tab-strip/basic",
            "Tab strip",
            "TabStrip",
            "Workspace editor tabs with active, inactive, and hovered state.",
            58,
            2,
            story_tab_strip,
        ),
        Story::new(
            "confirm/default",
            "Confirm dialog",
            "ConfirmDialog",
            "Destructive workspace-delete confirmation.",
            48,
            8, // 2 borders + 1 leading + 2 prompt lines + 1 spacer + 1 buttons + 1 trailing
            story_confirm_default,
        ),
        Story::new(
            "confirm/role-trust",
            "Role trust dialog",
            "ConfirmDialog",
            "Role-source trust confirmation with structured role and repository fields.",
            70,
            13, // 2 borders + 1 leading + 1 prompt + 1 sep + 2 rows + 1 sep + 2 notes + 1 spacer + 1 buttons + 1 trailing
            story_confirm_role_trust,
        ),
        Story::new(
            "error/default",
            "Error dialog",
            "ErrorDialog",
            "Launch failure modal with acknowledged OK action.",
            62,
            10, // 2 borders + 1 leading + 2-3 body rows + 1 spacer + 1 button + 1 trailing
            story_error_default,
        ),
        Story::new(
            "save-discard/default",
            "Save/discard dialog",
            "SaveDiscardDialog",
            "Dirty workspace editor exit with Cancel focused by default.",
            54,
            7, // 2 borders + 1 leading + 1 prompt + 1 spacer + 1 buttons + 1 trailing
            story_save_discard_default,
        ),
        Story::new(
            "status-popup/default",
            "Status popup",
            "StatusPopup",
            "Non-interactive role-resolution progress popup.",
            48,
            7, // 2 borders + 1 leading + 1 message + 1 spacer + 1 please-wait + 1 trailing
            story_status_popup_default,
        ),
        Story::new(
            "filter-input/populated",
            "Filter input",
            "FilterInput",
            "Picker filter row with typed query and visible cursor.",
            42,
            1,
            story_filter_input_populated,
        ),
        Story::new(
            "hint-bar/manager-footer",
            "Hint bar",
            "HintBar",
            "Wrapped manager footer shortcuts grouped by action.",
            54,
            2,
            story_hint_bar_manager_footer,
        ),
        Story::new(
            "select-list/agent-picker",
            "Select list",
            "SelectList",
            "Agent picker with context copy and selected row.",
            58,
            11,
            story_select_list_agent_picker,
        ),
        Story::new(
            "scrollable-panel/mounts",
            "Scrollable panel",
            "ScrollablePanel",
            "Mount table that overflows both axes and shows scrollbars.",
            64,
            9,
            story_scrollable_panel_mounts,
        ),
        Story::new(
            "status-footer/launch-progress",
            "Status footer",
            "StatusFooter",
            "White launch status footer with instance and debug chips.",
            72,
            1,
            story_status_footer_launch_progress,
        ),
        Story::new(
            "status-footer/cockpit-chrome",
            "Status footer",
            "StatusFooter",
            "Launch cockpit bottom chrome: hint bar above the white status footer.",
            72,
            3,
            story_status_footer_cockpit_chrome,
        ),
        Story::new(
            "text-input/workspace-name",
            "Text input",
            "TextInput",
            "Workspace rename dialog with current value and cursor.",
            58,
            5,
            story_text_input_workspace_name,
        ),
        Story::new(
            "toast/selection-copied",
            "Toast",
            "Toast",
            "Non-blocking selection-copy feedback above reserved footer rows.",
            54,
            8,
            story_toast_selection_copied,
        ),
        Story::new(
            "panel/unfocused",
            "Panel unfocused",
            "Panel",
            "Unfocused panel showing dark PHOSPHOR_DARK border.",
            54,
            6,
            story_panel_unfocused,
        ),
        Story::new(
            "container-info/debug",
            "Debug info",
            "ContainerInfoState",
            "Debug-mode debug info dialog with run ID and log path rows.",
            72,
            12,
            story_container_info_debug,
        ),
        Story::new(
            "diff-view/side-by-side",
            "Diff view side-by-side",
            "DiffView",
            "Modified-file diff with paired removed and added rows.",
            74,
            11,
            story_diff_view_side_by_side,
        ),
        Story::new(
            "diff-view/single-pane",
            "Diff view single-pane",
            "DiffView",
            "Added-file diff rendered as one scrollable pane.",
            58,
            9,
            story_diff_view_single_pane,
        ),
        Story::new(
            "select-list/empty",
            "Select list empty",
            "SelectList",
            "Select list with no items showing the empty-state placeholder.",
            48,
            8,
            story_select_list_empty,
        ),
        Story::new(
            "confirm/focus-yes",
            "Confirm dialog — Yes focused",
            "ConfirmDialog",
            "Confirmation dialog with Yes button pre-selected (non-default state).",
            48,
            7, // 2 borders + 1 leading + 1 prompt + 1 spacer + 1 buttons + 1 trailing
            story_confirm_focus_yes,
        ),
        Story::new(
            "button-strip/all-disabled",
            "Button strip all-disabled",
            "ButtonStrip",
            "Button strip with all buttons disabled — no item can be selected.",
            54,
            3,
            story_button_strip_all_disabled,
        ),
        Story::new(
            "save-discard/focus-save",
            "Save/discard dialog — Save focused",
            "SaveDiscardDialog",
            "Dirty workspace editor exit with Save pre-selected.",
            54,
            7,
            story_save_discard_focus_save,
        ),
        Story::new(
            "save-discard/focus-discard",
            "Save/discard dialog — Discard focused",
            "SaveDiscardDialog",
            "Dirty exit with Discard pre-selected (destructive focus state).",
            54,
            7,
            story_save_discard_focus_discard,
        ),
        Story::new(
            "scrollable-panel/scrolled",
            "Scrollable panel scrolled",
            "ScrollablePanel",
            "Mount list scrolled down with vertical thumb visible on the border.",
            54,
            8,
            story_scrollable_panel_scrolled,
        ),
        Story::new(
            "select-list/filtered-empty",
            "Select list filtered-empty",
            "SelectList",
            "Select list with a non-matching filter showing the 'no matches' placeholder.",
            48,
            8,
            story_select_list_filtered_empty,
        ),
    ]
}

fn story_brand_header_console(frame: &mut Frame<'_>, area: Rect) {
    render_brand_header(frame, area, "Console · workspace editor");
}

fn story_panel_focused(frame: &mut Frame<'_>, area: Rect) {
    let block = Panel::new()
        .title("Workspace")
        .focus(PanelFocus::FocusedScrollable)
        .block();
    let content_area = panel_body_area(&block, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Name: ", jackin_tui::theme::BOLD_WHITE),
                Span::styled("jackin-core", jackin_tui::theme::GREEN),
            ]),
            Line::from(vec![
                Span::styled("Role: ", jackin_tui::theme::BOLD_WHITE),
                Span::styled(
                    "github.com/jackin-project/roles/rust",
                    jackin_tui::theme::GREEN,
                ),
            ]),
            Line::from(vec![
                Span::styled("Agent: ", jackin_tui::theme::BOLD_WHITE),
                Span::styled("codex", jackin_tui::theme::GREEN),
            ]),
            Line::from(vec![
                Span::styled("Mounts: ", jackin_tui::theme::BOLD_WHITE),
                Span::styled("repo rw, ~/.config/gh ro", jackin_tui::theme::DIM),
            ]),
        ]),
        content_area,
    );
}

fn story_button_strip(frame: &mut Frame<'_>, area: Rect) {
    use ratatui::layout::{Constraint, Layout};
    let items = [
        ButtonStripItem::new("Save"),
        ButtonStripItem::new("Discard"),
        ButtonStripItem::disabled("Launch"),
        ButtonStripItem::new("Cancel"),
    ];
    // ButtonStrip is a single-row widget; center it vertically in the story canvas.
    let [_, strip_area, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);
    ButtonStrip::new(&items)
        .focused(0)
        .render(frame, strip_area);
}

fn story_tab_strip(frame: &mut Frame<'_>, area: Rect) {
    let labels = [
        ("General", true),
        ("Mounts", false),
        ("Roles", false),
        ("Secrets", false),
    ];
    TabStrip::new(&labels)
        .focused(true)
        .hovered(Some(2))
        .render(frame, area);
}

fn story_confirm_default(frame: &mut Frame<'_>, area: Rect) {
    let state = ConfirmState::new(
        "Delete workspace \"jackin-core\"?\nThis removes the saved workspace entry.",
    );
    render_confirm_dialog(frame, area, &state);
}

fn story_confirm_role_trust(frame: &mut Frame<'_>, area: Rect) {
    let state = ConfirmState::details(
        "Trust role source",
        "Trust this role source?",
        vec![
            ("Role".into(), "rust".into()),
            (
                "Repository".into(),
                "https://github.com/jackin-project/roles".into(),
            ),
        ],
        vec![
            "Dockerfile can run during image builds.".into(),
            "The role can access mounted workspace files.".into(),
        ],
    );
    render_confirm_dialog(frame, area, &state);
}

fn story_error_default(frame: &mut Frame<'_>, area: Rect) {
    let state = ErrorPopupState::new(
        "Launch failed",
        "Derived image build failed while installing role dependencies.\nOpen diagnostics run jk-run-3d7e23 for the full log.",
    );
    render_error_dialog(frame, area, &state);
}

fn story_save_discard_default(frame: &mut Frame<'_>, area: Rect) {
    let mut state = SaveDiscardState::new("Save workspace changes before leaving?");
    state.focus = SaveDiscardFocus::Cancel;
    render_save_discard_dialog(frame, area, &state);
}

fn story_status_popup_default(frame: &mut Frame<'_>, area: Rect) {
    let state = jackin_tui::components::StatusPopupState::new(
        "Loading role",
        "Resolving github:jackin-project/roles/rust",
    );
    render_status_popup(frame, area, &state);
}

fn story_filter_input_populated(frame: &mut Frame<'_>, area: Rect) {
    render_filter_input(frame, area, "cod");
}

fn story_hint_bar_manager_footer(frame: &mut Frame<'_>, area: Rect) {
    // UNREGISTERABLE(lookbook-fixture): static gallery sample illustrating the
    // hint-bar widget's layout. The lookbook crate has no dispatch surface and
    // no `Keymap<A>` to derive these glyphs from — every entry is demo content,
    // not an advertised real keybinding.
    let spans = [
        HintSpan::Key("↑↓"),
        HintSpan::Text("select"),
        HintSpan::Sep,
        HintSpan::Key("↵"),
        HintSpan::Text("open"),
        HintSpan::Sep,
        HintSpan::Key("D"),
        HintSpan::Text("delete"),
        HintSpan::GroupSep,
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::Sep,
        HintSpan::Key("Esc"),
        HintSpan::Text("back"),
    ];
    render_wrapped_hint_bar(frame, area, &spans);
}

fn story_select_list_agent_picker(frame: &mut Frame<'_>, area: Rect) {
    let mut state = SelectListState::new(vec![
        "claude".to_owned(),
        "codex".to_owned(),
        "amp".to_owned(),
        "kimi".to_owned(),
        "opencode".to_owned(),
    ]);
    state.select_index(1);
    let context = [
        Line::from(vec![
            Span::styled("Workspace: ", jackin_tui::theme::BOLD_WHITE),
            Span::styled("jackin-core", jackin_tui::theme::GREEN),
        ]),
        Line::from(vec![
            Span::styled("Role: ", jackin_tui::theme::BOLD_WHITE),
            Span::styled("rust", jackin_tui::theme::GREEN),
        ]),
    ];
    render_select_list(frame, area, &state, "Choose agent", &context);
}

fn story_scrollable_panel_mounts(frame: &mut Frame<'_>, area: Rect) {
    let lines = vec![
        Line::from("repo               /workspace/jackin-project/jackin                       rw"),
        Line::from("github-cli         /jackin/host/config/gh                             ro"),
        Line::from("codex              /jackin/codex                                      ro"),
        Line::from("claude             /jackin/claude                                     ro"),
        Line::from("cache              /jackin/host/cache/cargo                           rw"),
        Line::from("socket             /jackin/run/jackin.sock                            rw"),
        Line::from("role-manifest      /workspace/jackin.role.toml                         ro"),
        Line::from("diagnostics        /jackin/state/diagnostics/jk-run-3d7e23             rw"),
        Line::from("ssh                /jackin/host/ssh                                   ro"),
        Line::from("op-session         /jackin/host/config/op                             ro"),
    ];
    let mut scroll_x = 0;
    let mut scroll_y = 0;
    render_scrollable_block(
        frame,
        area,
        lines,
        &mut scroll_x,
        &mut scroll_y,
        true,
        Some("Global mounts"),
    );
}

fn story_status_footer_launch_progress(frame: &mut Frame<'_>, area: Rect) {
    render_status_footer(
        frame,
        area,
        "Building role image: rust-dev",
        "s7f8a2c1",
        Some("jk-run-3d7e23"),
        1.0, // fully opaque — the real launch cockpit fades in over ~30 frames
        StatusFooterHover {
            left: true,
            usage: false,
            right: false,
            right_debug: false,
        },
    );
}

// UNREGISTERABLE(lookbook-fixture): static gallery sample mirroring the launch
// cockpit's hint bar. The real cockpit derives these from COCKPIT_KEYMAP, but
// the lookbook crate depends only on jackin-tui and cannot reference the launch
// keymap — so this preview hardcodes the same glyphs as demo content.
const COCKPIT_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Ctrl-C"),
    HintSpan::Text("abort"),
    HintSpan::GroupSep,
    HintSpan::Key("Ctrl-Q"),
    HintSpan::Text("quit"),
];

fn story_status_footer_cockpit_chrome(frame: &mut Frame<'_>, area: Rect) {
    use jackin_tui::components::{bottom_chrome_areas, render_hint_bar};
    let chrome = bottom_chrome_areas(area);
    render_hint_bar(frame, chrome.hint, COCKPIT_HINT);
    render_status_footer(
        frame,
        chrome.footer,
        "Building role image: rust-dev",
        "s7f8a2c1",
        Some("jk-run-3d7e23"),
        1.0,
        StatusFooterHover {
            left: false,
            usage: false,
            right: false,
            right_debug: false,
        },
    );
}

fn story_text_input_workspace_name(frame: &mut Frame<'_>, area: Rect) {
    let state = TextInputState::new("Workspace name", "jackin-core");
    render_text_input(frame, area, &state);
}

fn story_toast_selection_copied(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Capsule pane content remains visible behind the toast."),
            Line::from("The footer rows below are reserved for available actions."),
            Line::from(""),
            Line::from("Ctrl+\\ menu   click focus pane"),
            Line::from("PR #495 · refactor: finish TUI architecture epic"),
        ]),
        area,
    );
    render_toast(frame, area, Toast::new("Selection copied"));
}

fn story_panel_unfocused(frame: &mut Frame<'_>, area: Rect) {
    let block = Panel::new()
        .title("Settings")
        .focus(PanelFocus::Unfocused)
        .block();
    let content_area = panel_body_area(&block, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Co-author trailer  enabled",
            jackin_tui::theme::DIM,
        ))),
        content_area,
    );
}

fn story_container_info_debug(frame: &mut Frame<'_>, area: Rect) {
    // Built from the shared accumulating model exactly as the capsule does once
    // every fact is known — Container ID, Run ID, and Diagnostics log render as
    // copyable cyan links; versions match the CLI strings.
    let state = DebugInfo {
        jackin_version: Some("0.6.0-dev".to_owned()),
        capsule_version: Some("0.6.0-dev+444004b".to_owned()),
        container_id: Some("jk-sk76zdat-thearchitect".to_owned()),
        role: Some("the-architect".to_owned()),
        agent: Some("claude".to_owned()),
        target: Some("/Users/jackin/Projects/jackin".to_owned()),
        run_id: Some("jk-run-cc5ff2".to_owned()),
        diagnostics_log_path: Some(
            "/Users/jackin/.jackin/data/diagnostics/runs/jk-run-cc5ff2.jsonl".to_owned(),
        ),
    }
    .into_state();
    render_container_info(frame, area, &state);
}

fn story_diff_view_side_by_side(frame: &mut Frame<'_>, area: Rect) {
    let before = r#"roles:
  architect:
    agent: claude
    trust: prompt
env:
  JACKIN_DEBUG: "0"
"#;
    let after = r#"roles:
  architect:
    agent: claude
    trust: full
env:
  JACKIN_DEBUG: "1"
"#;
    let mut state = DiffViewState::side_by_side(before, after, "before", "after");
    render_diff_view(frame, area, &mut state);
}

fn story_diff_view_single_pane(frame: &mut Frame<'_>, area: Rect) {
    let content = r#"name = "capsule-tools"
image = "ghcr.io/jackin-project/capsule-tools"
agent = "codex"
trust = "prompt"
"#;
    let mut state = DiffViewState::single_pane(content, SinglePaneKind::Added, "added role.toml");
    render_diff_view(frame, area, &mut state);
}

fn story_select_list_empty(frame: &mut Frame<'_>, area: Rect) {
    let state = SelectListState::new(vec![]);
    render_select_list(frame, area, &state, "No roles found", &[]);
}

fn story_confirm_focus_yes(frame: &mut Frame<'_>, area: Rect) {
    let state = ConfirmState::new("Exit without saving?").with_focus_yes();
    render_confirm_dialog(frame, area, &state);
}

fn story_button_strip_all_disabled(frame: &mut Frame<'_>, area: Rect) {
    use ratatui::layout::{Constraint, Layout};
    let items = [
        ButtonStripItem::disabled("Save"),
        ButtonStripItem::disabled("Discard"),
        ButtonStripItem::disabled("Cancel"),
    ];
    let [_, strip_area, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);
    ButtonStrip::new(&items)
        .focused(0)
        .render(frame, strip_area);
}

fn story_save_discard_focus_save(frame: &mut Frame<'_>, area: Rect) {
    let mut state = SaveDiscardState::new("Save workspace changes before leaving?");
    state.focus = SaveDiscardFocus::Save;
    render_save_discard_dialog(frame, area, &state);
}

fn story_save_discard_focus_discard(frame: &mut Frame<'_>, area: Rect) {
    let mut state = SaveDiscardState::new("Discard all uncommitted changes?");
    state.focus = SaveDiscardFocus::Discard;
    render_save_discard_dialog(frame, area, &state);
}

fn story_select_list_filtered_empty(frame: &mut Frame<'_>, area: Rect) {
    // Filter "xyz" matches none of the items — shows the "no matches" placeholder.
    let state = SelectListState::new(vec![
        "the-architect".into(),
        "agent-smith".into(),
        "neo".into(),
    ])
    .with_filter("xyz");
    render_select_list(frame, area, &state, "Select Role", &[]);
}

fn story_scrollable_panel_scrolled(frame: &mut Frame<'_>, area: Rect) {
    let lines = vec![
        Line::from("repo               /workspace/jackin-project/jackin                       rw"),
        Line::from("github-cli         /jackin/host/config/gh                             ro"),
        Line::from("codex              /jackin/codex                                      ro"),
        Line::from("claude             /jackin/claude                                     ro"),
        Line::from("cache              /jackin/host/cache/cargo                           rw"),
        Line::from("socket             /jackin/run/jackin.sock                            rw"),
        Line::from("role-manifest      /workspace/jackin.role.toml                         ro"),
        Line::from("diagnostics        /jackin/state/diagnostics/jk-run-3d7e23             rw"),
        Line::from("ssh                /jackin/host/ssh                                   ro"),
        Line::from("op-session         /jackin/host/config/op                             ro"),
        Line::from("dind               /var/run/docker.sock                               rw"),
        Line::from("certs              /jackin/host/certs                                 ro"),
    ];
    let mut scroll_x = 0;
    let mut scroll_y = 4; // scrolled past the first 4 rows — thumb visible
    render_scrollable_block(
        frame,
        area,
        lines,
        &mut scroll_x,
        &mut scroll_y,
        true,
        Some("Global mounts (scrolled)"),
    );
}

#[cfg(test)]
mod tests;
