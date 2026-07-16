// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
#[ignore = "accelerated lifecycle soak runs in the scheduled soak profile"]
fn soak_week_long_console_has_only_bounded_operations() {
    let mut screens = jackin_telemetry::ui::ScreenVisitTracker::new();
    let mut sessions = std::collections::BTreeSet::new();

    for index in 0..10_000 {
        screens
            .enter(jackin_telemetry::schema::enums::ScreenId::WorkspaceList)
            .unwrap();
        let action = jackin_telemetry::root_operation(
            &jackin_telemetry::operation::UI_ACTION,
            &[jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::UI_ACTION_NAME,
                value: jackin_telemetry::Value::Str("workspace.open"),
            }],
        )
        .unwrap();
        action.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        screens
            .exit(jackin_telemetry::schema::enums::TransitionReason::Action)
            .unwrap();

        if index % 100 == 0 {
            let session = jackin_telemetry::identity::SessionGuard::begin(
                jackin_telemetry::identity::SessionKind::Console,
            )
            .unwrap();
            sessions.insert(session.context().current.to_string());
            drop(session);
        }
    }

    assert_eq!(screens.sequence(), 10_000);
    assert_eq!(screens.current_screen(), None);
    assert_eq!(sessions.len(), 100);
    assert_eq!(jackin_telemetry::identity::current_session(), None);
}
