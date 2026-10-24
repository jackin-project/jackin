#!/usr/bin/env python3
"""Independent offline audit of the published LemonCrow baseline CSV join."""

import argparse
import csv
import hashlib
import json
import sys
from collections import Counter
from datetime import datetime, timezone
from decimal import Decimal
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-dir", type=Path, required=True)
    parser.add_argument("--evaluation-dir", type=Path, required=True)
    args = parser.parse_args()
    stem = "tbench_opus48_claudecode_2.1.205_"
    expected = {
        "tasks": "4380b0a5140595a49a8c86232873487ac31a95cc147e0f05800d9024b7cda28e",
        "turns": "4e073818e743a3f2f480a7c8b15129047b1ce1c31e39ca1324181ff05c6fdaf1",
    }
    data, sources = {}, []
    for label, digest in expected.items():
        path = args.baseline_dir / (stem + label + ".csv")
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != digest:
            raise ValueError(f"Pinned source hash mismatch: {path}")
        with path.open(newline="") as handle:
            data[label] = list(csv.DictReader(handle))
        sources.append({"path": str(path.resolve()), "sha256": actual,
                        "bytes": path.stat().st_size, "rows": len(data[label])})

    # Validate a one-to-one join before using a dictionary. No duplicate trial
    # can be silently overwritten, and no missing or extra turn can disappear.
    keys = {label: Counter(row["trial_name"] for row in rows)
            for label, rows in data.items()}
    assert keys["tasks"] == keys["turns"]
    assert all(count == 1 for group in keys.values() for count in group.values())
    turns = {row["trial_name"]: row for row in data["turns"]}
    ordinary = []
    equals_read, equals_both = 0, 0
    for task in data["tasks"]:
        turn = turns[task["trial_name"]]
        assert task["task"] == turn["task"]
        assert int(task["input_tokens"]) == int(turn["prompt_tokens"])
        assert int(task["output_tokens"]) == int(turn["completion_tokens"])
        reads, writes = int(turn["cache_read_tokens"]), int(turn["cache_creation_tokens"])
        fresh = int(turn["prompt_tokens"]) - reads - writes
        assert fresh >= 0
        ordinary.append(fresh)
        equals_read += int(task["cache_tokens"]) == reads
        equals_both += int(task["cache_tokens"]) == reads + writes

    def total(field):
        return sum(int(row[field]) for row in data["turns"])

    aggregate = {
        "trials": len(data["tasks"]),
        "tasks": len({row["task"] for row in data["tasks"]}),
        "pass": sum(row["result"] == "pass" for row in data["tasks"]),
        "fail": sum(row["result"] == "fail" for row in data["tasks"]),
        "other_reward": sum(row["result"] not in ("pass", "fail") for row in data["tasks"]),
        "missing_reported_cost": sum(not row["cost_usd"] for row in data["tasks"]),
        "prompt_tokens": total("prompt_tokens"),
        "cache_reads": total("cache_read_tokens"),
        "cache_writes": total("cache_creation_tokens"),
        "ordinary_input": sum(ordinary),
        "table_input_minus_table_cache": sum(int(row["input_tokens"]) - int(row["cache_tokens"])
                                             for row in data["tasks"]),
        "table_cache_equals_reads_rows": equals_read,
        "table_cache_equals_reads_plus_writes_rows": equals_both,
    }
    known_cost = sum((Decimal(row["cost_usd"]) for row in data["tasks"] if row["cost_usd"]), Decimal(0))
    author_path = args.evaluation_dir / "aggregation-results.json"
    author = json.loads(author_path.read_text())["lemoncrow_terminal_baseline"]
    mismatches = {key: {"independent": value, "author": author.get(key)}
                  for key, value in aggregate.items() if author.get(key) != value}
    assert not mismatches
    assert abs(Decimal(str(author["observed_reported_cost_sum_excludes_unknown"])) - known_cost) < Decimal("0.0000001")
    result = {
        "reviewer": "/root/headroom_lemoncrow",
        "author": "/root/rtk_leanctx",
        "executed_utc": datetime.now(timezone.utc).isoformat(),
        "argv": [sys.executable, *sys.argv],
        "cwd": str(Path.cwd()),
        "python_version": sys.version,
        "source_commit": "8aaee7ffa5b6f5f7e4a4a9a520af89ed527c7ec1",
        "sources": sources,
        "join": {"unique_trial_keys_each_file": True, "identical_key_sets": True,
                 "task_names_match": True, "input_and_output_totals_match_each_row": True,
                 "ordinary_nonnegative_each_row": True},
        "aggregate": aggregate,
        "known_reported_cost_decimal_excludes_34_unknown": str(known_cost),
        "author_comparison": {"path": str(author_path.resolve()),
                              "sha256": hashlib.sha256(author_path.read_bytes()).hexdigest(),
                              "all_compared_fields_match": True},
        "limits": "CSV class accounting only. No paid trial, invoice validation, candidate denominator, provider usage receipt, or task grading was reproduced.",
    }
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
