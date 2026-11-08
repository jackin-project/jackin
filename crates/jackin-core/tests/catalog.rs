// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, AuthClass, CatalogError, Provider, TUPLES, TupleState, lookup_tuple};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn codex_tuple_is_supported() -> TestResult {
    let tuple = lookup_tuple(Agent::Codex, Provider::Codex)?;
    assert_eq!(tuple.state, TupleState::Supported);
    assert_eq!(tuple.agent, Agent::Codex);
    assert_eq!(tuple.provider, Provider::Codex);
    Ok(())
}

#[test]
fn grok_build_with_kimi_is_a_typed_error() {
    let result = lookup_tuple(Agent::GrokBuild, Provider::Kimi);
    assert!(matches!(result, Err(CatalogError::UnknownTuple { .. })));
}

#[test]
fn blocked_and_unavailable_tuples_carry_reasons() -> TestResult {
    let blocked = lookup_tuple(Agent::ClaudeCode, Provider::Claude)?;
    assert_eq!(blocked.state, TupleState::BlockedVendor);
    assert_eq!(blocked.reason, Some("vendor clarification pending"));
    let supported = lookup_tuple(Agent::Omp, Provider::Kimi)?;
    assert_eq!(supported.state, TupleState::Supported);
    assert_eq!(supported.reason, None);
    Ok(())
}

#[test]
fn t13_through_t22_resolve_with_exact_states_and_reasons() -> TestResult {
    assert_eq!(TUPLES.len(), 22);
    let unproven = "not yet proven on a real gateway";
    let no_key = "no API key on this host";
    let cases = [
        (
            "T13",
            Agent::ClaudeCode,
            Provider::Kimi,
            AuthClass::StaticKey,
            "kimi-for-coding",
            TupleState::Unavailable,
            Some(unproven),
        ),
        (
            "T14",
            Agent::ClaudeCode,
            Provider::Zai,
            AuthClass::StaticKey,
            "glm-5.3",
            TupleState::Supported,
            None,
        ),
        (
            "T15",
            Agent::ClaudeCode,
            Provider::Minimax,
            AuthClass::StaticKey,
            "minimax-m3",
            TupleState::Unavailable,
            Some(unproven),
        ),
        (
            "T16",
            Agent::ClaudeCode,
            Provider::Mimo,
            AuthClass::StaticKey,
            "mimo-v2.6-pro",
            TupleState::Unavailable,
            Some(unproven),
        ),
        (
            "T17",
            Agent::Codex,
            Provider::Kimi,
            AuthClass::StaticKey,
            "kimi-for-coding",
            TupleState::Unavailable,
            Some(unproven),
        ),
        (
            "T18",
            Agent::Codex,
            Provider::Zai,
            AuthClass::StaticKey,
            "glm-5.3",
            TupleState::Supported,
            None,
        ),
        (
            "T19",
            Agent::Codex,
            Provider::Minimax,
            AuthClass::StaticKey,
            "minimax-m3",
            TupleState::Unavailable,
            Some(unproven),
        ),
        (
            "T20",
            Agent::Codex,
            Provider::Mimo,
            AuthClass::StaticKey,
            "mimo-v2.6-pro",
            TupleState::Unavailable,
            Some(unproven),
        ),
        (
            "T21",
            Agent::Omp,
            Provider::Claude,
            AuthClass::ApiKey,
            "claude-haiku-4-5",
            TupleState::Unavailable,
            Some(no_key),
        ),
        (
            "T22",
            Agent::Omp,
            Provider::Codex,
            AuthClass::ApiKey,
            "gpt-5.1-codex",
            TupleState::Unavailable,
            Some(no_key),
        ),
    ];
    for (id, agent, provider, class, model, state, reason) in cases {
        let tuple = lookup_tuple(agent, provider)?;
        assert_eq!(tuple.id, id);
        assert_eq!(tuple.agent, agent);
        assert_eq!(tuple.provider, provider);
        assert_eq!(tuple.auth_class, class);
        assert_eq!(tuple.state, state);
        assert_eq!(tuple.reason, reason);
        assert_eq!(tuple.allowed_models, [model]);
        assert_eq!(tuple.default_model, model);
        tuple.check_model(model)?;
        assert!(matches!(
            tuple.check_model("no-such-model"),
            Err(CatalogError::UnknownModel { .. })
        ));
    }
    Ok(())
}

#[test]
fn each_provider_lists_exactly_its_catalog_tuples() -> TestResult {
    let ids = |provider: Provider| -> Vec<&'static str> {
        provider.tuples().iter().map(|tuple| tuple.id).collect()
    };
    assert_eq!(ids(Provider::Kimi), ["T08", "T10", "T13", "T17"]);
    assert_eq!(ids(Provider::Zai), ["T09", "T14", "T18"]);
    assert_eq!(ids(Provider::Minimax), ["T06", "T11", "T15", "T19"]);
    assert_eq!(ids(Provider::Mimo), ["T07", "T12", "T16", "T20"]);
    assert_eq!(ids(Provider::Claude), ["T02", "T21"]);
    assert_eq!(ids(Provider::Codex), ["T01", "T22"]);
    Ok(())
}

#[test]
fn unknown_model_is_a_typed_error_and_default_model_is_accepted() -> TestResult {
    let tuple = lookup_tuple(Agent::Codex, Provider::Codex)?;
    assert!(matches!(
        tuple.check_model("no-such-model"),
        Err(CatalogError::UnknownModel { .. })
    ));
    let accepted = tuple.check_model(tuple.default_model)?;
    assert_eq!(accepted.agent, Agent::Codex);
    Ok(())
}
