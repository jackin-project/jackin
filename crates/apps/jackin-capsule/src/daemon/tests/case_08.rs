// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::dialog::{Dialog, DialogAction};
use crate::tui::layout::Tab;
use crate::tui::message::Action;
use jackin_protocol::control::{
    ClientMsg, FocusedAccountHeader, FocusedUsageView, QuotaBucketView, ServerMsg, UsageConfidence,
    UsageSeverity, UsageSnapshotStatus, UsageSource,
};

fn capability() -> jackin_protocol::usage_broker::UsageAccountCapability {
    jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "operator@example.com".to_owned(),
        surface_id: "codex-cli".to_owned(),
    }
}

fn cached_view() -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("web".to_owned()),
        focused_provider: None,
        account: FocusedAccountHeader {
            provider_label: "OpenAI".to_owned(),
            account_label: "operator@example.com".to_owned(),
            account_capability: None,
            username: None,
            plan_label: None,
            credential_origin: None,
        },
        buckets: vec![QuotaBucketView {
            label: "weekly".to_owned(),
            used_label: None,
            limit_label: None,
            used_percent: None,
            used_percent_origin: None,
            remaining_percent: None,
            remaining_percent_origin: None,
            count_quota: None,
            provider_quota_state: None,
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        }],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::Cache,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 0,
        updated_label: "seed".to_owned(),
        status_bar_label: String::new(),
        tabs: vec![],
        last_error: None,
    }
}

fn focused_session(mux: &mut super::super::Multiplexer) {
    let id = super::spawn_sleep_session(mux);
    let session = mux
        .session_supervisor
        .sessions
        .get_mut(id)
        .expect("spawned session");
    session.agent = Some("web".to_owned());
    session.usage_capability = Some(capability());
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("t", id, "codename-q"));
}

fn usage_view(mux: &mut super::super::Multiplexer) -> FocusedUsageView {
    mux.dialog_top()
        .and_then(|dialog| match dialog {
            Dialog::Usage { view, .. } => Some(view.as_ref().clone()),
            _ => None,
        })
        .expect("usage dialog open")
}

#[tokio::test]
async fn apply_action_open_usage_queues_focused_provider_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    focused_session(&mut mux);
    mux.apply_action(Action::OpenUsage);
    assert!(matches!(mux.dialog_top(), Some(Dialog::Usage { .. })));
    assert!(mux.usage.pending_usage_refresh.is_some());
    let id = mux.active_focused_id().expect("focused session");
    super::terminate_session(&mut mux, id);
}

#[tokio::test]
async fn apply_dialog_action_refresh_usage_queues_refresh_without_replacing_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    focused_session(&mut mux);
    mux.apply_action(Action::OpenUsage);
    let before = usage_view(&mut mux);
    mux.apply_dialog_action(DialogAction::RefreshUsage);
    assert!(mux.usage.pending_usage_refresh.is_some());
    assert_eq!(usage_view(&mut mux), before);
    let id = mux.active_focused_id().expect("focused session");
    super::terminate_session(&mut mux, id);
}

#[test]
fn control_usage_account_list_uses_in_memory_cache() {
    let dir = tempfile::tempdir().unwrap();
    let mut mux = super::test_mux(dir.path());
    let reply = super::super::control_reply_for_request(&mut mux, ClientMsg::UsageAccountList);
    match reply {
        ServerMsg::UsageAccounts { accounts } => assert!(accounts.is_empty()),
        reply => panic!("expected usage accounts, got {reply:?}"),
    }
    mux.usage
        .usage_cache
        .insert_snapshot_for_capability_for_test("web", None, &capability(), cached_view());
    let reply = super::super::control_reply_for_request(&mut mux, ClientMsg::UsageAccountList);
    match reply {
        ServerMsg::UsageAccounts { accounts } => {
            assert_eq!(accounts.len(), 1);
            assert_eq!(accounts[0].account_capability, Some(capability()));
            assert_eq!(accounts[0].account_label, "operator@example.com");
        }
        reply => panic!("expected usage accounts, got {reply:?}"),
    }
}
