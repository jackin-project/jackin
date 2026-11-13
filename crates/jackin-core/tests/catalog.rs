// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, AuthClass, CatalogError, Provider, TUPLES, TupleState, lookup_tuple};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Row {
    id: &'static str,
    agent: Agent,
    provider: Provider,
    auth_class: AuthClass,
    state: TupleState,
    reason: Option<&'static str>,
    model: &'static str,
}

const UNPROVEN: Option<&str> = Some("not yet proven on a real gateway");
const SUPPORTED: Option<&str> = None;

const ROWS: [Row; 22] = [
    Row {
        id: "T01",
        agent: Agent::Codex,
        provider: Provider::Codex,
        auth_class: AuthClass::OAuth,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "gpt-6.1-sol",
    },
    Row {
        id: "T02",
        agent: Agent::ClaudeCode,
        provider: Provider::Claude,
        auth_class: AuthClass::BlockedVendor,
        state: TupleState::BlockedVendor,
        reason: Some("vendor clarification pending"),
        model: "haiku",
    },
    Row {
        id: "T03",
        agent: Agent::GrokBuild,
        provider: Provider::Grok,
        auth_class: AuthClass::OAuth,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "grok-4.7",
    },
    Row {
        id: "T04",
        agent: Agent::MuseCode,
        provider: Provider::Muse,
        auth_class: AuthClass::OAuth,
        state: TupleState::Unavailable,
        reason: UNPROVEN,
        model: "muse-spark-1.2",
    },
    Row {
        id: "T05",
        agent: Agent::Antigravity,
        provider: Provider::Antigravity,
        auth_class: AuthClass::BlockedVendor,
        state: TupleState::Unavailable,
        reason: Some("subscription requires a desktop keyring in the sandbox"),
        model: "gemini-4-argon-medium",
    },
    Row {
        id: "T06",
        agent: Agent::MinimaxCode,
        provider: Provider::Minimax,
        auth_class: AuthClass::OAuth,
        state: TupleState::Unavailable,
        reason: Some("no stable Linux artifact"),
        model: "minimax-m3",
    },
    Row {
        id: "T07",
        agent: Agent::MimoCode,
        provider: Provider::Mimo,
        auth_class: AuthClass::OAuth,
        state: TupleState::Unavailable,
        reason: Some("native login has no refresh grant"),
        model: "mimo-v2.6-pro",
    },
    Row {
        id: "T08",
        agent: Agent::KimiCode,
        provider: Provider::Kimi,
        auth_class: AuthClass::OAuth,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "kimi-for-coding",
    },
    Row {
        id: "T09",
        agent: Agent::Omp,
        provider: Provider::Zai,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "glm-5.3",
    },
    Row {
        id: "T10",
        agent: Agent::Omp,
        provider: Provider::Kimi,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "kimi-for-coding",
    },
    Row {
        id: "T11",
        agent: Agent::Omp,
        provider: Provider::Minimax,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "minimax-m3",
    },
    Row {
        id: "T12",
        agent: Agent::Omp,
        provider: Provider::Mimo,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "mimo-v2.6-pro",
    },
    Row {
        id: "T13",
        agent: Agent::ClaudeCode,
        provider: Provider::Kimi,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "kimi-for-coding",
    },
    Row {
        id: "T14",
        agent: Agent::ClaudeCode,
        provider: Provider::Zai,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "glm-5.3",
    },
    Row {
        id: "T15",
        agent: Agent::ClaudeCode,
        provider: Provider::Minimax,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "MiniMax-M3",
    },
    Row {
        id: "T16",
        agent: Agent::ClaudeCode,
        provider: Provider::Mimo,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "mimo-v2.6-pro",
    },
    Row {
        id: "T17",
        agent: Agent::Codex,
        provider: Provider::Kimi,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "kimi-for-coding",
    },
    Row {
        id: "T18",
        agent: Agent::Codex,
        provider: Provider::Zai,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "glm-5.3",
    },
    Row {
        id: "T19",
        agent: Agent::Codex,
        provider: Provider::Minimax,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "MiniMax-M3",
    },
    Row {
        id: "T20",
        agent: Agent::Codex,
        provider: Provider::Mimo,
        auth_class: AuthClass::StaticKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "mimo-v2.6-pro",
    },
    Row {
        id: "T21",
        agent: Agent::Omp,
        provider: Provider::Claude,
        auth_class: AuthClass::ApiKey,
        state: TupleState::Supported,
        reason: SUPPORTED,
        model: "claude-haiku-5-5",
    },
    Row {
        id: "T22",
        agent: Agent::Omp,
        provider: Provider::Codex,
        auth_class: AuthClass::ApiKey,
        state: TupleState::Unavailable,
        reason: Some("no API key on this host"),
        model: "gpt-5.1-codex",
    },
];

#[test]
fn all_22_catalog_rows_match_the_approved_matrix() -> TestResult {
    assert_eq!(TUPLES.len(), ROWS.len());
    for (index, row) in ROWS.iter().enumerate() {
        let tuple = lookup_tuple(row.agent, row.provider)?;
        assert_eq!(TUPLES[index].id, row.id);
        assert_eq!(tuple.id, row.id);
        assert_eq!(tuple.agent, row.agent, "{}", row.id);
        assert_eq!(tuple.provider, row.provider, "{}", row.id);
        assert_eq!(tuple.auth_class, row.auth_class, "{}", row.id);
        assert_eq!(tuple.state, row.state, "{}", row.id);
        assert_eq!(tuple.reason, row.reason, "{}", row.id);
        assert_eq!(tuple.allowed_models, [row.model], "{}", row.id);
        assert_eq!(tuple.default_model, row.model, "{}", row.id);
        assert_eq!(tuple.check_model(row.model)?.id, row.id);
        assert!(
            matches!(
                tuple.check_model("no-such-model"),
                Err(CatalogError::UnknownModel { agent, provider, ref model })
                    if agent == row.agent && provider == row.provider && model == "no-such-model"
            ),
            "{}",
            row.id
        );
        assert_eq!(
            tuple.state == TupleState::Supported,
            tuple.reason.is_none(),
            "{}",
            row.id
        );
    }
    Ok(())
}

#[test]
fn model_matching_is_case_sensitive_and_rejects_removed_models() -> TestResult {
    for (id, accepted, rejected) in [
        ("T01", "gpt-6.1-sol", "gpt-5.1-codex"),
        ("T11", "minimax-m3", "MiniMax-M3"),
        ("T13", "kimi-for-coding", "no-such-model"),
        ("T15", "MiniMax-M3", "minimax-m3"),
        ("T19", "MiniMax-M3", "minimax-m3"),
        ("T21", "claude-haiku-5-5", "claude-haiku-4-5"),
        ("T22", "gpt-5.1-codex", "gpt-6.1-sol"),
    ] {
        let tuple = TUPLES.iter().find(|tuple| tuple.id == id).ok_or(id)?;
        assert_eq!(tuple.check_model(accepted)?.id, id);
        assert!(
            matches!(
                tuple.check_model(rejected),
                Err(CatalogError::UnknownModel { ref model, .. }) if model == rejected
            ),
            "{id}"
        );
    }
    Ok(())
}

#[test]
fn unknown_agent_and_provider_pairs_are_typed_errors() {
    for (agent, provider) in [
        (Agent::GrokBuild, Provider::Kimi),
        (Agent::Codex, Provider::Claude),
        (Agent::Omp, Provider::Grok),
    ] {
        assert!(matches!(
            lookup_tuple(agent, provider),
            Err(CatalogError::UnknownTuple { agent: found_agent, provider: found_provider })
                if found_agent == agent && found_provider == provider
        ));
    }
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
fn changed_model_declarations_agree_between_specs_and_tuples() -> TestResult {
    for (agent, id, accepted, rejected) in [
        (Agent::Codex, "T01", &["gpt-6.1-sol"][..], "not-a-model"),
        (
            Agent::Antigravity,
            "T05",
            &["gemini-4-argon-medium"][..],
            "gemini-4-argon",
        ),
    ] {
        let spec = agent.spec();
        assert_eq!(spec.allowed_models, accepted, "{id} spec");
        assert_eq!(spec.default_model, accepted[0], "{id} spec");
        let tuple = TUPLES.iter().find(|tuple| tuple.id == id).ok_or(id)?;
        assert_eq!(tuple.allowed_models, accepted, "{id} tuple");
        assert_eq!(tuple.default_model, accepted[0], "{id} tuple");
        for model in accepted {
            assert_eq!(tuple.check_model(model)?.id, id);
        }
        assert!(matches!(
            tuple.check_model(rejected),
            Err(CatalogError::UnknownModel { ref model, .. }) if model == rejected
        ));
    }
    Ok(())
}
