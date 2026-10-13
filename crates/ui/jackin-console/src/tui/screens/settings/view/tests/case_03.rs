// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn auth_state_lines_appends_scan_status_and_issues() {
    use super::model::AccountScanSummary;
    let mut auth = scan_view_auth();
    auth.scan.last_summary = Some(AccountScanSummary {
        joined: vec!["a".to_owned()],
        skipped: vec!["b".to_owned(), "c".to_owned()],
        fresh_install: true,
    });
    auth.scan.issues = vec![jackin_config::DiscoveryIssue {
        agent: jackin_core::Agent::Codex,
        directory: "/home/op/.codex".into(),
        error: jackin_config::DiscoveryError::Unreadable,
    }];
    let env = scan_view_env();
    let texts = line_texts(&auth_state_lines(&auth, &env, false));
    assert_eq!(
        texts.len(),
        auth.row_count() + auth.scan.status_line_count()
    );
    assert!(
        texts
            .iter()
            .any(|line| line.contains("Scan: joined 1, already present 2 (first run)")),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|line| line
            .contains("Scan issue: codex: credential source cannot be read (/home/op/.codex)")),
        "{texts:?}"
    );
}

#[test]
fn settings_header_does_not_duplicate_active_tab_label() {
    use jackin_config::AppConfig;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_settings_to_dump(state: &crate::tui::state::SettingsState<'_>) -> String {
        let backend = TestBackend::new(90, 18);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|frame| render_settings_with_footer(frame, frame.area(), state, false))
            .unwrap();
        let buf = term.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    let config = AppConfig::default();
    for tab in SettingsTab::ALL {
        let mut state = crate::tui::state::SettingsState::from_config(&config);
        state.active_tab = tab;
        let dump = render_settings_to_dump(&state);
        let header = dump.lines().next().unwrap_or_default();
        assert!(
            header.contains("settings"),
            "settings header missing for {tab:?}: {header:?}"
        );
        assert!(
            !header.contains("settings ·"),
            "settings header must not duplicate active tab for {tab:?}: {header:?}"
        );
    }
}
