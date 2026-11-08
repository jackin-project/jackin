// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, FileMode, Provider, SandboxPath, lookup_tuple};
use jackin_session::{omp_auth_file, omp_models_file, render_omp_config, render_omp_models};
use jackin_testkit::RoleFixture;
use noyalib::Value;

use super::support::{Spec, TestResult, guard};

struct Case {
    provider: Provider,
    id: &'static str,
    env: &'static str,
    base_url: Option<&'static str>,
    api: Option<&'static str>,
    auth_header: bool,
    model: &'static str,
    custom_model: bool,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            provider: Provider::Zai,
            id: "zai",
            env: "ZAI_API_KEY",
            base_url: Some("https://api.z.ai/api/coding/paas/v4"),
            api: None,
            auth_header: false,
            model: "glm-5.3",
            custom_model: false,
        },
        Case {
            provider: Provider::Kimi,
            id: "moonshot",
            env: "KIMI_API_KEY",
            base_url: Some("https://api.kimi.com/coding/v1"),
            api: Some("openai-completions"),
            auth_header: true,
            model: "kimi-for-coding",
            custom_model: true,
        },
        Case {
            provider: Provider::Minimax,
            id: "minimax-code",
            env: "MINIMAX_CODE_API_KEY",
            base_url: None,
            api: None,
            auth_header: false,
            model: "minimax-m3",
            custom_model: false,
        },
        Case {
            provider: Provider::Mimo,
            id: "xiaomi-token-plan-sgp",
            env: "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
            base_url: Some("https://token-plan-sgp.xiaomimimo.com/v1"),
            api: None,
            auth_header: false,
            model: "mimo-v2.6-pro",
            custom_model: false,
        },
        Case {
            provider: Provider::Claude,
            id: "anthropic",
            env: "ANTHROPIC_API_KEY",
            base_url: None,
            api: None,
            auth_header: false,
            model: "haiku",
            custom_model: false,
        },
        Case {
            provider: Provider::Codex,
            id: "openai",
            env: "OPENAI_API_KEY",
            base_url: None,
            api: None,
            auth_header: false,
            model: "gpt-5.1-codex",
            custom_model: false,
        },
    ]
}

fn expected_models(case: &Case) -> Value {
    let mut provider = noyalib::Mapping::new();
    if let Some(url) = case.base_url {
        provider.insert("baseUrl", Value::from(url));
    }
    if let Some(api) = case.api {
        provider.insert("api", Value::from(api));
    }
    provider.insert("apiKey", Value::from(case.env));
    if case.auth_header {
        provider.insert("authHeader", Value::from(true));
    }
    if case.custom_model {
        let mut model = noyalib::Mapping::new();
        model.insert("id", Value::from(case.model));
        provider.insert("models", Value::Sequence(vec![Value::Mapping(model)]));
    }
    let mut providers = noyalib::Mapping::new();
    providers.insert(case.id, Value::Mapping(provider));
    let mut root = noyalib::Mapping::new();
    root.insert("providers", Value::Mapping(providers));
    Value::Mapping(root)
}

#[test]
fn omp_models_yaml_pins_one_provider_the_env_named_key_and_one_model_for_custom_tuples()
-> TestResult {
    for case in cases() {
        let tuple = lookup_tuple(Agent::Omp, case.provider)?;
        let text = render_omp_models(tuple, None)?;
        assert!(!text.contains("placeholder"), "{text}");
        let parsed: Value = noyalib::from_str(&text)?;
        let providers = parsed
            .get("providers")
            .and_then(Value::as_mapping)
            .ok_or("no providers mapping")?;
        assert_eq!(providers.len(), 1, "{text}");
        let entry = providers
            .get(case.id)
            .and_then(Value::as_mapping)
            .ok_or("no provider entry")?;
        assert_eq!(entry.get("apiKey").and_then(Value::as_str), Some(case.env));
        assert_eq!(entry.get("baseUrl").and_then(Value::as_str), case.base_url);
        assert_eq!(entry.get("api").and_then(Value::as_str), case.api);
        assert_eq!(
            entry.get("authHeader").and_then(Value::as_bool),
            case.auth_header.then_some(true)
        );
        let models = entry.get("models").and_then(Value::as_sequence);
        if case.custom_model {
            let models = models.ok_or("no models entry")?;
            assert_eq!(models.len(), 1, "{text}");
        } else {
            assert!(models.is_none(), "{text}");
        }
        assert_eq!(parsed, expected_models(&case), "{text}");
    }
    Ok(())
}

#[test]
fn omp_models_file_pins_the_omp_agent_path_mode_and_text() -> TestResult {
    for case in cases() {
        let tuple = lookup_tuple(Agent::Omp, case.provider)?;
        let file = omp_models_file(tuple, None)?;
        assert_eq!(
            file.path,
            SandboxPath::parse("/sandbox/.omp/agent/models.yml")?
        );
        assert_eq!(file.mode, FileMode(0o600));
        assert_eq!(file.text, render_omp_models(tuple, None)?);
        assert!(!file.text.contains("placeholder"), "{}", file.text);
    }
    Ok(())
}

#[test]
fn omp_config_and_auth_goldens_stay_unchanged_for_every_omp_tuple() -> TestResult {
    let goldens = [
        (
            Provider::Zai,
            "providers:\n  zai:\n    baseUrl: https://api.z.ai/api/coding/paas/v4\n    apiKey: placeholder\ncredentialEnv: ZAI_API_KEY\nmodelRoles:\n  default: zai/glm-5.3\n",
        ),
        (
            Provider::Kimi,
            "providers:\n  moonshot:\n    baseUrl: https://api.kimi.com/coding/v1\n    apiKey: placeholder\ncredentialEnv: KIMI_API_KEY\nmodelRoles:\n  default: moonshot/kimi-for-coding\n",
        ),
        (
            Provider::Minimax,
            "providers:\n  minimax-code:\n    apiKey: placeholder\ncredentialEnv: MINIMAX_CODE_API_KEY\nmodelRoles:\n  default: minimax-code/minimax-m3\n",
        ),
        (
            Provider::Mimo,
            "providers:\n  xiaomi-token-plan-sgp:\n    baseUrl: https://token-plan-sgp.xiaomimimo.com/v1\n    apiKey: placeholder\ncredentialEnv: XIAOMI_TOKEN_PLAN_SGP_API_KEY\nmodelRoles:\n  default: xiaomi-token-plan-sgp/mimo-v2.6-pro\n",
        ),
        (
            Provider::Claude,
            "providers:\n  anthropic:\n    apiKey: placeholder\ncredentialEnv: ANTHROPIC_API_KEY\nmodelRoles:\n  default: anthropic/haiku\n",
        ),
        (
            Provider::Codex,
            "providers:\n  openai:\n    apiKey: placeholder\ncredentialEnv: OPENAI_API_KEY\nmodelRoles:\n  default: openai/gpt-5.1-codex\n",
        ),
    ];
    for (provider, config) in goldens {
        let tuple = lookup_tuple(Agent::Omp, provider)?;
        assert_eq!(render_omp_config(tuple, None)?, config);
        let auth = omp_auth_file(tuple);
        assert_eq!(auth.path, SandboxPath::parse("/sandbox/.omp/auth.json")?);
        assert_eq!(auth.mode, FileMode(0o600));
        let id = tuple.omp_provider_id().ok_or("no provider id")?;
        assert_eq!(
            auth.text,
            format!("{{\"{id}\": {{\"type\": \"api_key\", \"key\": \"placeholder\"}}}}")
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_launch_without_an_omp_instance_plans_no_models_file() -> TestResult {
    guard(async {
        let setup = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let plan = setup.plan(setup.request(1)?)?;
        assert!(plan.files().omp_models.is_none());
        Ok(())
    })
    .await
}
