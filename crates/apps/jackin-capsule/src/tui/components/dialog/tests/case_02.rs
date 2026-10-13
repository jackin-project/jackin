// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn picker_enter_with_empty_filtered_list_is_redraw_noop() {
    let mut d = picker(vec!["claude", "codex"]);
    for &c in b"zzz" {
        d.handle_key(&[c], None);
    }
    assert_eq!(
        d.handle_key(b"\r", None),
        DialogAction::Redraw,
        "Enter with no matches must not synthesise a SpawnAgent"
    );
}

#[test]
fn rename_tab_empty_input_clears_label() {
    let mut d = Dialog::RenameTab {
        tab_idx: 3,
        input: termrock::widgets::TextInputState::new("").with_allow_empty(true),
    };
    match d.handle_key(b"\r", None) {
        DialogAction::RenameTab { tab_idx, label } => {
            assert_eq!(tab_idx, 3);
            assert_eq!(label, "");
        }
        other => panic!("expected RenameTab, got {other:?}"),
    }
}

#[test]
fn rename_tab_backspace_removes_last_char() {
    let mut d = Dialog::RenameTab {
        tab_idx: 0,
        input: termrock::widgets::TextInputState::new("abc"),
    };
    assert_eq!(d.handle_key(b"\x7f", None), DialogAction::Redraw);
    let Dialog::RenameTab { input, .. } = d else {
        unreachable!()
    };
    assert_eq!(input.value(), "ab");
}

#[test]
fn rename_tab_esc_dismisses() {
    let mut d = Dialog::RenameTab {
        tab_idx: 0,
        input: termrock::widgets::TextInputState::new("abc"),
    };
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
}

#[test]
fn rename_tab_consumes_q_as_input_not_dismiss() {
    // `q` is a dismiss key for list-style dialogs but must be
    // accepted as input inside the rename-tab buffer — otherwise
    // operators can't type the letter into their tab name.
    let mut d = Dialog::RenameTab {
        tab_idx: 0,
        input: termrock::widgets::TextInputState::new("a"),
    };
    assert_eq!(d.handle_key(b"q", None), DialogAction::Redraw);
    let Dialog::RenameTab { input, .. } = d else {
        unreachable!()
    };
    assert_eq!(input.value(), "aq");
}

#[test]
fn container_info_state_shows_invocation_identity_without_local_artifacts() {
    let d = container_info_with_diagnostics_fixture();
    let state = d
        .container_info_state_with_debug(true)
        .expect("container info state should be available");
    let rows = state.rows();
    assert_eq!(
        rows.first()
            .map(crate::tui::components::container_info_surface::ContainerInfoRow::value),
        Some("jk-inv-b93735"),
        "invocation identity must be the first Debug info row"
    );

    let invocation_row = rows
        .iter()
        .find(|row| row.value() == "jk-inv-b93735")
        .expect("invocation identity row present");
    assert!(invocation_row.is_copyable());
    assert!(rows.iter().all(|row| row.href().is_none()));
}

#[test]
fn container_info_state_without_invocation_omits_identity_row() {
    let d = Dialog::ContainerInfo {
        container_name: "jk-abc123-thearchitect".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace/jackin".to_owned(),
        diagnostics: ContainerInfoDiagnostics {
            host_version: "0.6.0-test".to_owned(),
            invocation_id: String::new(),
        },
        copied_row: None,
        hovered_row: None,
        scroll: termrock::scroll::DialogScroll::new(),
    };
    let state = d
        .container_info_state_with_debug(true)
        .expect("container info state should be available");
    let rows = state.rows();

    assert!(
        rows.iter()
            .all(|row| row.label() != "Invocation ID" && row.href().is_none()),
        "missing invocation identity must not fabricate a row or local artifact"
    );
}

#[test]
fn container_info_enter_flips_copied_flag_for_render_feedback() {
    let mut d = container_info_fixture();
    drop(d.handle_key(b"\r", None));
    let Dialog::ContainerInfo { copied_row, .. } = d else {
        unreachable!()
    };
    assert_eq!(
        copied_row,
        Some(0),
        "Enter must mark the container-id row copied so the next render shows the copied affordance"
    );
}

#[test]
fn container_info_enter_does_not_dismiss_dialog() {
    // Operator copies once and expects to read the badge before
    // dismissing themselves — handle_key must NOT return Dismiss
    // for Enter.
    let mut d = container_info_fixture();
    let action = d.handle_key(b"\r", None);
    assert!(
        matches!(action, DialogAction::CopyToClipboard(_)),
        "Enter must request a copy, not dismiss; got {action:?}"
    );
}

#[test]
fn container_info_enter_copies_container_name() {
    let mut d = container_info_fixture();
    match d.handle_key(b"\r", None) {
        DialogAction::CopyToClipboard(payload) => {
            assert_eq!(payload, "jk-abc123-thearchitect");
        }
        other => panic!("Enter must request clipboard copy, got {other:?}"),
    }
}

#[test]
fn container_info_click_on_id_row_copies_container_name() {
    let mut d = container_info_fixture();
    let (row, col, _, _) = d.box_rect(40, 100);
    // Click the value (the cyan link), not the label column.
    match d.handle_click(row + 2, col + 22, 40, 100, None) {
        DialogAction::CopyToClipboard(payload) => {
            assert_eq!(payload, "jk-abc123-thearchitect");
        }
        other => panic!("Container ID row click must request clipboard copy, got {other:?}"),
    }
    let Dialog::ContainerInfo { copied_row, .. } = d else {
        unreachable!()
    };
    assert_eq!(copied_row, Some(0), "ID row click must show copy feedback");
}

#[test]
fn container_info_visible_debug_rows_map_to_shared_hit_targets() {
    let term_rows = 60;
    let term_cols = 100;
    let source = container_info_with_diagnostics_fixture();
    let state = source
        .container_info_state_with_debug(true)
        .expect("container info state should be available");
    let (_, col, _, width) = source.box_rect(term_rows, term_cols);
    let height =
        crate::tui::components::container_info_surface::container_info_required_height(&state);
    let area = Rect {
        x: col,
        y: 4,
        width,
        height,
    };
    let cases = [
        ("jk-inv-b93735", "jk-inv-b93735"),
        ("jk-abc123-thearchitect", "jk-abc123-thearchitect"),
    ];

    for (visible_text, expected_payload) in cases {
        let (screen_row, screen_col) =
            visible_cell_for_value(&state, term_rows, term_cols, area, visible_text);
        let expected_row = state
            .rows()
            .iter()
            .position(|row| row.value() == expected_payload)
            .expect("expected payload should be in Debug-info state");
        assert_eq!(
            crate::tui::components::container_info_surface::container_info_copy_payload_at(
                area, &state, screen_col, screen_row
            ),
            Some((expected_row, expected_payload.to_owned())),
            "visible {visible_text:?} should hit its matching shared Debug-info row"
        );
    }
}

#[test]
fn container_info_r_does_not_reveal_local_telemetry_artifacts() {
    let mut d = container_info_with_diagnostics_fixture();
    assert_eq!(d.handle_key(b"r", None), DialogAction::Redraw);
}

#[test]
fn container_info_o_does_not_reveal_local_telemetry_artifacts() {
    let mut d = container_info_with_diagnostics_fixture();
    assert_eq!(d.handle_key(b"o", None), DialogAction::Redraw);
}

#[test]
fn container_info_o_does_not_open_github_context_url() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = container_info_with_diagnostics_fixture();
    assert_eq!(d.handle_key(b"o", Some(&view)), DialogAction::Redraw);
}

#[test]
fn container_info_r_without_diagnostics_log_redraws() {
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"r", None), DialogAction::Redraw);
}

#[test]
fn container_info_o_without_diagnostics_log_redraws() {
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"o", None), DialogAction::Redraw);
}

#[test]
fn container_info_visible_container_row_maps_to_dialog_hover_and_copy_target() {
    let term_rows = 60;
    let term_cols = 100;
    let source = container_info_with_diagnostics_fixture();
    let (row, col, height, width) = source.box_rect(term_rows, term_cols);
    let area = Rect {
        x: col,
        y: row,
        width,
        height,
    };
    let state = source
        .container_info_state()
        .expect("container info state should be available");
    let (screen_row, screen_col) =
        visible_cell_for_value(&state, term_rows, term_cols, area, "jk-abc123-thearchitect");

    let mut hover_dialog = source.clone();
    assert!(
        hover_dialog.set_container_info_hover(screen_row, screen_col, term_rows, term_cols),
        "hovering visible container id should update row hover"
    );
    let Dialog::ContainerInfo { hovered_row, .. } = hover_dialog else {
        unreachable!()
    };
    assert_eq!(
        hovered_row,
        Some(0),
        "visible container id hover should target matching row"
    );

    let mut click_dialog = source;
    match click_dialog.handle_click(screen_row, screen_col, term_rows, term_cols, None) {
        DialogAction::CopyToClipboard(payload) => assert_eq!(payload, "jk-abc123-thearchitect"),
        other => panic!("visible container id click must copy payload, got {other:?}"),
    }
    let Dialog::ContainerInfo { copied_row, .. } = click_dialog else {
        unreachable!()
    };
    assert_eq!(
        copied_row,
        Some(0),
        "visible container id click should show copied feedback on matching row"
    );
}

#[test]
fn container_info_click_on_other_rows_does_not_copy() {
    let mut d = container_info_fixture();
    let (row, col, _, _) = d.box_rect(40, 100);
    assert_eq!(
        d.handle_click(row + 3, col + 2, 40, 100, None),
        DialogAction::Consume
    );
    let Dialog::ContainerInfo { copied_row, .. } = d else {
        unreachable!()
    };
    assert!(
        copied_row.is_none(),
        "non-copyable rows must not show copy feedback"
    );
}

#[test]
fn container_info_clear_copy_feedback_hides_badge() {
    let mut d = Dialog::ContainerInfo {
        container_name: "jk-abc123-thearchitect".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace/jackin".to_owned(),
        diagnostics: ContainerInfoDiagnostics::default(),
        copied_row: Some(0),
        hovered_row: None,
        scroll: termrock::scroll::DialogScroll::new(),
    };
    assert!(d.clear_copy_feedback());
    let Dialog::ContainerInfo { copied_row, .. } = d else {
        unreachable!()
    };
    assert!(copied_row.is_none());
}

#[test]
fn github_context_enter_copies_pr_url_and_shows_feedback() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: termrock::scroll::DialogScroll::new(),
    };

    match d.handle_key(b"\r", Some(&view)) {
        DialogAction::CopyToClipboard(payload) => {
            assert_eq!(payload, "https://github.com/jackin-project/jackin/pull/123");
        }
        other => panic!("Enter must request PR URL copy, got {other:?}"),
    }
    assert!(d.has_copy_feedback());
}

#[test]
fn github_context_o_opens_pr_url() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: termrock::scroll::DialogScroll::new(),
    };

    match d.handle_key(b"o", Some(&view)) {
        DialogAction::OpenHostUrl(url) => {
            assert_eq!(url, "https://github.com/jackin-project/jackin/pull/123");
        }
        other => panic!("O must request host PR open, got {other:?}"),
    }
}

#[test]
fn github_context_c_opens_ci_url_when_available() {
    let mut pr = pull_request_fixture();
    pr.checks = Some(
        crate::pull_request::PullRequestChecks::from_buckets(["fail"]).with_ci_url(Some(
            "https://github.com/jackin-project/jackin/actions/runs/1/job/2".to_owned(),
        )),
    );
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: termrock::scroll::DialogScroll::new(),
    };

    match d.handle_key(b"c", Some(&view)) {
        DialogAction::OpenHostUrl(url) => {
            assert_eq!(
                url,
                "https://github.com/jackin-project/jackin/actions/runs/1/job/2"
            );
        }
        other => panic!("C must request host CI open, got {other:?}"),
    }
}
