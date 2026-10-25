#!/usr/bin/env python3
"""Validate the planning pack. This is not a product test or a full STE checker."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"Expected a JSON object: {path}")
    return value


def sentences(value: str) -> list[str]:
    return [part.strip() for part in re.split(r"(?<=[.!?])\s+", value) if part.strip()]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent)
    parser.add_argument("--initial", action="store_true", help="Require untouched planning statuses.")
    parser.add_argument("--json", action="store_true", help="Print a machine-readable report.")
    args = parser.parse_args()
    root = args.root.resolve()
    failures: list[str] = []
    checks: list[dict[str, Any]] = []

    def check(name: str, good: bool, detail: Any = None) -> None:
        checks.append({"check": name, "passed": bool(good), "detail": detail})
        if not good:
            failures.append(name)

    required = ["README.md", "SPEC.md", "GOAL.md", "CONTRACTS.md", "SUBAGENTS.md", "PLAN.md",
                "ACCEPTANCE.md", "SOURCE-AUDIT.md", "TERMS.md", "TASKS.md", "TRACEABILITY.md",
                "task-graph.json", "acceptance-cases.json", "traceability.json"]
    missing = [name for name in required if not (root / name).is_file()]
    check("Required pack files exist", not missing, missing)
    if missing:
        result = {"passed": False, "checks": checks, "failures": failures}
        print(json.dumps(result, indent=2))
        return 1

    graph = read_json(root / "task-graph.json")
    cases_file = read_json(root / "acceptance-cases.json")
    trace = read_json(root / "traceability.json")
    tasks = graph["tasks"]
    cases = cases_file["cases"]
    task_ids = [task["id"] for task in tasks]
    case_ids = [case["id"] for case in cases]
    task_set, case_set = set(task_ids), set(case_ids)
    task_statuses = {"NOT_STARTED", "READY", "RUNNING", "REVIEW", "PASS", "FAIL", "BLOCKED"}
    case_statuses = {"NOT_RUN", "PASS", "FAIL", "BLOCKED"}
    bad_task_statuses = [{"id": task["id"], "status": task["status"]} for task in tasks
                         if task["status"] not in task_statuses]
    bad_case_statuses = [{"id": case["id"], "status": case["status"]} for case in cases
                         if case["status"] not in case_statuses]
    check("Task statuses use documented values", not bad_task_statuses, bad_task_statuses)
    check("Acceptance case statuses use documented values", not bad_case_statuses, bad_case_statuses)
    check("Task IDs are unique", len(task_set) == len(tasks))
    check("Case IDs are unique", len(case_set) == len(cases))
    check("Declared task count matches", graph["task_count"] == len(tasks), len(tasks))
    check("Declared case count matches", graph["acceptance_case_count"] == len(cases), len(cases))
    check("Expected task count", len(tasks) == 46, len(tasks))
    check("Expected acceptance case count", len(cases) == 125, len(cases))

    actual_digest = hashlib.sha256((root / "SPEC.md").read_bytes()).hexdigest()
    check("Original specification checksum is unchanged",
          actual_digest == graph["spec_sha256"] == trace["spec_sha256"], actual_digest)

    bad_dependencies = [(task["id"], dep) for task in tasks for dep in task["depends_on"]
                        if dep not in task_set or dep == task["id"]]
    check("Dependencies reference other known tasks", not bad_dependencies, bad_dependencies)
    done: set[str] = set()
    levels: list[list[str]] = []
    while len(done) < len(task_set):
        ready = [task["id"] for task in tasks if task["id"] not in done
                 and set(task["depends_on"]).issubset(done)]
        if not ready:
            break
        levels.append(ready)
        done.update(ready)
    check("Dependency graph has no cycle", done == task_set, {"ready_groups": len(levels)})
    check("Stored ready groups match dependencies",
          [set(group) for group in levels] == [set(group) for group in graph["topological_ready_groups"]])

    unknown_cases = [(task["id"], cid) for task in tasks for cid in task["test_cases"] if cid not in case_set]
    check("Task test references exist", not unknown_cases, unknown_cases)
    covered = {cid for task in tasks for cid in task["test_cases"]}
    check("Every case has a task owner", covered == case_set, sorted(case_set - covered))
    reverse_errors = []
    for case in cases:
        expected = {task["id"] for task in tasks if case["id"] in task["test_cases"]}
        if set(case["tasks"]) != expected:
            reverse_errors.append(case["id"])
    check("Case and task ownership maps agree", not reverse_errors, reverse_errors)

    plan = (root / "PLAN.md").read_text(encoding="utf-8")
    card_errors = []
    for task in tasks:
        card = root / task["task_file"]
        marker = f"### {task['id']} — "
        if not card.is_file() or marker not in plan:
            card_errors.append(task["id"])
            continue
        start = plan.index(marker, plan.index("## Detailed task cards"))
        end = plan.find("\n### ", start + len(marker))
        expected = plan[start:end if end != -1 else len(plan)].strip()
        if not card.read_text(encoding="utf-8").endswith(expected + "\n"):
            card_errors.append(task["id"])
    check("Individual task cards match the complete plan", not card_errors, card_errors)

    spec_lines = (root / "SPEC.md").read_text(encoding="utf-8").splitlines()
    actual_headings = [(i, re.sub(r"^#+ ", "", line)) for i, line in enumerate(spec_lines, 1)
                       if re.match(r"^#{1,2} ", line)]
    mapped_headings = [(section["start_line"], section["heading"]) for section in trace["sections"]]
    check("Every specification heading is mapped", actual_headings == mapped_headings,
          {"headings": len(actual_headings)})
    bad_trace = [section["heading"] for section in trace["sections"]
                 if not section["tasks"] or not set(section["tasks"]).issubset(task_set)]
    check("Traceability uses known task owners", not bad_trace, bad_trace)

    long_sentences = []
    sentence_count = 0
    for task in tasks:
        for field in ("steps", "acceptance", "remove_or_replace", "stop_conditions"):
            for item in task[field]:
                for sentence in sentences(item):
                    sentence_count += 1
                    count = len(sentence.split())
                    if count > 20:
                        long_sentences.append({"source": task["id"], "field": field,
                                               "words": count, "sentence": sentence})
    for case in cases:
        for field in ("setup", "action", "expected"):
            for sentence in sentences(case[field]):
                sentence_count += 1
                count = len(sentence.split())
                if count > 20:
                    long_sentences.append({"source": case["id"], "field": field,
                                           "words": count, "sentence": sentence})
    for line in (root / "GOAL.md").read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith(("#", "/goal")):
            continue
        for sentence in sentences(line):
            sentence_count += 1
            count = len(sentence.split())
            if count > 20:
                long_sentences.append({"source": "GOAL.md", "words": count, "sentence": sentence})
    check("Checked procedural sentences contain at most 20 whitespace-separated words",
          not long_sentences, {"sentences": sentence_count, "violations": long_sentences})

    if args.initial:
        bad_status = [task["id"] for task in tasks if task["status"] != "NOT_STARTED"]
        bad_status += [case["id"] for case in cases if case["status"] != "NOT_RUN"]
        check("Initial task and test statuses claim no execution", not bad_status, bad_status)
        check("Pack records no runtime tests or delegated reviews",
              graph["subagents_used_for_this_pack"] is False
              and graph["runtime_tests_executed_for_this_pack"] is False)

    result = {"passed": not failures, "scope": "Static planning-pack checks only",
              "not_checked": ["Product compilation", "Runtime capabilities", "Independent code review",
                              "Full ASD-STE100 dictionary conformity"],
              "counts": {"tasks": len(tasks), "cases": len(cases), "specification_sections": len(actual_headings),
                         "task_ready_groups": len(levels), "checked_sentences": sentence_count},
              "checks": checks, "failures": failures}
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        for item in checks:
            print(f"{'PASS' if item['passed'] else 'FAIL'}: {item['check']}")
        print("Static checks only. No product or full language-conformance result is implied.")
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Pack validation failed: {error}", file=sys.stderr)
        raise SystemExit(2)
