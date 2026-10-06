// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn github_context_url_click_copies_pr_url() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: termrock::scroll::DialogScroll::new(),
    };
    let (row, col, _, _) = d.box_rect(40, 120);

    assert!(d.clickable_at(row + 5, col + 18, 40, 120, Some(&view)));
    match d.handle_click(row + 5, col + 18, 40, 120, Some(&view)) {
        DialogAction::CopyToClipboard(payload) => {
            assert_eq!(payload, "https://github.com/jackin-project/jackin/pull/123");
        }
        other => panic!("GitHub URL row click must request clipboard copy, got {other:?}"),
    }
    assert!(d.has_copy_feedback());
}

#[test]
fn github_context_open_rows_click_open_urls() {
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
    let (row, col, _, _) = d.box_rect(40, 120);

    assert!(d.clickable_at(row + 7, col + 18, 40, 120, Some(&view)));
    match d.handle_click(row + 7, col + 18, 40, 120, Some(&view)) {
        DialogAction::OpenHostUrl(url) => {
            assert_eq!(url, "https://github.com/jackin-project/jackin/pull/123");
        }
        other => panic!("Open PR row click must request host open, got {other:?}"),
    }

    assert!(d.clickable_at(row + 8, col + 18, 40, 120, Some(&view)));
    match d.handle_click(row + 8, col + 18, 40, 120, Some(&view)) {
        DialogAction::OpenHostUrl(url) => {
            assert_eq!(
                url,
                "https://github.com/jackin-project/jackin/actions/runs/1/job/2"
            );
        }
        other => panic!("Open CI row click must request host open, got {other:?}"),
    }
}

#[test]
fn github_context_unavailable_ci_row_is_not_clickable() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: termrock::scroll::DialogScroll::new(),
    };
    let (row, col, _, _) = d.box_rect(40, 120);

    assert!(
        !d.clickable_at(row + 8, col + 18, 40, 120, Some(&view)),
        "unavailable CI row must not advertise a clickable host-open target"
    );
    assert_eq!(
        d.handle_click(row + 8, col + 18, 40, 120, Some(&view)),
        DialogAction::Consume,
        "clicking unavailable CI should be consumed inside the dialog"
    );
    assert_eq!(
        d.handle_key(b"c", Some(&view)),
        DialogAction::Redraw,
        "C shortcut should not open a host URL without a CI target"
    );
}

#[test]
fn github_context_uses_shared_focused_info_dialog() {
    let pr = pull_request_fixture();
    let d = Dialog::GitHubContext {
        copied: false,
        scroll: termrock::scroll::DialogScroll::new(),
    };

    let view = github_view_for_fixture(&pr);
    let snapshot = d.to_ratatui_snapshot(Some(&view));
    let crate::tui::components::dialog_widgets::DialogRatatuiSnapshot::DebugInfo(state) = snapshot
    else {
        panic!("GitHub context must use the shared ContainerInfoState renderer");
    };

    assert_eq!(
        state.rows()[3].value(),
        "https://github.com/jackin-project/jackin/pull/123"
    );
    assert!(
        state.rows()[3].is_copyable(),
        "GitHub URL should be the copyable shared info row"
    );
}

#[test]
fn usage_projection_empty_inventory_has_no_retry_copy() {
    let dialog = Dialog::new_usage_with_tab(
        jackin_protocol::control::FocusedUsageView::unavailable(
            "No agents configured for this Capsule.",
            1_781_185_560,
        ),
        UsageDialogTab::Overview,
    );
    let state = dialog.usage_state().expect("usage state");
    assert_eq!(
        state.rows()[0].value(),
        "No agents configured for this Capsule."
    );
    let hints = dialog.footer_hint_spans(None, termrock::scroll::ScrollAxes::none());
    assert!(
        !hints
            .iter()
            .any(|hint| matches!(hint, termrock::widgets::HintSpan::Key("r")))
    );
}

#[test]
fn usage_overview_renders_one_row_per_account_tab() {
    let mut view = usage_view_fixture();
    view.tabs = vec![
        jackin_protocol::control::UsageProviderTab {
            id: "test-tab-claude-a".to_owned(),
            label: "Claude".to_owned(),
            status_label: "40% left".to_owned(),
            account_label: "a@example.com".to_owned(),
            plan_label: Some("Max".to_owned()),
            source_label: Some("fresh · provider".to_owned()),
            active: false,
        },
        jackin_protocol::control::UsageProviderTab {
            id: "test-tab-claude-b".to_owned(),
            label: "Claude".to_owned(),
            status_label: "60% left".to_owned(),
            account_label: "b@example.com".to_owned(),
            plan_label: Some("Max 20x".to_owned()),
            source_label: Some("fresh · provider".to_owned()),
            active: true,
        },
        jackin_protocol::control::UsageProviderTab {
            id: "test-tab-codex".to_owned(),
            label: "Codex".to_owned(),
            status_label: "37% left".to_owned(),
            account_label: "codex@example.com".to_owned(),
            plan_label: Some("Pro 20x".to_owned()),
            source_label: Some("fresh · provider".to_owned()),
            active: false,
        },
    ];
    let strip = crate::tui::components::dialog_widgets::usage_tab_strip_labels(
        &view,
        UsageDialogTab::Overview,
    );
    assert_eq!(
        strip
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        vec!["Overview", "Anthropic", "Anthropic", "OpenAI"]
    );
    let dialog = Dialog::new_usage_with_tab(view, UsageDialogTab::Overview);
    let state = dialog.usage_state().expect("usage state");
    assert_eq!(state.rows().len(), 3);
    assert_eq!(
        state
            .rows()
            .iter()
            .map(|row| row.value().to_owned())
            .collect::<Vec<_>>(),
        vec!["40% left", "60% left", "37% left"]
    );
}

#[test]
fn usage_overview_matches_provider_head_of_composite_tab_labels() {
    use crate::tui::components::dialog_widgets::usage::is_overview_provider_label;

    assert!(is_overview_provider_label("Anthropic"));
    assert!(is_overview_provider_label("Anthropic · a@example.com"));
    assert!(is_overview_provider_label("Cursor · c@example.com"));
    assert!(is_overview_provider_label("OpenCode · o@example.com"));
    assert!(!is_overview_provider_label("Nous Portal · n@example.com"));
    assert!(!is_overview_provider_label("Username"));
}

#[test]
fn usage_provider_tab_renders_meterless_family_bucket_as_plain_row() {
    let text = render_usage_dialog_snapshot_for_view(
        100,
        32,
        UsageDialogTab::Provider,
        antigravity_usage_view_fixture(),
    );
    assert!(
        text.contains("73% left"),
        "metered family bucket must render its percent:\n{text}"
    );
    // Plain label/value row — never the overview join, which glues label and
    // value without a separator ("Gemini · WeeklyNo data").
    assert!(
        text.contains("Gemini · Weekly No data"),
        "meter-less family bucket must render as a plain row:\n{text}"
    );
    assert!(
        !text.contains("WeeklyNo data"),
        "overview-arm misroute must not garble the row:\n{text}"
    );
}

#[test]
fn usage_dialog_renders_auth_source_and_omits_blank_email() {
    // credential_origin, distinct username, and plan stay in Details while the
    // Rust identity projection supplies an honest non-account state above them.
    let mut view = usage_view_fixture();
    view.account = jackin_protocol::control::FocusedAccountHeader {
        provider_label: "Z.AI".to_owned(),
        account_label: String::new(),
        username: Some("donbeave".to_owned()),
        plan_label: Some("GLM Coding".to_owned()),
        credential_origin: Some("API token \u{b7} env ZAI_API_KEY".to_owned()),
    };
    let snapshot = render_usage_dialog_snapshot_for_view(120, 40, UsageDialogTab::Provider, view);
    assert!(
        snapshot.contains("Auth API token \u{b7} env ZAI_API_KEY"),
        "auth source line missing:\n{snapshot}"
    );
    assert!(
        snapshot.contains("Username donbeave"),
        "username detail missing:\n{snapshot}"
    );
    assert!(
        snapshot.contains("Plan GLM Coding"),
        "plan detail missing:\n{snapshot}"
    );
    assert!(
        !snapshot.contains("account unavailable"),
        "blank email must be omitted, not labelled unavailable:\n{snapshot}"
    );
}

#[test]
fn usage_dialog_renders_usage_status_rows_for_error_and_stale_states() {
    let mut values = Vec::new();
    for status in [
        jackin_protocol::control::UsageSnapshotStatus::NeedsLogin,
        jackin_protocol::control::UsageSnapshotStatus::Stale,
        jackin_protocol::control::UsageSnapshotStatus::Unsupported,
        jackin_protocol::control::UsageSnapshotStatus::Error,
    ] {
        let mut view = usage_view_fixture();
        view.status = status;
        let d = Dialog::new_usage(view);
        values.extend(
            d.usage_state()
                .expect("usage state")
                .rows()
                .iter()
                .map(|row| row.value().to_owned()),
        );
    }

    assert!(values.iter().any(|value| value == "Sign in required"));
    assert!(
        values
            .iter()
            .any(|value| value == "Update delayed · Updated now")
    );
    assert!(
        values
            .iter()
            .any(|value| value == "Usage limits unsupported")
    );
    assert!(
        values
            .iter()
            .any(|value| value == "Update failed · Updated now")
    );
}

#[test]
fn usage_dialog_renders_bucket_status_rows_for_error_states() {
    let mut view = usage_view_fixture();
    view.buckets = vec![
        usage_status_bucket(
            "Tokens",
            jackin_protocol::control::UsageSnapshotStatus::NeedsLogin,
        ),
        usage_status_bucket(
            "Weekly",
            jackin_protocol::control::UsageSnapshotStatus::Stale,
        ),
        usage_status_bucket(
            "Credits",
            jackin_protocol::control::UsageSnapshotStatus::Unsupported,
        ),
        usage_status_bucket(
            "Detail",
            jackin_protocol::control::UsageSnapshotStatus::Error,
        ),
    ];
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(crate::tui::components::container_info_surface::ContainerInfoRow::value)
        .collect();

    assert!(values.iter().any(|value| value.contains("needs login")));
    assert!(values.iter().any(|value| value.contains("stale")));
    assert!(values.iter().any(|value| value.contains("unsupported")));
    assert!(values.iter().any(|value| value.contains("error")));
}

#[test]
fn usage_dialog_rows_render_provider_quota_snapshot() {
    let d = Dialog::new_usage(usage_view_fixture());
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(crate::tui::components::container_info_surface::ContainerInfoRow::value)
        .collect();

    assert_eq!(state.rows()[0].label(), "Identity provider");
    assert_eq!(state.rows()[0].value(), "OpenAI");
    assert_eq!(state.rows()[1].label(), "Identity account");
    assert_eq!(state.rows()[1].value(), "alexey@example.com");
    assert_eq!(state.rows()[2].label(), "Identity activity");
    assert_eq!(state.rows()[2].value(), "Updated now");
    assert!(values.iter().any(|value| {
        value.starts_with("████")
            && value.contains("37% left")
            && value.contains("10% in reserve")
            && value.contains("Resets 15:07")
            && !value.contains("used / 100%")
    }));
    assert!(values.contains(&"ACP billing unavailable · unsupported"));
    assert!(!values.contains(&"fresh"));
    let rows_debug = format!("{:?}", state.rows());
    assert!(!rows_debug.contains("Account availability"));
    assert!(!rows_debug.contains("Header"));
    assert!(!rows_debug.contains("Instance"));
    assert!(!values.contains(&"local diagnostic detail"));
}

#[test]
fn usage_dialog_renders_shared_provider_tab_strip_labels() {
    let d = Dialog::new_usage(usage_view_fixture());
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(32, 120);
    let backend = TestBackend::new(120, 32);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..32)
        .map(|y| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Overview"), "{rendered}");
    assert!(rendered.contains("OpenAI"), "{rendered}");
    assert!(rendered.contains("Anthropic"), "{rendered}");
    assert!(rendered.contains("Amp"), "{rendered}");
}
