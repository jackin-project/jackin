// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, FileMode, Provider, SandboxPath, TupleState, lookup_tuple,
};
use jackin_session::{plan_static_files, render_grok_config};
use serde_json::{Value, json};
use tokio::process::Command;

use super::support::TestResult;

const HELPER: &str = "jq -n -e --arg access_token \"${GROK_SESSION_TOKEN-}\" '$access_token | select(test(\"^openshell:resolve:[^\\\\r\\\\n]+$\")) | {access_token: ., issuer: \"https://auth.x.ai\"}'";

fn instance() -> Result<AgentInstance, Box<dyn std::error::Error>> {
    let tuple = lookup_tuple(Agent::GrokBuild, Provider::Grok)?;
    Ok(AgentInstance::new(
        tuple.agent,
        tuple.provider,
        AccountName::parse("grok")?,
        tuple.default_model.to_owned(),
    ))
}

fn helper_command() -> Result<String, Box<dyn std::error::Error>> {
    let file = render_grok_config(Some("grok-4.7"));
    let config: toml::Value = toml::from_str(&file.text)?;
    Ok(config["auth"]["auth_provider_command"]
        .as_str()
        .ok_or("missing [auth] external session configuration")?
        .to_owned())
}

#[test]
fn grok_config_uses_external_session_auth_without_an_api_key_fallback() -> TestResult {
    let tuple = lookup_tuple(Agent::GrokBuild, Provider::Grok)?;
    let file = render_grok_config(Some("grok-4.7"));
    assert_eq!(file.mode, FileMode(0o600));
    let config: toml::Value = toml::from_str(&file.text)?;
    assert_eq!(config["cli"]["auto_update"].as_bool(), Some(false));
    assert_eq!(config["models"]["default"].as_str(), Some("grok-4.7"));
    assert_eq!(config["auth"]["disable_api_key_auth"].as_bool(), Some(true));
    assert_eq!(helper_command()?, HELPER);
    assert!(!file.text.contains("XAI_API_KEY"));
    assert!(!file.text.contains("refresh_token"));
    assert_eq!(tuple.credential_env(), Some("GROK_SESSION_TOKEN"));
    assert_eq!(tuple.state, TupleState::Supported);
    assert_eq!(tuple.reason, None);
    assert_eq!(tuple.agent.spec().launch_argv(), ["grok", "--trust"]);
    Ok(())
}

#[test]
fn grok_static_files_cover_lead_and_secondary_homes_without_generic_auth_json() -> TestResult {
    let instance = instance()?;
    for (home, expected) in [
        ("/sandbox", "/sandbox/.grok/config.toml"),
        (
            "/sandbox/.jackin/homes/grok",
            "/sandbox/.jackin/homes/grok/.grok/config.toml",
        ),
    ] {
        let files = plan_static_files(
            &instance,
            &SandboxPath::parse(home)?,
            &SandboxPath::from_static("/sandbox/project"),
            None,
        )?;
        assert_eq!(files.len(), 1);
        let config = &files[0];
        assert_eq!(config.path.as_str(), expected);
        assert_eq!(config.mode, FileMode(0o600));
        assert_eq!(config.text, render_grok_config(Some("grok-4.7")).text);
        assert!(!config.path.as_str().ends_with("auth.json"));
        assert!(
            !config
                .text
                .contains("GROK_SESSION_TOKEN\": \"placeholder\"")
        );
    }
    Ok(())
}

#[tokio::test]
async fn grok_helper_emits_only_session_fields_and_rejects_malformed_placeholders() -> TestResult {
    let command = helper_command()?;
    let valid = Command::new("/bin/sh")
        .arg("-c")
        .arg(&command)
        .env(
            "GROK_SESSION_TOKEN",
            "openshell:resolve:env:GROK_SESSION_TOKEN",
        )
        .output()
        .await?;
    assert!(valid.status.success(), "{valid:?}");
    let output: Value = serde_json::from_slice(&valid.stdout)?;
    assert_eq!(
        output,
        json!({
            "access_token": "openshell:resolve:env:GROK_SESSION_TOKEN",
            "issuer": "https://auth.x.ai"
        })
    );
    assert_eq!(output.as_object().map(serde_json::Map::len), Some(2));

    for value in [
        "",
        "placeholder",
        "raw-FAKE-GROK-SESSION",
        "openshell:resolve:",
        "x-openshell:resolve:env:GROK_SESSION_TOKEN",
        "openshell:resolve:env:GROK_SESSION_TOKEN\nraw-FAKE-GROK-SESSION",
    ] {
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(&command)
            .env("GROK_SESSION_TOKEN", value)
            .output()
            .await?;
        assert!(!output.status.success(), "{value:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{value:?}: {output:?}");
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("raw-FAKE-GROK-SESSION"),
            "{value:?}"
        );
    }
    let missing = Command::new("/bin/sh")
        .arg("-c")
        .arg(&command)
        .env_remove("GROK_SESSION_TOKEN")
        .output()
        .await?;
    assert!(!missing.status.success(), "{missing:?}");
    assert!(missing.stdout.is_empty(), "{missing:?}");
    Ok(())
}
