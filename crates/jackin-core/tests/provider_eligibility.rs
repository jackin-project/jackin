// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, AgentTuple, AuthClass, ChatgptAccountId, CredentialSource,
    EnvName, HostPath, Provider, TUPLES, TupleState, lookup_tuple,
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

fn host() -> Result<CredentialSource, Box<dyn std::error::Error>> {
    Ok(CredentialSource::HostLogin {
        path: HostPath::parse(std::path::Path::new("/tmp/native-home"))?,
    })
}

fn file() -> Result<CredentialSource, Box<dyn std::error::Error>> {
    Ok(CredentialSource::File {
        path: HostPath::parse(std::path::Path::new("/tmp/native-key"))?,
    })
}

fn chatgpt() -> Result<CredentialSource, Box<dyn std::error::Error>> {
    Ok(CredentialSource::CodexLogin {
        home: HostPath::parse(std::path::Path::new("/tmp/codex-home"))?,
        chatgpt_account_id: ChatgptAccountId::parse("acct-1")?,
    })
}

fn credential(
    provider: Provider,
    auth_class: AuthClass,
) -> Result<Option<CredentialSource>, Box<dyn std::error::Error>> {
    Ok(match auth_class {
        AuthClass::OAuth if provider == Provider::Codex => Some(chatgpt()?),
        AuthClass::OAuth => Some(host()?),
        AuthClass::StaticKey | AuthClass::ApiKey => Some(file()?),
        AuthClass::BlockedVendor => None,
    })
}

fn publishes(tuple: &AgentTuple, class: AuthClass) -> bool {
    tuple.state == TupleState::Supported && tuple.auth_class == class
}

#[test]
fn an_account_reaches_the_gateway_only_through_a_supported_tuple() -> TestResult {
    for tuple in TUPLES {
        let Some(credential) = credential(tuple.provider, tuple.auth_class)? else {
            let account = account(tuple.provider, host()?)?;
            assert!(
                !account.has_provider_for(tuple.agent),
                "{} publishes a provider",
                tuple.id
            );
            continue;
        };
        let account = account(tuple.provider, credential)?;
        assert_eq!(
            account.has_provider_for(tuple.agent),
            publishes(&tuple, tuple.auth_class),
            "{} {:?} {:?}",
            tuple.id,
            tuple.agent,
            tuple.provider
        );
    }
    Ok(())
}

#[test]
fn the_unavailable_and_blocked_tuples_publish_no_provider() -> TestResult {
    let mut excluded = Vec::new();
    for tuple in TUPLES {
        if tuple.state == TupleState::Supported {
            continue;
        }
        excluded.push(tuple.id);
        let credential = credential(tuple.provider, tuple.auth_class)?.unwrap_or(host()?);
        let account = account(tuple.provider, credential)?;
        assert!(
            !account.has_provider_for(tuple.agent),
            "{} publishes a provider",
            tuple.id
        );
    }
    let expected = TUPLES
        .iter()
        .filter(|tuple| tuple.state != TupleState::Supported)
        .map(|tuple| tuple.id)
        .collect::<Vec<_>>();
    assert_eq!(excluded, expected, "the catalog changed");
    Ok(())
}

#[test]
fn the_native_login_accounts_follow_their_tuple_state() -> TestResult {
    for (provider, agent, tuple_id) in [
        (Provider::Muse, Agent::MuseCode, "T04"),
        (Provider::Antigravity, Agent::Antigravity, "T05"),
        (Provider::Minimax, Agent::MinimaxCode, "T06"),
        (Provider::Mimo, Agent::MimoCode, "T07"),
        (Provider::Kimi, Agent::KimiCode, "T08"),
    ] {
        let tuple = lookup_tuple(agent, provider)?;
        assert_eq!(tuple.id, tuple_id, "{tuple_id}");
        let account = account(provider, host()?)?;
        let want = publishes(tuple, AuthClass::OAuth);
        assert_eq!(account.has_provider_for(agent), want, "{tuple_id}");
        assert_eq!(account.has_provider(), want, "{tuple_id}");
    }
    Ok(())
}

#[test]
fn an_api_key_account_is_published_only_by_a_supported_api_key_tuple() -> TestResult {
    let t22 = lookup_tuple(Agent::Omp, Provider::Codex)?;
    let codex = account(Provider::Codex, file()?)?;
    assert_eq!(
        codex.has_provider_for(Agent::Omp),
        publishes(t22, AuthClass::ApiKey),
        "T22"
    );
    assert_eq!(
        codex.has_provider(),
        publishes(t22, AuthClass::ApiKey),
        "T22"
    );

    let t21 = lookup_tuple(Agent::Omp, Provider::Claude)?;
    let claude = account(Provider::Claude, file()?)?;
    assert_eq!(
        claude.has_provider_for(Agent::Omp),
        publishes(t21, AuthClass::ApiKey),
        "T21"
    );
    assert_eq!(
        claude.has_provider(),
        publishes(t21, AuthClass::ApiKey),
        "T21"
    );

    let t02 = lookup_tuple(Agent::ClaudeCode, Provider::Claude)?;
    assert_eq!(
        claude.has_provider_for(Agent::ClaudeCode),
        publishes(t02, AuthClass::ApiKey),
        "T02"
    );
    Ok(())
}

#[test]
fn a_plan_key_account_serves_the_plan_tuples_of_its_provider() -> TestResult {
    let zai = account(
        Provider::Zai,
        CredentialSource::Env {
            var: EnvName::parse("ZAI_API_KEY")?,
        },
    )?;
    for agent in [Agent::Omp, Agent::ClaudeCode, Agent::Codex] {
        let tuple = lookup_tuple(agent, Provider::Zai)?;
        assert_eq!(
            zai.has_provider_for(agent),
            publishes(tuple, AuthClass::StaticKey),
            "{}",
            tuple.id
        );
    }

    let kimi = account(
        Provider::Kimi,
        CredentialSource::Env {
            var: EnvName::parse("KIMI_API_KEY")?,
        },
    )?;
    for agent in [Agent::Omp, Agent::ClaudeCode, Agent::Codex] {
        let tuple = lookup_tuple(agent, Provider::Kimi)?;
        assert_eq!(
            kimi.has_provider_for(agent),
            publishes(tuple, AuthClass::StaticKey),
            "{}",
            tuple.id
        );
    }

    let t08 = lookup_tuple(Agent::KimiCode, Provider::Kimi)?;
    assert_eq!(
        kimi.has_provider_for(Agent::KimiCode),
        publishes(t08, AuthClass::StaticKey),
        "T08 needs a native login"
    );
    Ok(())
}

#[test]
fn a_native_login_is_not_published_as_a_plan_key() -> TestResult {
    let kimi = account(Provider::Kimi, host()?)?;
    for agent in [Agent::Omp, Agent::ClaudeCode, Agent::Codex] {
        let tuple = lookup_tuple(agent, Provider::Kimi)?;
        assert_eq!(
            kimi.has_provider_for(agent),
            publishes(tuple, AuthClass::OAuth),
            "{} needs a plan key",
            tuple.id
        );
    }

    let t08 = lookup_tuple(Agent::KimiCode, Provider::Kimi)?;
    assert_eq!(
        kimi.has_provider_for(Agent::KimiCode),
        publishes(t08, AuthClass::OAuth),
        "T08"
    );
    assert_eq!(kimi.has_provider(), publishes(t08, AuthClass::OAuth), "T08");
    Ok(())
}
