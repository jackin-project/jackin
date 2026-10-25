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

ORIGINAL_SPEC_SHA256 = "857e2bf002676af9fbc2b8eac0b0aea4d7657d873b90b3a64e8abfe767523a3e"


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"Expected a JSON object: {path}")
    return value


def sentences(value: str) -> list[str]:
    return [part.strip() for part in re.split(r"(?<=[.!?])\s+", value) if part.strip()]


def render_task_card(task: dict[str, Any]) -> str:
    dependencies = ", ".join(task["depends_on"]) or "None"
    cases = ", ".join(task["test_cases"]) or "None"
    lines = [
        f"### {task['id']} — {task['title']}",
        "",
        f"**Owner:** {task['owner']}.",
        "",
        f"**Dependencies:** {dependencies}.",
        "",
        f"**Source basis:** {task['source_basis']}.",
        "",
        "#### Write scope",
        "",
        *[f"- {item}" for item in task["write_scope"]],
        "",
        "#### Required inputs",
        "",
        *[f"- {item}" for item in task["inputs"]],
        "",
        f"**Task test directory:** `{task['test_write_scope']}`.",
        "",
        "#### Procedure",
        "",
        *[f"{index}. {item}" for index, item in enumerate(task["steps"], 1)],
        "",
        "#### Pass conditions",
        "",
        *[f"- {item}" for item in task["acceptance"]],
        "",
        f"**Acceptance cases:** {cases}.",
        "",
        "#### Removal and replacement",
        "",
        *[f"- {item}" for item in task["remove_or_replace"]],
        "",
        "#### Required evidence",
        "",
        *[f"- `{item}`" for item in task["required_artifacts"]],
        "",
        "#### Stop conditions",
        "",
        *[f"- {item}" for item in task["stop_conditions"]],
        "",
        f"**Review:** {task['review_rule']}",
        "",
        "---",
    ]
    return "\n".join(lines)


def render_acceptance_case(case: dict[str, Any]) -> str:
    tasks = ", ".join(case["tasks"])
    return "\n".join(
        [
            f"### {case['id']} — {case['title']}",
            "",
            f"**Evidence:** {case['level']}. **Tasks:** {tasks}. **Initial status:** NOT_RUN.",
            "",
            f"**Setup:** {case['setup']}",
            "",
            f"**Action:** {case['action']}",
            "",
            f"**Required result:** {case['expected']}",
        ]
    )


def render_task_index(tasks: list[dict[str, Any]]) -> list[str]:
    lines = [
        "| ID | Task | Subagent | Dependencies |",
        "| --- | --- | --- | --- |",
    ]
    for task in tasks:
        dependencies = ", ".join(task["depends_on"]) or "None"
        lines.append(
            f"| {task['id']} | {task['title']} | {task['owner']} | {dependencies} |"
        )
    return lines


def task_index_table(document: str, heading: str, next_heading: str | None) -> list[str] | None:
    heading_pattern = re.compile(rf"(?m)^{re.escape(heading)}\s*$")
    matches = list(heading_pattern.finditer(document))
    if len(matches) != 1:
        return None
    start = matches[0].end()
    end = len(document)
    if next_heading is not None:
        next_pattern = re.compile(rf"(?m)^{re.escape(next_heading)}\s*$")
        next_matches = [match for match in next_pattern.finditer(document) if match.start() > start]
        if len(next_matches) != 1:
            return None
        end = next_matches[0].start()
    section = document[start:end]
    return [line.strip() for line in section.splitlines() if line.lstrip().startswith("|")]


def render_traceability_table(sections: list[dict[str, Any]]) -> list[str]:
    lines = [
        "| Specification section | Lines | Responsible tasks |",
        "| --- | --- | --- |",
    ]
    for section in sections:
        tasks = ", ".join(section["tasks"])
        lines.append(
            f"| {section['heading']} | {section['start_line']}–{section['end_line']} | {tasks} |"
        )
    return lines


def traceability_table(document: str) -> list[str] | None:
    source_heading = re.compile(r"(?m)^## Source classes\s*$")
    matches = list(source_heading.finditer(document))
    if len(matches) != 1:
        return None
    section = document[:matches[0].start()]
    return [line.strip() for line in section.splitlines() if line.lstrip().startswith("|")]


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
                "PACK-CHECKS.md", "task-graph.json", "acceptance-cases.json", "traceability.json"]
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
    check("Original specification matches its fixed checksum and both manifests",
          actual_digest == ORIGINAL_SPEC_SHA256
          and graph["spec_sha256"] == ORIGINAL_SPEC_SHA256
          and trace["spec_sha256"] == ORIGINAL_SPEC_SHA256,
          {"actual": actual_digest, "graph": graph["spec_sha256"], "traceability": trace["spec_sha256"]})

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
    stored_groups = graph["topological_ready_groups"]
    stored_group_ids = [task_id for group in stored_groups for task_id in group]
    stored_group_counts: dict[str, int] = {}
    for task_id in stored_group_ids:
        stored_group_counts[task_id] = stored_group_counts.get(task_id, 0) + 1
    stored_group_duplicates = sorted(task_id for task_id, count in stored_group_counts.items() if count > 1)
    expected_group_lists = [sorted(group) for group in levels]
    stored_group_lists = [sorted(group) for group in stored_groups]
    check("Stored ready groups match dependencies without duplicate task IDs",
          not stored_group_duplicates and stored_group_lists == expected_group_lists,
          {"duplicate_task_ids": stored_group_duplicates,
           "expected_group_count": len(expected_group_lists),
           "stored_group_count": len(stored_group_lists)})

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

    expected_index = render_task_index(tasks)
    task_index = task_index_table((root / "TASKS.md").read_text(encoding="utf-8"),
                                  "# Task index", None)
    check("Task graph fields match TASKS index table", task_index == expected_index,
          {"expected_rows": len(expected_index),
           "actual_rows": len(task_index) if task_index is not None else None})

    plan = (root / "PLAN.md").read_text(encoding="utf-8")
    plan_index = task_index_table(plan, "## Task index", "## Detailed task cards")
    check("Task graph fields match PLAN task index table", plan_index == expected_index,
          {"expected_rows": len(expected_index),
           "actual_rows": len(plan_index) if plan_index is not None else None})
    task_section_start = plan.find("## Detailed task cards")
    task_section = plan[task_section_start:] if task_section_start != -1 else ""
    plan_headings = list(re.finditer(r"(?m)^### (.+)$", task_section))
    expected_task_headings = [f"{task['id']} — {task['title']}" for task in tasks]
    actual_task_headings = [match.group(1) for match in plan_headings]
    check("PLAN task headings match the task graph",
          task_section_start != -1 and actual_task_headings == expected_task_headings,
          {"expected_count": len(expected_task_headings), "actual_count": len(actual_task_headings),
           "headings_match": actual_task_headings == expected_task_headings})

    plan_task_errors = []
    card_errors = []
    for task in tasks:
        expected = render_task_card(task).strip()
        marker = f"### {task['id']} — {task['title']}"
        matching_plan = [match for match in plan_headings if match.group(0) == marker]
        if len(matching_plan) != 1:
            plan_task_errors.append(task["id"])
        else:
            start = matching_plan[0].start()
            next_heading = next((match for match in plan_headings if match.start() > start), None)
            end = next_heading.start() if next_heading else len(task_section)
            if task_section[start:end].strip() != expected:
                plan_task_errors.append(task["id"])

        card = root / task["task_file"]
        if not card.is_file():
            card_errors.append(task["id"])
            continue
        card_text = card.read_text(encoding="utf-8")
        card_headings = list(re.finditer(r"(?m)^### (.+)$", card_text))
        matching_card = [match for match in card_headings if match.group(0) == marker]
        if len(matching_card) != 1 or card_text[matching_card[0].start():].strip() != expected:
            card_errors.append(task["id"])
    check("Task graph fields match PLAN task cards", not plan_task_errors, plan_task_errors)
    check("Task graph fields match individual task cards", not card_errors, card_errors)

    acceptance = (root / "ACCEPTANCE.md").read_text(encoding="utf-8")
    case_section_start = acceptance.find("## Detailed cases")
    case_section = acceptance[case_section_start:] if case_section_start != -1 else ""
    case_headings = list(re.finditer(r"(?m)^### (.+)$", case_section))
    expected_case_headings = [f"{case['id']} — {case['title']}" for case in cases]
    actual_case_headings = [match.group(1) for match in case_headings]
    heading_counts = {heading: actual_case_headings.count(heading) for heading in set(actual_case_headings)}
    duplicate_case_headings = sorted(heading for heading, count in heading_counts.items() if count > 1)
    missing_case_headings = sorted(set(expected_case_headings) - set(actual_case_headings))
    unexpected_case_headings = sorted(set(actual_case_headings) - set(expected_case_headings))
    check("Acceptance case headings have no missing, duplicate, or extra blocks",
          case_section_start != -1
          and not duplicate_case_headings
          and not missing_case_headings
          and not unexpected_case_headings
          and actual_case_headings == expected_case_headings,
          {"duplicates": duplicate_case_headings, "missing": missing_case_headings,
           "unexpected": unexpected_case_headings})
    case_content_errors = []
    for case in cases:
        marker = f"### {case['id']} — {case['title']}"
        matching = [match for match in case_headings if match.group(0) == marker]
        if len(matching) != 1:
            case_content_errors.append(case["id"])
            continue
        start = matching[0].start()
        next_heading = next((match for match in case_headings if match.start() > start), None)
        end = next_heading.start() if next_heading else len(case_section)
        if case_section[start:end].strip() != render_acceptance_case(case).strip():
            case_content_errors.append(case["id"])
    check("Acceptance case fields match the acceptance-case manifest",
          not case_content_errors, case_content_errors)

    spec_lines = (root / "SPEC.md").read_text(encoding="utf-8").splitlines()
    actual_headings = [(i, re.sub(r"^#+ ", "", line)) for i, line in enumerate(spec_lines, 1)
                       if re.match(r"^#{1,2} ", line)]
    expected_trace_sections = [
        {"start_line": start_line,
         "end_line": actual_headings[index + 1][0] - 1
         if index + 1 < len(actual_headings) else len(spec_lines),
         "heading": heading}
        for index, (start_line, heading) in enumerate(actual_headings)
    ]
    mapped_headings = [(section["start_line"], section["heading"]) for section in trace["sections"]]
    expected_headings = [(section["start_line"], section["heading"])
                         for section in expected_trace_sections]
    check("Every specification heading is mapped", expected_headings == mapped_headings,
          {"expected_count": len(expected_headings), "actual_count": len(mapped_headings)})
    trace_range_errors = []
    for index, expected in enumerate(expected_trace_sections):
        if index >= len(trace["sections"]):
            trace_range_errors.append({"index": index, "missing": expected})
            continue
        section = trace["sections"][index]
        actual = {key: section.get(key) for key in ("start_line", "end_line", "heading")}
        if actual != expected:
            trace_range_errors.append({"index": index, "expected": expected, "actual": actual})
    if len(trace["sections"]) > len(expected_trace_sections):
        trace_range_errors.extend(
            {"index": index, "unexpected": trace["sections"][index]}
            for index in range(len(expected_trace_sections), len(trace["sections"]))
        )
    check("Traceability ranges match exact specification sections",
          not trace_range_errors, trace_range_errors)
    trace_doc = (root / "TRACEABILITY.md").read_text(encoding="utf-8")
    actual_trace_table = traceability_table(trace_doc)
    expected_trace_table = render_traceability_table(trace["sections"])
    check("Traceability table matches its manifest", actual_trace_table == expected_trace_table,
          {"expected_rows": len(expected_trace_table),
           "actual_rows": len(actual_trace_table) if actual_trace_table is not None else None})
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
