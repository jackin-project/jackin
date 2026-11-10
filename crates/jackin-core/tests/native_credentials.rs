// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, AuthClass, ConfigError, CredentialSource, EnvName, HostPath,
    Provider, TUPLES, TupleState, lookup_tuple,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn account(
    provider: Provider,
    credential: CredentialSource,
) -> Result<Account, Box<dyn std::error::Error>> {
    Ok(Account::new(
        AccountName::parse("native")?,
        provider,
        credential,
    )?)
}

fn file() -> Result<CredentialSource, Box<dyn std::error::Error>> {
    Ok(CredentialSource::File {
        path: HostPath::parse(std::path::Path::new("/tmp/native-key"))?,
    })
}

fn host() -> Result<CredentialSource, Box<dyn std::error::Error>> {
    Ok(CredentialSource::HostLogin {
        path: HostPath::parse(std::path::Path::new("/tmp/native-home"))?,
    })
}

#[test]
fn native_subscription_tuples_stay_unavailable_with_oauth_reasons() -> TestResult {
    let grok = lookup_tuple(Agent::GrokBuild, Provider::Grok)?;
    assert_eq!(grok.credential_env(), Some("GROK_SESSION_TOKEN"));
    assert_eq!(
        grok.agent.spec().credential_env_names,
        ["GROK_SESSION_TOKEN"]
    );
    for (agent, provider, reason) in [
        (
            Agent::MinimaxCode,
            Provider::Minimax,
            "no stable Linux artifact",
        ),
        (
            Agent::MimoCode,
            Provider::Mimo,
            "native login has no refresh grant",
        ),
        (
            Agent::KimiCode,
            Provider::Kimi,
            "native refresh and final live proof pending",
        ),
    ] {
        let tuple = lookup_tuple(agent, provider)?;
        assert_eq!(tuple.auth_class, AuthClass::OAuth);
        assert_eq!(agent.spec().auth_class, AuthClass::OAuth);
        assert_eq!(tuple.state, TupleState::Unavailable);
        assert_eq!(tuple.reason, Some(reason));
    }
    Ok(())
}

#[test]
fn t06_through_t08_accept_only_host_logins() -> TestResult {
    for (agent, provider) in [
        (Agent::MinimaxCode, Provider::Minimax),
        (Agent::MimoCode, Provider::Mimo),
        (Agent::KimiCode, Provider::Kimi),
    ] {
        account(provider, host()?)?.check_credential_eligibility(agent)?;
        for credential in [
            CredentialSource::Env {
                var: EnvName::parse("NATIVE_TEST_KEY")?,
            },
            file()?,
            CredentialSource::OmpHost,
        ] {
            let error = account(provider, credential)?
                .check_credential_eligibility(agent)
                .expect_err("static source must not serve a native subscription");
            assert!(matches!(
                error,
                ConfigError::CredentialNotEligible { account, agent: found }
                    if account.as_str() == "native" && found == agent
            ));
        }
    }
    Ok(())
}

#[test]
fn native_logins_never_serve_static_t09_through_t20() -> TestResult {
    for tuple in TUPLES.iter().filter(|tuple| {
        tuple
            .id
            .strip_prefix('T')
            .and_then(|id| id.parse::<u8>().ok())
            .is_some_and(|id| (9..=20).contains(&id))
    }) {
        account(tuple.provider, file()?)?.check_credential_eligibility(tuple.agent)?;
        let error = account(tuple.provider, host()?)?
            .check_credential_eligibility(tuple.agent)
            .expect_err("native host login served a static tuple");
        assert!(matches!(
            error,
            ConfigError::CredentialNotEligible { agent, .. } if agent == tuple.agent
        ));
    }
    Ok(())
}

#[test]
fn kimi_profiles_have_only_their_regional_refresh_configuration() {
    for (yaml, id, api, auth) in [
        (
            include_str!("../src/assets/t08-kimi.yaml"),
            "kimi",
            "api.kimi.com",
            "auth.kimi.com",
        ),
        (
            include_str!("../src/assets/t08-kimi-global.yaml"),
            "kimi-global",
            "api.kimi.ai",
            "auth.kimi.ai",
        ),
    ] {
        assert!(yaml.contains(&format!("id: {id}\n")));
        assert!(yaml.contains(&format!("token_url: https://{auth}/api/oauth/token\n")));
        assert!(yaml.contains(&format!("host: {api}\n")));
        assert!(yaml.contains(&format!("host: {auth}\n")));
        assert!(yaml.contains("env_vars: [KIMI_API_KEY]"));
        assert!(!yaml.contains("scope:"));
        let other = if api == "api.kimi.com" {
            "api.kimi.ai"
        } else {
            "api.kimi.com"
        };
        assert!(!yaml.contains(other));
    }
}

#[test]
fn t09_keeps_anthropic_messages_production_behavior() -> TestResult {
    let tuple = lookup_tuple(Agent::Omp, Provider::Zai)?;
    assert_eq!(tuple.id, "T09");
    assert_eq!(tuple.state, TupleState::Supported);
    assert_eq!(tuple.omp_model_api(), Some("anthropic-messages"));
    assert_eq!(tuple.omp_base_url(), Some("https://api.z.ai/api/anthropic"));
    Ok(())
}
