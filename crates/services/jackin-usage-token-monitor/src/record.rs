// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Token usage recording.

use jackin_core::Agent;
use jackin_telemetry::{Attr, Value, histogram, metric, schema};

use super::TokenTotals;

pub(crate) fn record_token_usage(agent: Agent, previous: &TokenTotals, current: &TokenTotals) {
    let Some(provider) = provider_name(agent) else {
        return;
    };
    let (input, output) = token_usage_delta(agent, previous, current);
    record_token_type(provider, schema::enums::GenAiTokenType::Input, input);
    record_token_type(provider, schema::enums::GenAiTokenType::Output, output);
}

pub(crate) fn token_usage_delta(
    agent: Agent,
    previous: &TokenTotals,
    current: &TokenTotals,
) -> (u64, u64) {
    let uncached_input = current.input_tokens.saturating_sub(previous.input_tokens);
    let separate_cached_input = match agent {
        Agent::Claude | Agent::Amp | Agent::Kimi => current
            .cache_read_tokens
            .saturating_sub(previous.cache_read_tokens)
            .saturating_add(
                current
                    .cache_write_tokens
                    .saturating_sub(previous.cache_write_tokens),
            ),
        Agent::Codex | Agent::Opencode | Agent::Grok => 0,
        // No readers yet, so no separate cached-input accounting.
        Agent::Antigravity
        | Agent::Gemini
        | Agent::Cursor
        | Agent::Muse
        | Agent::Omp
        | Agent::Hermes => 0,
    };
    let input = uncached_input.saturating_add(separate_cached_input);
    let output = current.output_tokens.saturating_sub(previous.output_tokens);
    (input, output)
}

pub(crate) fn record_token_type(
    provider: schema::enums::GenAiProviderName,
    token_type: schema::enums::GenAiTokenType,
    tokens: u64,
) {
    if tokens == 0 {
        return;
    }
    let attrs = [
        Attr {
            key: schema::attrs::GEN_AI_PROVIDER_NAME,
            value: Value::Str(provider.as_str()),
        },
        Attr {
            key: schema::attrs::GEN_AI_OPERATION_NAME,
            value: Value::Str(schema::enums::GenAiOperationName::Chat.as_str()),
        },
        Attr {
            key: schema::attrs::GEN_AI_TOKEN_TYPE,
            value: Value::Str(token_type.as_str()),
        },
    ];
    let _metric_result =
        histogram(&metric::GEN_AI_CLIENT_TOKEN_USAGE).record(tokens as f64, &attrs);
}

pub(crate) const fn provider_name(agent: Agent) -> Option<schema::enums::GenAiProviderName> {
    use schema::enums::GenAiProviderName;
    match agent {
        Agent::Claude => Some(GenAiProviderName::Anthropic),
        Agent::Codex => Some(GenAiProviderName::Openai),
        Agent::Amp => Some(GenAiProviderName::Amp),
        Agent::Kimi => Some(GenAiProviderName::Kimi),
        Agent::Grok => Some(GenAiProviderName::Xai),
        Agent::Opencode => None,
        Agent::Antigravity | Agent::Gemini => Some(GenAiProviderName::Google),
        Agent::Cursor => Some(GenAiProviderName::Cursor),
        Agent::Muse => Some(GenAiProviderName::Meta),
        // Omp/Hermes route arbitrary providers per session; no single
        // attribution until the readers report the routed provider.
        Agent::Omp | Agent::Hermes => None,
    }
}
