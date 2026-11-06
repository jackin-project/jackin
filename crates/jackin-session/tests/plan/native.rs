// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{FileMode, ProviderName, SandboxPath};
use jackin_session::{render_kimi_config, render_mimo_config};

use super::support::TestResult;

#[test]
fn kimi_placeholder_config_names_provider_and_env_with_one_placeholder() -> TestResult {
    let provider = ProviderName::parse("jackin-work-kimi")?;
    let file = render_kimi_config(&provider);
    assert_eq!(
        file.path,
        SandboxPath::parse("/sandbox/.kimi-code/config.toml")?
    );
    assert_eq!(file.mode, FileMode(0o600));
    assert!(file.text.contains("jackin-work-kimi"), "{}", file.text);
    assert!(
        file.text.contains("api_key_env = \"KIMI_API_KEY\""),
        "{}",
        file.text
    );
    assert_eq!(file.text.matches("placeholder").count(), 1, "{}", file.text);
    assert!(!file.text.contains("MIMO_API_KEY"), "{}", file.text);
    Ok(())
}

#[test]
fn mimo_placeholder_config_names_provider_and_env_with_one_placeholder() -> TestResult {
    let provider = ProviderName::parse("jackin-work-mimo")?;
    let file = render_mimo_config(&provider);
    assert_eq!(file.path, SandboxPath::parse("/sandbox/.mimo/config.toml")?);
    assert_eq!(file.mode, FileMode(0o600));
    assert!(file.text.contains("jackin-work-mimo"), "{}", file.text);
    assert!(
        file.text.contains("api_key_env = \"MIMO_API_KEY\""),
        "{}",
        file.text
    );
    assert_eq!(file.text.matches("placeholder").count(), 1, "{}", file.text);
    assert!(!file.text.contains("KIMI_API_KEY"), "{}", file.text);
    Ok(())
}
