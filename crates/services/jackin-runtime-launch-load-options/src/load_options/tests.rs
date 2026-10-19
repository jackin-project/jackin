// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{LoadOptions, LoadOptionsError};

#[test]
fn programmatic_whitespace_model_is_rejected() {
    let mut options = LoadOptions {
        non_interactive: true,
        agent: Some(jackin_core::Agent::Codex),
        model: Some("   ".into()),
        effort: Some(jackin_core::ReasoningEffort::Max),
        ..LoadOptions::default()
    };
    let selector = jackin_core::RoleSelector::new(None, "agent-smith");
    let config = jackin_config::AppConfig::default();

    assert_eq!(
        options.validate_programmatic(&config, &selector),
        Err(LoadOptionsError::EmptyModel)
    );

    options.model = Some("gpt-6-luna".into());
    assert_eq!(options.validate_programmatic(&config, &selector), Ok(()));
}
