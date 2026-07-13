// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Phase 1 correctness gate: `Agent::runtime()` must produce byte-identical
//! output to the existing direct `Agent` methods.  If any adapter drifts from
//! the canonical enum behavior these tests catch it.

use crate::agent::Agent;
use crate::auth::AuthForwardMode;

#[test]
fn runtime_slug_matches_agent_slug() {
    for agent in Agent::ALL {
        assert_eq!(
            agent.runtime().slug(),
            agent.slug(),
            "Agent::{agent:?}: runtime().slug() != slug()"
        );
    }
}

#[test]
fn runtime_label_matches_agent_label() {
    for agent in Agent::ALL {
        assert_eq!(
            agent.runtime().label(),
            agent.label(),
            "Agent::{agent:?}: runtime().label() != label()"
        );
    }
}

#[test]
fn runtime_supported_modes_match_agent() {
    for agent in Agent::ALL {
        let direct = agent.supported_modes();
        let via_runtime = agent.runtime().supported_modes();
        assert_eq!(
            direct, via_runtime,
            "Agent::{agent:?}: supported_modes mismatch"
        );
    }
}

#[test]
fn runtime_required_env_var_matches_agent() {
    let modes = [
        AuthForwardMode::Sync,
        AuthForwardMode::ApiKey,
        AuthForwardMode::OAuthToken,
        AuthForwardMode::Ignore,
    ];
    for agent in Agent::ALL {
        for mode in modes {
            assert_eq!(
                agent.required_env_var(mode),
                agent.runtime().required_env_var(mode),
                "Agent::{agent:?} mode {mode:?}: required_env_var mismatch"
            );
        }
    }
}

#[test]
fn runtime_install_block_matches_agent() {
    let source = "agent-binary/claude";
    for agent in Agent::ALL {
        assert_eq!(
            agent.install_block(source),
            agent.runtime().install_block(source),
            "Agent::{agent:?}: install_block mismatch"
        );
    }
}

#[test]
fn runtime_fallback_install_block_matches_agent() {
    for agent in Agent::ALL {
        assert_eq!(
            agent.fallback_install_block(),
            agent.runtime().fallback_install_block(),
            "Agent::{agent:?}: fallback_install_block mismatch"
        );
    }
}

#[test]
fn runtime_fallback_install_command_matches_agent() {
    for agent in Agent::ALL {
        assert_eq!(
            agent.fallback_install_command(),
            agent.runtime().fallback_install_command(),
            "Agent::{agent:?}: fallback_install_command mismatch"
        );
    }
}

#[test]
fn state_paths_have_sensible_structure() {
    for agent in Agent::ALL {
        let paths = agent.runtime().state_paths();
        // Credential dirs should not be empty.
        assert!(
            !paths.credential_dir.is_empty(),
            "Agent::{agent:?}: credential_dir must not be empty"
        );
        // If a file is named, it should be under the dir.
        if let Some(file) = paths.credential_file {
            assert!(
                file.starts_with(paths.credential_dir),
                "Agent::{agent:?}: credential_file {:?} must be under credential_dir {:?}",
                file,
                paths.credential_dir
            );
        }
    }
}

#[test]
fn amp_state_paths_describe_xdg_data_store() {
    let paths = Agent::Amp.runtime().state_paths();

    assert_eq!(paths.credential_dir, ".local/share/amp");
    assert_eq!(paths.credential_file, Some(".local/share/amp/secrets.json"));
    assert_eq!(paths.folder_env_var, Some("XDG_DATA_HOME"));
}
