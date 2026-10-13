// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn supported_modes_opencode_excludes_oauth_token() {
    let modes = Agent::Opencode.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_new_agents_are_sync_api_key_ignore() {
    for agent in [
        Agent::Antigravity,
        Agent::Gemini,
        Agent::Cursor,
        Agent::Muse,
        Agent::Omp,
        Agent::Hermes,
    ] {
        assert_eq!(
            agent.supported_modes(),
            &[
                AuthForwardMode::Sync,
                AuthForwardMode::ApiKey,
                AuthForwardMode::Ignore
            ],
            "{agent:?}",
        );
    }
}
