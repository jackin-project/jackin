// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn picker(agents: Vec<&str>) -> Dialog {
    // Mirror the daemon's construction site: `Dialog::new_agent_picker`
    // computes the initial `selected` past the leading `"agents"`
    // section row. Tests that explicitly want a different starting
    // selection construct `Dialog::AgentPicker { … }` inline.
    Dialog::new_agent_picker(
        agents.into_iter().map(String::from).collect(),
        PickerIntent::NewTab,
    )
}

pub(super) fn palette_with(selected: usize, filter: impl Into<String>) -> Dialog {
    Dialog::CommandPalette {
        selected,
        filter: filter.into(),
        close_label: PaletteCloseLabel::ChooseTarget,
    }
}

pub(super) fn palette() -> Dialog {
    palette_with(0, String::new())
}

pub(super) fn container_info_fixture() -> Dialog {
    Dialog::ContainerInfo {
        container_name: "jk-abc123-thearchitect".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace/jackin".to_owned(),
        diagnostics: ContainerInfoDiagnostics::default(),
        copied_row: None,
        hovered_row: None,
        scroll: termrock::scroll::DialogScroll::new(),
    }
}

pub(super) fn container_info_with_diagnostics_fixture() -> Dialog {
    Dialog::ContainerInfo {
        container_name: "jk-abc123-thearchitect".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace/jackin".to_owned(),
        diagnostics: ContainerInfoDiagnostics {
            host_version: "0.6.0-test".to_owned(),
            invocation_id: "jk-inv-b93735".to_owned(),
        },
        copied_row: None,
        hovered_row: None,
        scroll: termrock::scroll::DialogScroll::new(),
    }
}

pub(super) fn visible_cell_for_value(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    term_rows: u16,
    term_cols: u16,
    area: Rect,
    needle: &str,
) -> (u16, u16) {
    let backend = TestBackend::new(term_cols, term_rows);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            crate::tui::components::container_info_surface::render_container_info(
                frame, area, state,
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let needle_chars: Vec<char> = needle.chars().collect();
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if needle_chars.iter().enumerate().all(|(offset, ch)| {
                let Ok(offset) = u16::try_from(offset) else {
                    return false;
                };
                x.saturating_add(offset) < area.x.saturating_add(area.width)
                    && buf[(x.saturating_add(offset), y)].symbol() == ch.to_string()
            }) {
                return (y, x);
            }
        }
    }
    let mut rows = Vec::new();
    for y in area.y..area.y.saturating_add(area.height) {
        let row_text = (area.x..area.x.saturating_add(area.width))
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>();
        rows.push(row_text);
    }
    panic!(
        "visible value {needle:?} not found in rendered container info:\n{}",
        rows.join("\n")
    );
}

pub(super) fn pull_request_fixture() -> PullRequestInfo {
    PullRequestInfo {
        number: 123,
        title: "Surface PR context in Capsule".to_owned(),
        url: "https://github.com/jackin-project/jackin/pull/123".to_owned(),
        is_draft: false,
        checks: None,
    }
}

pub(super) const GITHUB_FIXTURE_BRANCH: &str = "feature/container-info";

pub(super) fn github_view_for_fixture(pr: &PullRequestInfo) -> GithubContextView<'_> {
    GithubContextView {
        branch: Some(GITHUB_FIXTURE_BRANCH),
        status: PullRequestStatus::Loaded(pr),
    }
}

pub(super) fn usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    jackin_protocol::control::FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: jackin_protocol::control::FocusedAccountHeader {
            provider_label: "OpenAI / Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: None,
        },
        buckets: vec![
            jackin_protocol::control::QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets 15:07".to_owned()),
                resets_at: None,
                status_slot: None,
                pace_label: Some("10% in reserve".to_owned()),
                status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
            },
            jackin_protocol::control::QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Credits".to_owned(),
                used_label: None,
                limit_label: None,
                remaining_percent: None,
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: Some("ACP billing unavailable".to_owned()),
                status: jackin_protocol::control::UsageSnapshotStatus::Unsupported,
            },
        ],
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        source: jackin_protocol::control::UsageSource::Cli,
        confidence: jackin_protocol::control::UsageConfidence::Authoritative,
        fetched_at_epoch: 1_781_185_560,
        updated_label: "Updated now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: vec![
            jackin_protocol::control::UsageProviderTab {
                id: "test-tab-codex".to_owned(),
                label: "Codex".to_owned(),
                status_label: "37% left · Resets in 1h 21m (Jun 17, 23:15)".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("Pro 20x".to_owned()),
                source_label: Some("fresh · provider".to_owned()),
                active: true,
            },
            jackin_protocol::control::UsageProviderTab {
                id: "test-tab-claude".to_owned(),
                label: "Claude".to_owned(),
                status_label: "16% left · Resets in 46m (Jun 17, 22:40)".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("Max".to_owned()),
                source_label: Some("stale · provider".to_owned()),
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                id: "test-tab-amp".to_owned(),
                label: "Amp".to_owned(),
                status_label: "unsupported".to_owned(),
                account_label: "account unavailable".to_owned(),
                plan_label: None,
                source_label: None,
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                id: "test-tab-grok".to_owned(),
                label: "Grok Build".to_owned(),
                status_label: "needs login".to_owned(),
                account_label: "account unavailable".to_owned(),
                plan_label: None,
                source_label: Some("needs-login · provider".to_owned()),
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                id: "test-tab-zai".to_owned(),
                label: "GLM / Z.AI".to_owned(),
                status_label: "88% left · Resets in 4d (Jun 21, 00:00)".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("GLM Coding".to_owned()),
                source_label: Some("fresh · provider".to_owned()),
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                id: "test-tab-kimi".to_owned(),
                label: "Kimi".to_owned(),
                status_label: "72% left · Resets in 13h (Jun 18, 11:00)".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("Moonshot".to_owned()),
                source_label: Some("fresh · provider".to_owned()),
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                id: "test-tab-minimax".to_owned(),
                label: "MiniMax".to_owned(),
                status_label: "100% left".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("M1 Coding".to_owned()),
                source_label: Some("fresh · provider".to_owned()),
                active: false,
            },
        ],
        last_error: None,
    }
}

pub(super) fn usage_status_bucket(
    label: &str,
    status: jackin_protocol::control::UsageSnapshotStatus,
) -> jackin_protocol::control::QuotaBucketView {
    jackin_protocol::control::QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::default(),
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status,
    }
}

pub(super) fn quota_bucket(
    label: &str,
    remaining_percent: u8,
    reset_label: Option<&str>,
    pace_label: Option<&str>,
) -> jackin_protocol::control::QuotaBucketView {
    jackin_protocol::control::QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::default(),
        label: label.to_owned(),
        used_label: Some(format!("{}% used", 100u8.saturating_sub(remaining_percent))),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(remaining_percent),
        reset_label: reset_label.map(str::to_owned),
        resets_at: None,
        status_slot: None,
        pace_label: pace_label.map(str::to_owned),
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
    }
}

pub(super) fn text_bucket(label: &str, value: &str) -> jackin_protocol::control::QuotaBucketView {
    jackin_protocol::control::QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::default(),
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: Some(value.to_owned()),
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
    }
}

pub(super) fn provider_usage_view_fixture(
    tab_label: &str,
    provider_label: &str,
    account_label: &str,
    plan_label: Option<&str>,
    updated_label: &str,
    buckets: Vec<jackin_protocol::control::QuotaBucketView>,
) -> jackin_protocol::control::FocusedUsageView {
    let mut view = usage_view_fixture();
    view.focused_provider = Some(provider_label.to_owned());
    view.account = jackin_protocol::control::FocusedAccountHeader {
        provider_label: provider_label.to_owned(),
        account_label: account_label.to_owned(),
        username: None,
        plan_label: plan_label.map(str::to_owned),
        credential_origin: None,
    };
    view.updated_label = updated_label.to_owned();
    view.buckets = buckets;
    for tab in &mut view.tabs {
        tab.active = tab.label == tab_label;
    }
    view
}

pub(super) fn openai_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    let mut credits = quota_bucket("Credits", 0, None, None);
    credits.used_label = None;
    credits.limit_label = Some("1K tokens".to_owned());
    provider_usage_view_fixture(
        "Codex",
        "OpenAI",
        "account@work.test",
        Some("Pro 20x"),
        "Updated 1m ago",
        vec![
            quota_bucket("Session", 97, Some("Resets 19:45"), Some("33% in reserve")),
            quota_bucket(
                "Weekly",
                19,
                Some("Resets tomorrow, 04:18"),
                Some("12% in reserve"),
            ),
            quota_bucket("Codex Spark 5-hour", 100, Some("Resets 21:31"), None),
            quota_bucket(
                "Codex Spark Weekly",
                100,
                Some("Resets Jul 1 at 16:31"),
                None,
            ),
            text_bucket(
                "Limit Reset Credits",
                "2 manual resets available · Next expires Jul 12 at 08:14",
            ),
            credits,
        ],
    )
}

pub(super) fn anthropic_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Claude",
        "Anthropic",
        "account@work.test",
        Some("Max"),
        "Updated 2m ago",
        vec![
            quota_bucket(
                "Session",
                89,
                Some("Resets in 2h 12m (Jun 17, 19:19)"),
                Some("34% in reserve"),
            ),
            // limits-array shape: weekly_all is labelled "All models", and a
            // model-scoped window (Fable) renders as its own non-headline row.
            quota_bucket(
                "All models",
                55,
                Some("Resets in 1w 1d (Jun 26, 13:59)"),
                Some("28% in reserve"),
            ),
            quota_bucket("Fable", 57, Some("Resets in 1w 1d (Jun 26, 13:59)"), None),
            quota_bucket("Sonnet", 85, Some("Resets in 1w 1d (Jun 26, 13:59)"), None),
        ],
    )
}
