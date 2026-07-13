// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Static pricing table for token cost estimation.
//!
//! Used when a JSONL/SQLite record does not include a pre-calculated costUSD.
//! APPROXIMATE — last updated 2026-06-04. Prices in USD per 1M tokens.

/// Pricing for a specific model.
#[derive(Debug, Clone)]
pub(crate) struct ModelPrice {
    pub(crate) input_per_1m: f64,
    pub(crate) output_per_1m: f64,
    pub(crate) cache_read_per_1m: f64,
    pub(crate) cache_write_per_1m: f64,
}

/// Estimate cost in USD from token counts using the static pricing table.
/// Returns `None` when the model is not in the table.
pub(crate) fn estimate_cost_usd(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) -> Option<f64> {
    let price = model_price(model)?;
    let cost = (input_tokens as f64 / 1_000_000.0) * price.input_per_1m
        + (output_tokens as f64 / 1_000_000.0) * price.output_per_1m
        + (cache_read_tokens as f64 / 1_000_000.0) * price.cache_read_per_1m
        + (cache_write_tokens as f64 / 1_000_000.0) * price.cache_write_per_1m;
    Some(cost)
}

fn model_price(model: &str) -> Option<ModelPrice> {
    // Prices as of 2026-06-04 (APPROXIMATE).
    let price = match model {
        m if m.contains("claude-opus-4") => ModelPrice {
            input_per_1m: 15.0,
            output_per_1m: 75.0,
            cache_read_per_1m: 1.50,
            cache_write_per_1m: 18.75,
        },
        m if m.contains("claude-sonnet-4") => ModelPrice {
            input_per_1m: 3.0,
            output_per_1m: 15.0,
            cache_read_per_1m: 0.30,
            cache_write_per_1m: 3.75,
        },
        m if m.contains("claude-haiku-4") || m.contains("claude-3-5-haiku") => ModelPrice {
            input_per_1m: 0.80,
            output_per_1m: 4.0,
            cache_read_per_1m: 0.08,
            cache_write_per_1m: 1.0,
        },
        m if m.contains("claude-3-5-sonnet") => ModelPrice {
            input_per_1m: 3.0,
            output_per_1m: 15.0,
            cache_read_per_1m: 0.30,
            cache_write_per_1m: 3.75,
        },
        m if m.contains("gpt-4o") => ModelPrice {
            input_per_1m: 2.50,
            output_per_1m: 10.0,
            cache_read_per_1m: 1.25,
            cache_write_per_1m: 0.0,
        },
        m if m.contains("o3") => ModelPrice {
            input_per_1m: 10.0,
            output_per_1m: 40.0,
            cache_read_per_1m: 2.50,
            cache_write_per_1m: 0.0,
        },
        m if m.contains("kimi") || m.contains("moonshot") => ModelPrice {
            input_per_1m: 0.50,
            output_per_1m: 1.50,
            cache_read_per_1m: 0.05,
            cache_write_per_1m: 0.0,
        },
        _ => return None,
    };
    Some(price)
}

#[cfg(test)]
mod tests;
