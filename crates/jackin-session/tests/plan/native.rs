// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, FileMode, ProviderName, SandboxPath};
use jackin_session::{render_kimi_config, render_mimo_config};

use super::support::TestResult;

#[test]
fn kimi_config_renders_the_vendor_model_and_provider_shape() -> TestResult {
    let provider = ProviderName::parse("jackin-work-kimi")?;
    let file = render_kimi_config(&provider);
    assert_eq!(
        file.path,
        SandboxPath::parse("/sandbox/.kimi-code/config.toml")?
    );
    let config: toml::Table = file.text.parse()?;
    assert_eq!(
        config["default_model"].as_str(),
        Some(Agent::KimiCode.spec().default_model)
    );
    let table = config["providers"]["jackin-work-kimi"]
        .as_table()
        .ok_or("providers.jackin-work-kimi is not a table")?;
    assert_eq!(table.len(), 3, "{}", file.text);
    assert_eq!(table["type"].as_str(), Some("kimi"));
    assert_eq!(
        table["base_url"].as_str(),
        Some("https://api.kimi.com/coding/v1")
    );
    assert_eq!(table["api_key_env"].as_str(), Some("KIMI_API_KEY"));
    let model = &config["models"]["kimi-for-coding"];
    assert_eq!(model["provider"].as_str(), Some("jackin-work-kimi"));
    assert_eq!(model["model"].as_str(), Some("kimi-for-coding"));
    assert_eq!(model["max_context_size"].as_integer(), Some(1_048_576));
    assert!(!file.text.contains("placeholder"), "{}", file.text);
    Ok(())
}

#[test]
fn mimo_config_renders_the_vendor_json_at_the_mimocode_path() -> TestResult {
    let file = render_mimo_config();
    assert_eq!(
        file.path,
        SandboxPath::parse("/sandbox/.config/mimocode/mimocode.json")?
    );
    assert_eq!(file.mode, FileMode(0o600));
    let value: serde_json::Value = serde_json::from_str(&file.text)?;
    assert_eq!(value["model"], "xiaomi/mimo-v2.6-pro");
    assert_eq!(value["small_model"], "xiaomi/mimo-v2.6-pro");
    assert_eq!(
        value["provider"]["xiaomi"]["options"]["baseURL"],
        "https://token-plan-sgp.xiaomimimo.com/v1"
    );
    assert_eq!(
        value["provider"]["xiaomi"]["options"]["apiKey"],
        "{env:MIMO_API_KEY}"
    );
    assert!(!file.text.contains("placeholder"), "{}", file.text);
    Ok(())
}
