# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Normalize an explicit final API usage object, offline, without model inference.

python3 provider_usage.py receipt.json --contract openai-responses --cache-writes paid

Contracts: OpenAI Responses API, Claude Messages API, Gemini Interactions API.
Not a parser for host transcripts, Codex subscription credits, or invoices.
Optional --rates is a flat USD/MTok map for the exact model, tier, context size,
date, and API route of this one request. Unknown classes are never priced as zero.
"""
import argparse
import json
import math
from pathlib import Path
from session_cost import normalize as claude_counts, nonnegative


def required(obj, name):
    if name not in obj:
        raise ValueError(f"missing {name}; incomplete usage receipt")
    return nonnegative(obj[name], name)


def normalize(contract, usage, cache_writes=None):
    if not isinstance(usage, dict):
        raise ValueError("usage must be an object")
    reasoning = None
    if contract == "claude-messages":
        counts = claude_counts(usage)
        samples = usage.get("iterations", [usage])
        splits = [(s.get("output_tokens_details") or {}).get("thinking_tokens") for s in samples]
        if all(x is not None for x in splits):
            reasoning = 0
            for s, value in zip(samples, splits):
                value = nonnegative(value, "thinking_tokens")
                if value > s["output_tokens"]:
                    raise ValueError("thinking exceeds output total")
                reasoning += value
    elif contract == "openai-responses":
        if cache_writes not in ("paid", "ordinary-input"):
            raise ValueError("choose paid or ordinary-input cache-write accounting explicitly")
        total = required(usage, "input_tokens")
        output = required(usage, "output_tokens")
        detail = usage.get("input_tokens_details") or {}
        read = required(detail, "cached_tokens")
        counts = {"input": total - read, "cache_read": read, "output": output}
        if cache_writes == "paid":
            write = required(detail, "cache_write_tokens")
            parts = detail.get("cache_write_tokens_details") or {}
            w30 = required(parts, "ttl_30m_tokens")
            w12 = required(parts, "ttl_12h_tokens")
            if w30 + w12 != write:
                raise ValueError("write lifetime classes do not sum to cache_write_tokens")
            counts.update(input=total-read-write, cache_write_30m=w30, cache_write_12h=w12)
        elif detail.get("cache_write_tokens", 0):
            raise ValueError("nonzero paid-write receipt conflicts with ordinary-input contract")
        if counts["input"] < 0:
            raise ValueError("read/write subsets exceed input total")
        value = (usage.get("output_tokens_details") or {}).get("reasoning_tokens")
        if value is not None:
            reasoning = nonnegative(value, "reasoning_tokens")
            if reasoning > output:
                raise ValueError("reasoning exceeds output total")
    elif contract == "gemini-interactions":
        total = required(usage, "total_input_tokens")
        read = nonnegative(usage.get("total_cached_tokens", 0), "total_cached_tokens")
        visible = required(usage, "total_output_tokens")
        reasoning = required(usage, "total_thought_tokens")
        if read > total:
            raise ValueError("cached subset exceeds input total")
        counts = {"input": total-read, "cache_read": read, "output": visible+reasoning}
        if "total_tokens" in usage and required(usage, "total_tokens") != total+visible+reasoning:
            raise ValueError("Interactions total does not match input + output + thoughts")
    else:
        raise ValueError("unknown receipt contract")
    return {"contract": contract, "tokens": counts, "reasoning_output_tokens": reasoning,
            "reasoning_included_in_output": True}


def price(normalized, rates):
    counts = normalized["tokens"]
    if counts.get("cache_write_unknown", 0):
        return {"usd": None, "reason": "cache write lifetime unknown"}
    parts = {}
    for key, count in counts.items():
        if key == "cache_write_unknown":
            continue
        if not count:
            parts[key] = 0.0
            continue
        rate = rates.get(key)
        if rate is None:
            return {"usd": None, "reason": f"no exact rate for {key}"}
        if isinstance(rate, bool) or not isinstance(rate, (int, float)) or not math.isfinite(rate) or rate < 0:
            raise ValueError(f"invalid rate for {key}")
        parts[key] = count * rate / 1_000_000
    return {"usd": sum(parts.values()), "by_class": parts,
            "scope": "API list-price estimate; excludes non-token fees and discounts"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--contract", required=True,
                        choices=["openai-responses", "claude-messages", "gemini-interactions"])
    parser.add_argument("--cache-writes", choices=["paid", "ordinary-input"])
    parser.add_argument("--rates", type=Path)
    args = parser.parse_args()
    receipt = json.loads(args.receipt.read_text(encoding="utf-8"))
    result = normalize(args.contract, receipt.get("usage", receipt), args.cache_writes)
    if args.rates:
        result["list_price_estimate"] = price(result, json.loads(args.rates.read_text(encoding="utf-8")))
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
