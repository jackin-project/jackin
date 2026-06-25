//! Tests for the parent module.
use super::*;

#[test]
fn pricing_table_returns_estimate_for_known_model() {
    let cost = estimate_cost_usd("claude", "claude-sonnet-4-6", 1_000_000, 100_000, 0, 0);
    assert!(cost.is_some());
    assert!(cost.unwrap() > 0.0);
}

#[test]
fn pricing_table_returns_none_for_unknown_model() {
    let cost = estimate_cost_usd("claude", "future-unknown-model-xyz", 1000, 100, 0, 0);
    assert!(cost.is_none());
}

#[test]
fn pricing_table_applies_tiered_calculation() {
    // 200k input tokens at sonnet pricing: 200k * $3/1M = $0.60
    let cost = estimate_cost_usd("claude", "claude-sonnet-4-6-20251101", 200_000, 0, 0, 0);
    assert!(cost.is_some());
    assert!((cost.unwrap() - 0.60).abs() < 0.01);
}
