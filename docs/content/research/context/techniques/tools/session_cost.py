# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

"""Audit explicitly supplied Claude JSONL files without discovering private sessions.

python3 session_cost.py session.jsonl [more.jsonl] [--prices rates.json]

Prices are USD/MTok keyed by the exact message model: input, cache_read,
cache_write_5m, cache_write_1h, output. Missing write TTLs remain unknown.
Aggregate pricing requires one price profile per model across these receipts;
split mixed tiers, dates, service modes, and routes before supplying --prices.
List-price estimates are not invoices or subscription-quota measurements.
"""
import argparse
import json
import math
from pathlib import Path

FIELDS = ("input", "cache_read", "cache_write_5m", "cache_write_1h",
          "cache_write_unknown", "output")


def nonnegative(value, field):
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ValueError(f"{field} must be a nonnegative integer")
    return value


def normalize(usage):
    """Return disjoint token classes; never silently drop aggregate cache writes."""
    if "iterations" in usage:
        iterations = usage["iterations"]
        if not isinstance(iterations, list) or not iterations:
            raise ValueError("iterations must be a nonempty final usage array")
        total = {k: 0 for k in FIELDS}
        for item in iterations:
            if not isinstance(item, dict) or "iterations" in item:
                raise ValueError("invalid nested iteration receipt")
            for key, value in normalize(item).items():
                total[key] += value
        return total  # Top-level usage excludes compaction; do not add it again.
    counts = {k: 0 for k in FIELDS}
    for target, source in (("input", "input_tokens"),
                           ("cache_read", "cache_read_input_tokens"),
                           ("output", "output_tokens")):
        if source in ("input_tokens", "output_tokens") and source not in usage:
            raise ValueError(f"missing {source}; incomplete usage receipt")
        counts[target] = nonnegative(usage.get(source, 0), source)
    detail = usage.get("cache_creation") or {}
    counts["cache_write_5m"] = nonnegative(
        detail.get("ephemeral_5m_input_tokens", 0), "5m cache writes")
    counts["cache_write_1h"] = nonnegative(
        detail.get("ephemeral_1h_input_tokens", 0), "1h cache writes")
    known = counts["cache_write_5m"] + counts["cache_write_1h"]
    aggregate = nonnegative(usage.get("cache_creation_input_tokens", known),
                            "cache_creation_input_tokens")
    if aggregate < known:
        raise ValueError("cache write breakdown exceeds aggregate")
    counts["cache_write_unknown"] = aggregate - known
    return counts


def collect(paths):
    """Deduplicate IDs per session, rejecting ambiguous/non-monotone receipts.

    Equal records repeat; cumulative streaming updates replace earlier records.
    Missing session IDs scope the message ID to the explicit file.
    """
    seen = {}
    duplicates = 0
    for filename in paths:
        path = Path(filename)
        with path.open(encoding="utf-8") as stream:
            for line_number, line in enumerate(stream, 1):
                if not line.strip():
                    continue
                try:
                    record = json.loads(line)
                    if record.get("type") != "assistant":
                        continue
                    message = record.get("message") or {}
                    usage = message.get("usage")
                    if usage is None:
                        continue
                    if not message.get("id"):
                        raise ValueError("usage-bearing response has no message ID")
                    session = record.get("sessionId") or record.get("session_id")
                    scope = ("session", session) if session else ("file", str(path.resolve()))
                    key = (scope, message["id"])
                    model = message.get("model") or "unknown"
                    new = normalize(usage)
                    if key in seen:
                        duplicates += 1
                        old_model, old = seen[key]
                        if old_model != model:
                            raise ValueError("response model changed for the same ID")
                        old_values = [old[k] for k in ("input", "cache_read", "output")]
                        new_values = [new[k] for k in ("input", "cache_read", "output")]
                        old_values.append(sum(old[k] for k in FIELDS if k.startswith("cache_write")))
                        new_values.append(sum(new[k] for k in FIELDS if k.startswith("cache_write")))
                        if any(n < o for n, o in zip(new_values, old_values)):
                            raise ValueError("non-monotone duplicate usage; inspect receipts")
                        if new_values == old_values and new["cache_write_unknown"] > old["cache_write_unknown"]:
                            continue  # Retain the more detailed equivalent receipt.
                        if any(new[k] < old[k] for k in ("cache_write_5m", "cache_write_1h")):
                            raise ValueError("conflicting cache TTL breakdown for the same ID")
                    seen[key] = (model, new)
                except (ValueError, TypeError, AttributeError) as error:
                    raise ValueError(f"{path.name}:{line_number}: {error}") from error
    totals = {}
    for model, counts in seen.values():
        dest = totals.setdefault(model, {k: 0 for k in FIELDS})
        for key in FIELDS:
            dest[key] += counts[key]
    return {"unique_responses": len(seen), "duplicate_records": duplicates,
            "tokens_by_model": totals}


def price(totals, rates):
    costs = {}
    for model, counts in totals.items():
        rate = rates.get(model)
        if rate is None:
            costs[model] = {"usd": None, "reason": "no rate for exact model"}
            continue
        required = [k for k in FIELDS if k != "cache_write_unknown"]
        if any(k not in rate or isinstance(rate[k], bool) or
               not isinstance(rate[k], (int, float)) or
               not math.isfinite(rate[k]) or rate[k] < 0 for k in required):
            raise ValueError(f"invalid or incomplete rates for {model}")
        if counts["cache_write_unknown"]:
            costs[model] = {"usd": None, "reason": "cache write TTL unknown"}
            continue
        parts = {k: counts[k] * rate[k] / 1_000_000 for k in required}
        costs[model] = {"usd": sum(parts.values()), "by_class": parts}
    return costs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("transcripts", nargs="+")
    parser.add_argument("--prices", type=Path)
    args = parser.parse_args()
    result = collect(args.transcripts)
    result["accounting"] = "observed Claude usage; no subscription conversion"
    if args.prices:
        result["list_price_estimate"] = price(
            result["tokens_by_model"], json.loads(args.prices.read_text()))
    result["limits"] = [
        "Explicit files only; include subagents and failed attempts.",
        "Output includes reasoning and tool arguments; no inferred thinking percentage.",
        "This is not a Codex/OpenAI receipt parser; missing input/output receipts fail."]
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
