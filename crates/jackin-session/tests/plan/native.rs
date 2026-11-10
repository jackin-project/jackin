// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, FileMode, SandboxPath};
use jackin_session::{
    KimiLoginMetadata, KimiRegion, plan_kimi_files, render_kimi_auth, render_kimi_config,
};

use super::support::TestResult;

#[test]
fn kimi_config_is_exact_for_each_regional_profile() -> TestResult {
    for (region, base_url, slot, oauth_host) in [
        (
            KimiRegion::MainlandCn,
            "https://api.kimi.com/coding/v1",
            "kimi-code",
            None,
        ),
        (
            KimiRegion::Global,
            "https://api.kimi.ai/coding/v1",
            "kimi-code-env-0e4f99c69cc27850",
            Some("https://auth.kimi.ai"),
        ),
    ] {
        let file = render_kimi_config(region);
        assert_eq!(
            file.path,
            SandboxPath::parse("/sandbox/.kimi-code/config.toml")?
        );
        assert_eq!(file.mode, FileMode(0o600));
        let config: toml::Table = file.text.parse()?;
        assert_eq!(
            config["default_model"].as_str(),
            Some("kimi-code/kimi-for-coding")
        );
        let provider = config["providers"]["managed:kimi-code"]
            .as_table()
            .ok_or("managed provider is not a table")?;
        assert_eq!(provider.len(), 4, "{}", file.text);
        assert_eq!(provider["type"].as_str(), Some("kimi"));
        assert_eq!(provider["base_url"].as_str(), Some(base_url));
        assert_eq!(provider["api_key"].as_str(), Some(""));
        assert!(!provider.contains_key("api_key_env"));
        let oauth = provider["oauth"].as_table().ok_or("oauth is not a table")?;
        assert_eq!(oauth["storage"].as_str(), Some("file"));
        assert_eq!(
            oauth["key"].as_str(),
            Some(format!("oauth/{slot}").as_str())
        );
        assert_eq!(
            oauth.get("oauth_host").and_then(toml::Value::as_str),
            oauth_host
        );
        assert_eq!(oauth.len(), if oauth_host.is_some() { 3 } else { 2 });
        let model = &config["models"]["kimi-for-coding"];
        assert_eq!(model["provider"].as_str(), Some("managed:kimi-code"));
        assert_eq!(model["model"].as_str(), Some("kimi-for-coding"));
        assert_eq!(model["max_context_size"].as_integer(), Some(262_144));
        assert_eq!(
            model["capabilities"],
            toml::Value::Array(
                [
                    "thinking",
                    "always_thinking",
                    "image_in",
                    "video_in",
                    "tool_use"
                ]
                .into_iter()
                .map(|value| toml::Value::String(value.to_owned()))
                .collect()
            )
        );
        for forbidden in ["api_key_env", "jackin-work", "KIMI_API_KEY", "placeholder"] {
            assert!(!file.text.contains(forbidden), "{forbidden}: {}", file.text);
        }
    }
    Ok(())
}

#[test]
fn kimi_auth_preserves_scope_but_contains_no_refresh_grant_or_host_expiry() -> TestResult {
    for (region, path) in [
        (
            KimiRegion::MainlandCn,
            "/sandbox/.kimi-code/credentials/kimi-code.json",
        ),
        (
            KimiRegion::Global,
            "/sandbox/.kimi-code/credentials/kimi-code-env-0e4f99c69cc27850.json",
        ),
    ] {
        let file = render_kimi_auth(
            region,
            "membership.read membership.write",
            "openshell:resolve:KIMI_API_KEY",
        );
        assert_eq!(file.path, SandboxPath::parse(path)?);
        assert_eq!(file.mode, FileMode(0o600));
        let auth: serde_json::Value = serde_json::from_str(&file.text)?;
        let object = auth.as_object().ok_or("auth is not an object")?;
        assert_eq!(object.len(), 6);
        assert_eq!(auth["access_token"], "openshell:resolve:KIMI_API_KEY");
        assert_eq!(auth["refresh_token"], "");
        assert_eq!(auth["expires_at"], 0);
        assert_eq!(auth["expires_in"], 0);
        assert!(auth["expires_at"].is_number());
        assert!(auth["expires_in"].is_number());
        assert_eq!(auth["scope"], "membership.read membership.write");
        assert_eq!(auth["token_type"], "Bearer");
        assert!(!file.text.contains("1800000000"));
        assert!(!file.text.contains("FAKE-REFRESH"));
    }
    assert_eq!(Agent::KimiCode.spec().auth_file.mode, FileMode(0o600));
    Ok(())
}

#[test]
fn kimi_files_relocate_to_lead_and_secondary_homes() -> TestResult {
    let metadata = KimiLoginMetadata {
        region: KimiRegion::Global,
        scope: "membership.read".to_owned(),
    };
    for (home, prefix) in [
        ("/sandbox", "/sandbox/.kimi-code"),
        (
            "/sandbox/.jackin/homes/kimi",
            "/sandbox/.jackin/homes/kimi/.kimi-code",
        ),
    ] {
        let [config, auth] = plan_kimi_files(
            &SandboxPath::parse(home)?,
            &metadata,
            "openshell:resolve:KIMI_API_KEY",
        )?;
        assert_eq!(
            config.path,
            SandboxPath::parse(&format!("{prefix}/config.toml"))?
        );
        assert_eq!(
            auth.path,
            SandboxPath::parse(&format!(
                "{prefix}/credentials/kimi-code-env-0e4f99c69cc27850.json"
            ))?
        );
        assert!(auth.text.contains("membership.read"));
        assert!(!auth.text.contains("FAKE-REFRESH"));
    }
    Ok(())
}
