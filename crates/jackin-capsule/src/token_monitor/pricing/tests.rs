//! Tests for the parent module.
use super::*;
use jackin_core::agent::Agent;

#[test]
fn pricing_table_returns_estimate_for_known_model() {
    let cost = estimate_cost_usd("claude-sonnet-4-6", 1_000_000, 100_000, 0, 0);
    assert!(cost.is_some());
    assert!(cost.unwrap() > 0.0);
}

#[test]
fn pricing_table_returns_none_for_unknown_model() {
    let cost = estimate_cost_usd("future-unknown-model-xyz", 1000, 100, 0, 0);
    assert!(cost.is_none());
}

#[test]
fn pricing_table_applies_tiered_calculation() {
    // 200k input tokens at sonnet pricing: 200k * $3/1M = $0.60
    let cost = estimate_cost_usd("claude-sonnet-4-6-20251101", 200_000, 0, 0, 0);
    assert!(cost.is_some());
    assert!((cost.unwrap() - 0.60).abs() < 0.01);
}

#[test]
fn agent_slug_fallback_prices_kimi() {
    // Kimi carries no wire model, so the monitor keys pricing on the agent slug;
    // drive this off `Agent::slug()` so a slug rename breaks the test rather than
    // silently leaving Kimi cost as `None`.
    let cost = estimate_cost_usd(Agent::Kimi.slug(), 1_000_000, 1_000_000, 0, 0);
    assert!(cost.is_some(), "kimi slug must hit a pricing row");
    assert!(cost.unwrap() > 0.0);
}
