// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde_json::{Map, Value, json};

use super::{normalize_usage, read_utf8};

#[derive(Args)]
pub(crate) struct ProviderUsageArgs {
    receipt: PathBuf,
    #[arg(long)]
    contract: String,
    #[arg(long)]
    cache_writes: Option<String>,
    #[arg(long)]
    rates: Option<PathBuf>,
}

pub(super) fn run(args: ProviderUsageArgs) -> Result<()> {
    let receipt: Value = serde_json::from_str(&read_utf8(&args.receipt)?)
        .with_context(|| format!("parse receipt {}", args.receipt.display()))?;
    let usage = receipt.get("usage").unwrap_or(&receipt);
    let mut result = normalize(&args.contract, usage, args.cache_writes.as_deref())?;
    if let Some(path) = args.rates {
        let rates: Value = serde_json::from_str(&read_utf8(&path)?)
            .with_context(|| format!("parse rates {}", path.display()))?;
        let rates = rates.as_object().context("rates must be a JSON object")?;
        result["list_price_estimate"] = price(&result, rates)?;
    }
    super::print_json(&result)
}

fn normalize(contract: &str, usage: &Value, cache_writes: Option<&str>) -> Result<Value> {
    let usage = usage.as_object().context("usage must be an object")?;
    let mut reasoning = None;
    let tokens = match contract {
        "claude-messages" => {
            let counts = normalize_usage(&Value::Object(usage.clone()))?;
            let samples = match usage.get("iterations") {
                None => vec![Value::Object(usage.clone())],
                Some(Value::Array(iterations)) => iterations.clone(),
                Some(_) => bail!("iterations must be a nonempty final usage array"),
            };
            let split_values = samples
                .iter()
                .map(|sample| {
                    sample
                        .get("output_tokens_details")
                        .and_then(|details| details.get("thinking_tokens"))
                        .filter(|value| !value.is_null())
                })
                .collect::<Vec<_>>();
            if split_values.iter().all(Option::is_some) {
                let mut total = 0_u64;
                for (sample, value) in samples.iter().zip(split_values) {
                    let value = value.context("thinking token split disappeared")?;
                    let value = nonnegative(value, "thinking_tokens")?;
                    let output = sample
                        .get("output_tokens")
                        .and_then(Value::as_u64)
                        .context("missing or invalid output_tokens; incomplete usage receipt")?;
                    if value > output {
                        bail!("thinking exceeds output total");
                    }
                    total = total
                        .checked_add(value)
                        .context("thinking token total overflow")?;
                }
                reasoning = Some(total);
            }
            counts.to_json()
        }
        "openai-responses" => {
            if !matches!(cache_writes, Some("paid" | "ordinary-input")) {
                bail!("choose paid or ordinary-input cache-write accounting explicitly");
            }
            let total = required(usage, "input_tokens")?;
            let output = required(usage, "output_tokens")?;
            let detail = optional_object(usage, "input_tokens_details")?;
            let read = required(detail, "cached_tokens")?;
            let (write, write_parts) = match cache_writes {
                Some("paid") => {
                    let write = required(detail, "cache_write_tokens")?;
                    let parts = optional_object(detail, "cache_write_tokens_details")?;
                    let thirty = required(parts, "ttl_30m_tokens")?;
                    let twelve = required(parts, "ttl_12h_tokens")?;
                    if thirty.checked_add(twelve) != Some(write) {
                        bail!("write lifetime classes do not sum to cache_write_tokens");
                    }
                    (write, Some((thirty, twelve)))
                }
                Some("ordinary-input") => {
                    if optional_u64(detail, "cache_write_tokens")? > 0 {
                        bail!("nonzero paid-write receipt conflicts with ordinary-input contract");
                    }
                    (0, None)
                }
                _ => bail!("choose paid or ordinary-input cache-write accounting explicitly"),
            };
            let input = total
                .checked_sub(read)
                .and_then(|value| value.checked_sub(write))
                .context("read/write subsets exceed input total")?;
            let mut counts = Map::new();
            counts.insert("input".to_owned(), json!(input));
            counts.insert("cache_read".to_owned(), json!(read));
            counts.insert("output".to_owned(), json!(output));
            if let Some((thirty, twelve)) = write_parts {
                counts.insert("cache_write_30m".to_owned(), json!(thirty));
                counts.insert("cache_write_12h".to_owned(), json!(twelve));
            }
            let details = optional_object(usage, "output_tokens_details")?;
            if let Some(value) = details.get("reasoning_tokens")
                && !value.is_null()
            {
                let value = nonnegative(value, "reasoning_tokens")?;
                if value > output {
                    bail!("reasoning exceeds output total");
                }
                reasoning = Some(value);
            }
            Value::Object(counts)
        }
        "gemini-interactions" => {
            let total = required(usage, "total_input_tokens")?;
            let read = optional_u64(usage, "total_cached_tokens")?;
            let visible = required(usage, "total_output_tokens")?;
            let thoughts = required(usage, "total_thought_tokens")?;
            let input = total
                .checked_sub(read)
                .context("cached subset exceeds input total")?;
            let output = visible
                .checked_add(thoughts)
                .context("output plus thoughts overflow")?;
            if usage.contains_key("total_tokens")
                && required(usage, "total_tokens")?
                    != total
                        .checked_add(visible)
                        .and_then(|value| value.checked_add(thoughts))
                        .context("Interactions total overflow")?
            {
                bail!("Interactions total does not match input + output + thoughts");
            }
            reasoning = Some(thoughts);
            json!({"input": input, "cache_read": read, "output": output})
        }
        _ => bail!("unknown receipt contract"),
    };
    Ok(json!({
        "contract": contract,
        "tokens": tokens,
        "reasoning_output_tokens": reasoning,
        "reasoning_included_in_output": true
    }))
}

fn required(object: &Map<String, Value>, field: &str) -> Result<u64> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .with_context(|| format!("missing or invalid {field}; incomplete usage receipt"))
}

fn optional_u64(object: &Map<String, Value>, field: &str) -> Result<u64> {
    match object.get(field) {
        None => Ok(0),
        Some(value) => nonnegative(value, field),
    }
}

fn optional_object<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a Map<String, Value>> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(empty_object()),
        Some(Value::Object(value)) => Ok(value),
        Some(_) => bail!("{field} must be an object"),
    }
}

fn empty_object() -> &'static Map<String, Value> {
    static EMPTY: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);
    &EMPTY
}

fn nonnegative(value: &Value, field: &str) -> Result<u64> {
    value
        .as_u64()
        .with_context(|| format!("{field} must be a nonnegative integer"))
}

fn price(normalized: &Value, rates: &Map<String, Value>) -> Result<Value> {
    let counts = normalized
        .get("tokens")
        .and_then(Value::as_object)
        .context("normalized tokens must be an object")?;
    if counts
        .get("cache_write_unknown")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        > 0
    {
        return Ok(json!({"usd": null, "reason": "cache write lifetime unknown"}));
    }
    let mut parts = Map::new();
    let mut total = 0.0_f64;
    for (field, count) in counts {
        if field == "cache_write_unknown" {
            continue;
        }
        let count = count
            .as_u64()
            .with_context(|| format!("normalized {field} count must be a nonnegative integer"))?;
        if count == 0 {
            parts.insert(field.clone(), json!(0.0));
            continue;
        }
        let Some(rate) = rates.get(field).filter(|rate| !rate.is_null()) else {
            return Ok(json!({"usd": null, "reason": format!("no exact rate for {field}")}));
        };
        let rate = rate
            .as_f64()
            .filter(|rate| rate.is_finite() && *rate >= 0.0)
            .with_context(|| format!("invalid rate for {field}"))?;
        let cost = count as f64 * rate / 1_000_000.0;
        if !cost.is_finite() || !(total + cost).is_finite() {
            bail!("price estimate overflow for {field}");
        }
        total += cost;
        parts.insert(field.clone(), json!(cost));
    }
    Ok(json!({
        "usd": total,
        "by_class": parts,
        "scope": "API list-price estimate; excludes non-token fees and discounts"
    }))
}

#[cfg(test)]
mod tests;
