// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, CatalogError, FileMode, Provider, SandboxPath, lookup_tuple};
use jackin_session::{omp_auth_file, render_omp_config};

use super::support::TestResult;

fn cases() -> Vec<(Provider, &'static str, &'static str, &'static str)> {
    vec![
        (Provider::Zai, "zai", "ZAI_API_KEY", "glm-5.3"),
        (Provider::Kimi, "moonshot", "KIMI_API_KEY", "kimi-for-coding"),
        (
            Provider::Minimax,
            "minimax-code",
            "MINIMAX_CODE_API_KEY",
            "minimax-m3",
        ),
        (
            Provider::Mimo,
            "xiaomi-token-plan-sgp",
            "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
            "mimo-v2.6-pro",
        ),
    ]
}

fn base_urls() -> Vec<(Provider, Option<&'static str>)> {
    vec![
        (
            Provider::Zai,
            Some("https://api.z.ai/api/coding/paas/v4"),
        ),
        (Provider::Kimi, Some("https://api.kimi.com/coding/v1")),
        (Provider::Minimax, None),
        (
            Provider::Mimo,
            Some("https://token-plan-sgp.xiaomimimo.com/v1"),
        ),
    ]
}

#[test]
fn omp_config_sets_the_exact_plan_base_url_per_tuple() -> TestResult {
    for (provider, url) in base_urls() {
        let tuple = lookup_tuple(Agent::Omp, provider)?;
        let text = render_omp_config(tuple, None)?;
        match url {
            Some(url) => assert!(text.contains(&format!("    baseUrl: {url}\n")), "{text}"),
            None => assert!(!text.contains("baseUrl"), "{text}"),
        }
    }
    Ok(())
}

#[test]
fn omp_config_names_each_tuples_provider_env_model_and_one_placeholder() -> TestResult {
    for (provider, id, env, model) in cases() {
        let tuple = lookup_tuple(Agent::Omp, provider)?;
        let text = render_omp_config(tuple, None)?;
        assert!(text.contains(&format!("  {id}:\n")), "{text}");
        assert!(text.contains(&format!("credentialEnv: {env}\n")), "{text}");
        assert!(
            text.contains(&format!("default: {id}/{model}\n")),
            "{text}"
        );
        assert_eq!(text.matches("placeholder").count(), 1, "{text}");
        assert!(!text.contains("MINIMAX_API_KEY"), "{text}");
        assert!(!text.contains("MIMO_API_KEY"), "{text}");
    }
    Ok(())
}

#[test]
fn omp_config_unknown_model_is_a_typed_error() -> TestResult {
    let tuple = lookup_tuple(Agent::Omp, Provider::Zai)?;
    let err = render_omp_config(tuple, Some("no-such-model")).err().ok_or("expected error")?;
    assert!(
        matches!(err, CatalogError::UnknownModel { ref model, .. } if model == "no-such-model"),
        "{err}"
    );
    Ok(())
}

#[test]
fn omp_auth_file_holds_exactly_one_account_at_mode_0600() -> TestResult {
    for (provider, id, _, _) in cases() {
        let tuple = lookup_tuple(Agent::Omp, provider)?;
        let file = omp_auth_file(tuple);
        assert_eq!(file.path, SandboxPath::parse("/sandbox/.omp/auth.json")?);
        assert_eq!(file.mode, FileMode(0o600));
        assert_eq!(file.text.matches("placeholder").count(), 1, "{}", file.text);
        assert!(file.text.contains(&format!("\"{id}\"")), "{}", file.text);
        for (other, other_id, _, _) in cases() {
            if other != provider {
                assert!(!file.text.contains(other_id), "{}", file.text);
            }
        }
    }
    Ok(())
}
