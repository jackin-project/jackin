// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage view sanitization.

use jackin_protocol::control::FocusedUsageView;

use super::MAX_DISPLAY_CHARS;

pub(crate) fn sanitize_usage_view(mut view: FocusedUsageView) -> FocusedUsageView {
    view.focused_agent = view.focused_agent.map(|value| sanitize_text(&value));
    view.focused_provider = view.focused_provider.map(|value| sanitize_text(&value));
    view.account.provider_label = sanitize_text(&view.account.provider_label);
    view.account.account_label = sanitize_text(&view.account.account_label);
    view.account.username = view.account.username.map(|value| sanitize_text(&value));
    view.account.plan_label = view.account.plan_label.map(|value| sanitize_text(&value));
    view.account.credential_origin = view
        .account
        .credential_origin
        .map(|value| sanitize_text(&value));
    view.updated_label = sanitize_text(&view.updated_label);
    view.status_bar_label = sanitize_text(&view.status_bar_label);
    view.last_error = view.last_error.map(|value| sanitize_text(&value));
    for bucket in &mut view.buckets {
        bucket.label = sanitize_text(&bucket.label);
        bucket.used_label = bucket.used_label.take().map(|value| sanitize_text(&value));
        bucket.limit_label = bucket.limit_label.take().map(|value| sanitize_text(&value));
        bucket.reset_label = bucket.reset_label.take().map(|value| sanitize_text(&value));
        bucket.pace_label = bucket.pace_label.take().map(|value| sanitize_text(&value));
        if let Some(money) = &mut bucket.used_money {
            money.currency = sanitize_text(&money.currency);
        }
        if let Some(money) = &mut bucket.limit_money {
            money.currency = sanitize_text(&money.currency);
        }
    }
    for tab in &mut view.tabs {
        tab.label = sanitize_text(&tab.label);
        tab.status_label = sanitize_text(&tab.status_label);
        tab.account_label = sanitize_text(&tab.account_label);
        tab.plan_label = tab.plan_label.take().map(|value| sanitize_text(&value));
        tab.source_label = tab.source_label.take().map(|value| sanitize_text(&value));
    }
    view
}

pub(crate) fn sanitize_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_DISPLAY_CHARS)
        .collect()
}
