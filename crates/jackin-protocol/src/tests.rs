// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for protocol types.
use super::*;

#[test]
fn label_round_trips_through_from_label() {
    for provider in Provider::ALL {
        assert_eq!(Provider::from_label(provider.label()), Some(provider));
    }
    assert_eq!(Provider::from_label("Gemini"), None);
}

#[test]
fn anthropic_needs_no_env_overrides() {
    assert!(Provider::Anthropic.env_overrides(Some("tok")).is_empty());
}

#[test]
fn zai_injects_token_only_when_present() {
    assert_eq!(
        Provider::Zai.env_overrides(Some("tok")),
        vec![
            ("ANTHROPIC_AUTH_TOKEN".to_owned(), "tok".to_owned()),
            ("ANTHROPIC_BASE_URL".to_owned(), ZAI_BASE_URL.to_owned()),
            (
                "ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(),
                ZAI_DEFAULT_OPUS_MODEL.to_owned()
            ),
            (
                "ANTHROPIC_DEFAULT_SONNET_MODEL".to_owned(),
                ZAI_DEFAULT_SONNET_MODEL.to_owned()
            ),
            (
                "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_owned(),
                ZAI_DEFAULT_HAIKU_MODEL.to_owned()
            ),
            ("API_TIMEOUT_MS".to_owned(), ZAI_API_TIMEOUT_MS.to_owned()),
            (
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_owned(),
                "1".to_owned()
            ),
        ]
    );
    // None and empty both mean "daemon backfills the token from env":
    // emit the base-url redirect and model mapping but no token entry.
    for absent in [None, Some("")] {
        assert_eq!(
            Provider::Zai.env_overrides(absent),
            vec![
                ("ANTHROPIC_BASE_URL".to_owned(), ZAI_BASE_URL.to_owned()),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(),
                    ZAI_DEFAULT_OPUS_MODEL.to_owned()
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_owned(),
                    ZAI_DEFAULT_SONNET_MODEL.to_owned()
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_owned(),
                    ZAI_DEFAULT_HAIKU_MODEL.to_owned()
                ),
                ("API_TIMEOUT_MS".to_owned(), ZAI_API_TIMEOUT_MS.to_owned()),
                (
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_owned(),
                    "1".to_owned()
                ),
            ]
        );
    }
}

#[test]
fn available_for_provider_matrix() {
    // Claude: Anthropic always included (subscription auth, no key needed).
    assert_eq!(
        Provider::available_for("claude", |p| matches!(p, Provider::Zai)),
        vec![Provider::Anthropic, Provider::Zai]
    );
    assert_eq!(
        Provider::available_for("claude", |p| matches!(p, Provider::Minimax)),
        vec![Provider::Anthropic, Provider::Minimax]
    );
    assert_eq!(
        Provider::available_for("claude", |p| matches!(p, Provider::Kimi)),
        vec![Provider::Anthropic, Provider::Kimi]
    );
    assert_eq!(
        Provider::available_for("claude", |p| {
            matches!(p, Provider::Zai | Provider::Minimax | Provider::Kimi)
        }),
        vec![
            Provider::Anthropic,
            Provider::Zai,
            Provider::Minimax,
            Provider::Kimi
        ]
    );
    // No alt providers -> no picker (Anthropic alone = native sole -> empty).
    assert!(Provider::available_for("claude", |_| false).is_empty());

    // Codex: OpenAI always included (native). Only MiniMax supports it today
    // (GLM/Kimi deferred), and Zai/Kimi are filtered out by `supports_agent`.
    assert_eq!(
        Provider::available_for("codex", |p| matches!(p, Provider::Minimax)),
        vec![Provider::Openai, Provider::Minimax]
    );
    assert!(Provider::available_for("codex", |_| false).is_empty());
    assert!(Provider::available_for("codex", |p| matches!(p, Provider::Zai)).is_empty());
    assert!(Provider::available_for("codex", |p| matches!(p, Provider::Kimi)).is_empty());

    // OpenCode: Anthropic only when anthropic_api_key is set (subscription not available).
    assert_eq!(
        Provider::available_for("opencode", |p| {
            matches!(p, Provider::Anthropic | Provider::Zai | Provider::Minimax)
        }),
        vec![Provider::Anthropic, Provider::Zai, Provider::Minimax]
    );
    assert_eq!(
        Provider::available_for("opencode", |p| {
            matches!(p, Provider::Zai | Provider::Minimax)
        }),
        vec![Provider::Zai, Provider::Minimax]
    );
    assert!(Provider::available_for("opencode", |_| false).is_empty());
    // Only ANTHROPIC_API_KEY, no alts -> sole entry is native Anthropic -> no picker.
    assert!(Provider::available_for("opencode", |p| matches!(p, Provider::Anthropic)).is_empty());
    // A single alt provider survives so the caller auto-routes through it.
    assert_eq!(
        Provider::available_for("opencode", |p| matches!(p, Provider::Zai)),
        vec![Provider::Zai]
    );
    assert_eq!(
        Provider::available_for("opencode", |p| matches!(p, Provider::Kimi)),
        vec![Provider::Kimi]
    );

    // Unknown agent (amp): always empty because no adapters support it.
    assert!(Provider::available_for("amp", |_| true).is_empty());
}

#[test]
fn codex_profile_is_some_only_for_minimax() {
    assert_eq!(Provider::Minimax.codex_profile(), Some("minimax"));
    for p in [
        Provider::Anthropic,
        Provider::Openai,
        Provider::Zai,
        Provider::Kimi,
    ] {
        assert_eq!(
            p.codex_profile(),
            None,
            "{p:?} must not declare a Codex profile"
        );
    }
}

#[test]
fn minimax_env_overrides_map_all_tiers_to_same_model() {
    let env = Provider::Minimax.env_overrides(Some("mk"));
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_BASE_URL" && v == MINIMAX_BASE_URL)
    );
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_DEFAULT_OPUS_MODEL" && v == MINIMAX_DEFAULT_MODEL)
    );
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_DEFAULT_SONNET_MODEL" && v == MINIMAX_DEFAULT_MODEL)
    );
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_DEFAULT_HAIKU_MODEL" && v == MINIMAX_DEFAULT_MODEL)
    );
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_AUTH_TOKEN" && v == "mk")
    );
}

#[test]
fn kimi_env_overrides_map_all_tiers_to_same_model() {
    let env = Provider::Kimi.env_overrides(Some("kk"));
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_BASE_URL" && v == KIMI_BASE_URL)
    );
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_DEFAULT_OPUS_MODEL" && v == KIMI_DEFAULT_MODEL)
    );
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_DEFAULT_SONNET_MODEL" && v == KIMI_DEFAULT_MODEL)
    );
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_DEFAULT_HAIKU_MODEL" && v == KIMI_DEFAULT_MODEL)
    );
    assert!(
        env.iter()
            .any(|(k, v)| k == "ANTHROPIC_AUTH_TOKEN" && v == "kk")
    );
}
