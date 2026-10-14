// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::Agent;

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
    let cost = estimate_cost_usd("claude-sonnet-4-6-20251101", 200_000, 0, 0, 0);
    assert!(cost.is_some());
    assert!((cost.unwrap() - 0.60).abs() < 0.01);
}

#[test]
fn agent_slug_fallback_prices_kimi() {
    let cost = estimate_cost_usd(Agent::Kimi.slug(), 1_000_000, 1_000_000, 0, 0);
    assert!(cost.is_some(), "kimi slug must hit a pricing row");
    assert!(cost.unwrap() > 0.0);
}
