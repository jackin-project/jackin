// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, AuthClass, CatalogError, Provider, TUPLES, TupleState, lookup_tuple};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn codex_tuple_is_unavailable_until_the_host_login_is_renewed() -> TestResult {
    let tuple = lookup_tuple(Agent::Codex, Provider::Codex)?;
    assert_eq!(tuple.state, TupleState::Unavailable);
    assert_eq!(
        tuple.reason,
        Some("final live proof pending after host re-login")
    );
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
    assert_eq!(blocked.auth_class, AuthClass::BlockedVendor);
    assert_eq!(blocked.state, TupleState::BlockedVendor);
    assert_eq!(blocked.reason, Some("vendor clarification pending"));
    let grok = lookup_tuple(Agent::GrokBuild, Provider::Grok)?;
    assert_eq!(grok.state, TupleState::Unavailable);
    assert_eq!(grok.reason, Some("not yet proven on a real gateway"));
    let supported = lookup_tuple(Agent::Omp, Provider::Zai)?;
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
            "MiniMax-M3",
            TupleState::Unavailable,
            Some("uppercase MiniMax-M3 live proof pending"),
        ),
        (
            "T16",
            Agent::ClaudeCode,
            Provider::Mimo,
            AuthClass::StaticKey,
            "mimo-v2.6-pro",
            TupleState::Unavailable,
            Some("final live proof pending with MiMo Token Plan key"),
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
            "MiniMax-M3",
            TupleState::Unavailable,
            Some("uppercase MiniMax-M3 live proof pending"),
        ),
        (
            "T20",
            Agent::Codex,
            Provider::Mimo,
            AuthClass::StaticKey,
            "mimo-v2.6-pro",
            TupleState::Unavailable,
            Some("final live proof pending with MiMo Token Plan key"),
        ),
        (
            "T21",
            Agent::Omp,
            Provider::Claude,
            AuthClass::ApiKey,
            "claude-haiku-5-5",
            TupleState::Unavailable,
            Some("final live proof pending with Anthropic API key"),
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

#[test]
fn changed_model_declarations_agree_between_specs_and_tuples() -> TestResult {
    for (agent, id, accepted, rejected) in [
        (Agent::Codex, "T01", "gpt-6.1-sol", "gpt-5.1-codex"),
        (
            Agent::Antigravity,
            "T05",
            "gemini-4-argon-medium",
            "gemini-4-argon",
        ),
    ] {
        let spec = agent.spec();
        assert_eq!(spec.allowed_models, [accepted], "{id} spec");
        assert_eq!(spec.default_model, accepted, "{id} spec");
        let tuple = TUPLES.iter().find(|tuple| tuple.id == id).ok_or(id)?;
        assert_eq!(tuple.allowed_models, [accepted], "{id} tuple");
        assert_eq!(tuple.default_model, accepted, "{id} tuple");
        assert_eq!(tuple.check_model(accepted)?.id, id);
        assert!(matches!(
            tuple.check_model(rejected),
            Err(CatalogError::UnknownModel { ref model, .. }) if model == rejected
        ));
    }
    Ok(())
}

#[test]
fn model_acceptance_is_exact_and_independent_of_launch_availability() -> TestResult {
    for (id, accepted, rejected) in [
        ("T11", "minimax-m3", "MiniMax-M3"),
        ("T15", "MiniMax-M3", "minimax-m3"),
        ("T19", "MiniMax-M3", "minimax-m3"),
        ("T21", "claude-haiku-5-5", "claude-haiku-4-5"),
        ("T22", "gpt-5.1-codex", "gpt-6.1-sol"),
    ] {
        let tuple = TUPLES.iter().find(|tuple| tuple.id == id).ok_or(id)?;
        assert_eq!(tuple.allowed_models, [accepted], "{id}");
        assert_eq!(tuple.default_model, accepted, "{id}");
        assert_eq!(tuple.check_model(accepted)?.id, id);
        assert!(matches!(
            tuple.check_model(rejected),
            Err(CatalogError::UnknownModel { ref model, .. }) if model == rejected
        ));
    }
    Ok(())
}

#[test]
fn availability_states_and_reasons_are_exact() -> TestResult {
    let uppercase_pending = "uppercase MiniMax-M3 live proof pending";
    let mimo_key = "final live proof pending with MiMo Token Plan key";
    for (id, state, reason) in [
        (
            "T01",
            TupleState::Unavailable,
            Some("final live proof pending after host re-login"),
        ),
        (
            "T05",
            TupleState::Unavailable,
            Some("subscription requires a desktop keyring in the sandbox"),
        ),
        ("T09", TupleState::Supported, None),
        (
            "T10",
            TupleState::Unavailable,
            Some("final live proof pending with Kimi plan key"),
        ),
        (
            "T11",
            TupleState::Unavailable,
            Some("final live proof pending with MiniMax plan key"),
        ),
        ("T12", TupleState::Unavailable, Some(mimo_key)),
        ("T14", TupleState::Supported, None),
        ("T15", TupleState::Unavailable, Some(uppercase_pending)),
        ("T16", TupleState::Unavailable, Some(mimo_key)),
        ("T18", TupleState::Supported, None),
        ("T19", TupleState::Unavailable, Some(uppercase_pending)),
        ("T20", TupleState::Unavailable, Some(mimo_key)),
        (
            "T21",
            TupleState::Unavailable,
            Some("final live proof pending with Anthropic API key"),
        ),
        (
            "T22",
            TupleState::Unavailable,
            Some("no API key on this host"),
        ),
    ] {
        let tuple = TUPLES.iter().find(|tuple| tuple.id == id).ok_or(id)?;
        assert_eq!(tuple.state, state, "{id}");
        assert_eq!(tuple.reason, reason, "{id}");
    }
    Ok(())
}
