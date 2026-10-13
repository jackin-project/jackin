// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn breadcrumb_auth_mode_uses_single_account_directly() {
    use jackin_config::AuthForwardMode;
    let (mut config, ws) = breadcrumb_config();
    config.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["c-codex".into()];
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .default_launch = None;
    let mode = breadcrumb_auth_mode(&config, Agent::Codex, Some(&ws), "smith").unwrap();
    assert_eq!(mode, AuthForwardMode::ApiKey);
}

#[test]
fn breadcrumb_auth_mode_falls_back_to_first_admitted_instance() {
    use jackin_config::AuthForwardMode;
    let (config, ws) = breadcrumb_config();
    // Two Claude accounts: no single account resolves, but the breadcrumb
    // must not fail the launch — it reports the first admitted instance.
    jackin_config::resolve_account(&config, Agent::Claude, Some(&ws), "smith")
        .expect_err("two claude accounts must not resolve to one");
    let mode = breadcrumb_auth_mode(&config, Agent::Claude, Some(&ws), "smith").unwrap();
    assert_eq!(mode, AuthForwardMode::ApiKey);
}

#[test]
fn breadcrumb_auth_mode_ignores_agents_with_no_admitted_instance() {
    use jackin_config::AuthForwardMode;
    let (mut config, ws) = breadcrumb_config();
    config
        .workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .default_launch = Some(vec!["codex-c".into()]);
    // Claude is still ambiguous (two authorized accounts) while the launch
    // admits only Codex, so the Claude breadcrumb reports `Ignore`.
    let mode = breadcrumb_auth_mode(&config, Agent::Claude, Some(&ws), "smith").unwrap();
    assert_eq!(mode, AuthForwardMode::Ignore);
}
