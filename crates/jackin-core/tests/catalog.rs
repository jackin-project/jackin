// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, CatalogError, Provider, TupleState, lookup_tuple};

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
fn claude_code_with_zai_is_a_typed_error() {
    let result = lookup_tuple(Agent::ClaudeCode, Provider::Zai);
    assert!(matches!(result, Err(CatalogError::UnknownTuple { .. })));
}

#[test]
fn blocked_and_unavailable_tuples_carry_reasons() -> TestResult {
    let blocked = lookup_tuple(Agent::ClaudeCode, Provider::Claude)?;
    assert_eq!(blocked.state, TupleState::BlockedVendor);
    assert!(!blocked.reason.unwrap_or_default().is_empty());
    let unavailable = lookup_tuple(Agent::Omp, Provider::Kimi)?;
    assert_eq!(unavailable.state, TupleState::Unavailable);
    assert!(!unavailable.reason.unwrap_or_default().is_empty());
    Ok(())
}

#[test]
fn kimi_provider_has_exactly_two_tuples() {
    let pairs: Vec<(Agent, Provider)> = Provider::Kimi
        .tuples()
        .iter()
        .map(|tuple| (tuple.agent, tuple.provider))
        .collect();
    assert_eq!(
        pairs,
        [
            (Agent::KimiCode, Provider::Kimi),
            (Agent::Omp, Provider::Kimi)
        ]
    );
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
