#!/usr/bin/env python3
"""Record causal fixtures for per-task acceptance result validation."""
from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parent
VALIDATOR = ROOT / "validate_pack.py"
RECEIPTS = ROOT / "validation" / "assignment-results-receipts.json"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def task_map(pack: Path) -> dict[str, dict[str, Any]]:
    return {task["id"]: task for task in read_json(pack / "task-graph.json")["tasks"]}


def case_map(pack: Path) -> dict[str, dict[str, Any]]:
    return {case["id"]: case for case in read_json(pack / "acceptance-cases.json")["cases"]}


def update_cases(pack: Path, update: Callable[[dict[str, dict[str, Any]]], None]) -> None:
    path = pack / "acceptance-cases.json"
    data = read_json(path)
    cases = {case["id"]: case for case in data["cases"]}
    update(cases)
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def mark_all_cases_pass(cases: dict[str, dict[str, Any]]) -> None:
    for case in cases.values():
        case["status"] = "PASS"
        for result in case["assignment_results"]:
            result["status"] = "PASS"


def set_assignment(case: dict[str, Any], task_id: str, status: str) -> None:
    for result in case["assignment_results"]:
        if result["task_id"] == task_id:
            result["status"] = status
            return
    raise ValueError(f"Missing assignment: {case['id']} / {task_id}")


def set_shared_summary(u02: str, t32: str, summary: str) -> Callable[[dict[str, dict[str, Any]]], None]:
    def update(cases: dict[str, dict[str, Any]]) -> None:
        case = cases["A22"]
        set_assignment(case, "U02", u02)
        set_assignment(case, "T32", t32)
        case["status"] = summary
    return update


def all_pass(pack: Path) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    for task in graph["tasks"]:
        task["status"] = "PASS"
    graph_path.write_text(json.dumps(graph, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    update_cases(pack, mark_all_cases_pass)


def run_fixture(name: str, mutate: Callable[[Path], None], expected_exit: int,
                expected_failures: list[str], initial: bool = False) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="jackin-assignment-") as temp_dir:
        pack = Path(temp_dir) / "pack"
        shutil.copytree(ROOT, pack, ignore=shutil.ignore_patterns("__pycache__"))
        mutate(pack)
        command = [sys.executable, str(pack / "validate_pack.py"), "--root", str(pack), "--json"]
        if initial:
            command.append("--initial")
        process = subprocess.run(command, capture_output=True, text=True, check=False)
        try:
            report = json.loads(process.stdout)
            failures = report["failures"]
        except (json.JSONDecodeError, KeyError, TypeError):
            report = None
            failures = []
        passed = process.returncode == expected_exit and all(item in failures for item in expected_failures)
        return {
            "scenario": name,
            "argv": command,
            "exit_code": process.returncode,
            "expected_exit_code": expected_exit,
            "expected_failures": expected_failures,
            "actual_failures": failures,
            "passed": passed,
            "stdout": process.stdout,
            "stderr": process.stderr,
            "fixture_sha256": {
                "task-graph.json": sha256(pack / "task-graph.json"),
                "acceptance-cases.json": sha256(pack / "acceptance-cases.json"),
                "validate_pack.py": sha256(pack / "validate_pack.py"),
            },
        }


def main() -> int:
    receipts: list[dict[str, Any]] = []

    receipts.append(run_fixture(
        "shared PASS summary requires every owner",
        lambda pack: update_cases(pack, set_shared_summary("PASS", "PASS", "PASS")),
        0, [],
    ))
    receipts.append(run_fixture(
        "task PASS with its own assignment results",
        all_pass,
        0, [],
    ))
    receipts.append(run_fixture(
        "mixed PASS and NOT_RUN summarizes as NOT_RUN",
        lambda pack: update_cases(pack, set_shared_summary("PASS", "NOT_RUN", "NOT_RUN")),
        0, [],
    ))
    receipts.append(run_fixture(
        "mixed PASS and BLOCKED summarizes as BLOCKED",
        lambda pack: update_cases(pack, set_shared_summary("PASS", "BLOCKED", "BLOCKED")),
        0, [],
    ))
    receipts.append(run_fixture(
        "FAIL takes precedence over BLOCKED",
        lambda pack: update_cases(pack, set_shared_summary("BLOCKED", "FAIL", "FAIL")),
        0, [],
    ))

    def upstream_pass(pack: Path) -> None:
        all_pass(pack)
        update_cases(pack, set_shared_summary("PASS", "NOT_RUN", "NOT_RUN"))
    receipts.append(run_fixture(
        "upstream PASS cannot close downstream assignment",
        upstream_pass,
        1, ["PASS tasks have passing assigned case results"],
    ))

    receipts.append(run_fixture(
        "legacy summary cannot PASS with incomplete assignment",
        lambda pack: update_cases(pack, set_shared_summary("PASS", "NOT_RUN", "PASS")),
        1, ["Case status matches assignment summary"],
    ))

    def legacy_manifest(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            for case in cases.values():
                case.pop("assignment_results")
        update_cases(pack, update)
    receipts.append(run_fixture(
        "legacy manifest without assignment_results is rejected",
        legacy_manifest,
        1, ["Assignment results have exact unique case owners and valid statuses"],
    ))

    def malformed(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            cases["A22"]["assignment_results"][0]["evidence"] = "unexpected"
        update_cases(pack, update)
    receipts.append(run_fixture(
        "malformed assignment row is rejected", malformed,
        1, ["Assignment results have exact unique case owners and valid statuses"],
    ))

    def invalid_assignment_status(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            cases["A22"]["assignment_results"][0]["status"] = "PASSED"
        update_cases(pack, update)
    receipts.append(run_fixture(
        "undocumented assignment status is rejected", invalid_assignment_status,
        1, ["Assignment results have exact unique case owners and valid statuses"],
    ))

    def duplicate(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            rows = cases["A22"]["assignment_results"]
            rows[1] = dict(rows[0])
        update_cases(pack, update)
    receipts.append(run_fixture(
        "duplicate task assignment is rejected", duplicate,
        1, ["Assignment results have exact unique case owners and valid statuses"],
    ))

    def missing(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            cases["A22"]["assignment_results"].pop()
        update_cases(pack, update)
    receipts.append(run_fixture(
        "missing task assignment is rejected", missing,
        1, ["Assignment results have exact unique case owners and valid statuses"],
    ))

    def extra(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            cases["A22"]["assignment_results"].append({"task_id": "UNKNOWN", "status": "NOT_RUN"})
        update_cases(pack, update)
    receipts.append(run_fixture(
        "extra task assignment is rejected", extra,
        1, ["Assignment results have exact unique case owners and valid statuses"],
    ))

    def assignment_progress(pack: Path) -> None:
        update_cases(pack, set_shared_summary("PASS", "NOT_RUN", "NOT_RUN"))
    receipts.append(run_fixture(
        "initial mode rejects assignment progress", assignment_progress,
        1, ["Initial task, case, and assignment statuses claim no execution"], initial=True,
    ))

    def malformed_initial_assignment(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            cases["A22"]["assignment_results"][0] = None
        update_cases(pack, update)
    receipts.append(run_fixture(
        "initial mode rejects malformed assignment rows cleanly", malformed_initial_assignment,
        1, ["Assignment results have exact unique case owners and valid statuses",
            "Initial task, case, and assignment statuses claim no execution"], initial=True,
    ))

    def t33_without_checkpoint_dependency(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        tasks = {task["id"]: task for task in graph["tasks"]}
        def depends_on_checkpoint(task_id: str) -> bool:
            pending = list(tasks[task_id]["depends_on"])
            seen: set[str] = set()
            while pending:
                dependency = pending.pop()
                if dependency == "T27":
                    return True
                if dependency not in seen:
                    seen.add(dependency)
                    pending.extend(tasks[dependency]["depends_on"])
            return False
        for task in graph["tasks"]:
            task["status"] = "NOT_STARTED" if depends_on_checkpoint(task["id"]) or task["id"] == "T27" else "PASS"
        tasks["T33"]["status"] = "RUNNING"
        graph_path.write_text(json.dumps(graph, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        update_cases(pack, mark_all_cases_pass)
    receipts.append(run_fixture(
        "T33 waits for T27 checkpoint completion",
        t33_without_checkpoint_dependency,
        1, ["Task progress waits for completed dependencies"],
    ))

    def blocked_task(pack: Path) -> None:
        all_pass(pack)
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        graph["tasks"][[task["id"] for task in graph["tasks"]].index("T40")]["status"] = "BLOCKED"
        graph_path.write_text(json.dumps(graph, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        def update(cases: dict[str, dict[str, Any]]) -> None:
            for case_id in ("A123", "A124", "A125"):
                case = cases[case_id]
                for result in case["assignment_results"]:
                    result["status"] = "BLOCKED"
                case["status"] = "BLOCKED"
        update_cases(pack, update)
    receipts.append(run_fixture(
        "BLOCKED task and assignment remain blocked without failing validation",
        blocked_task,
        0, [],
    ))

    def no_case_task(pack: Path) -> None:
        all_pass(pack)
        if task_map(pack)["U06"]["test_cases"]:
            raise ValueError("Expected U06 to retain manual completion with no assigned cases")
    receipts.append(run_fixture(
        "task without acceptance cases keeps completion rule",
        no_case_task,
        0, [],
    ))

    payload = {
        "scope": "Static validator fixtures only. No product or runtime tests are performed.",
        "source_sha256": {
            "validate_pack.py": sha256(VALIDATOR),
            "validate_assignment_fixtures.py": sha256(Path(__file__)),
            "task-graph.json": sha256(ROOT / "task-graph.json"),
            "acceptance-cases.json": sha256(ROOT / "acceptance-cases.json"),
        },
        "fixtures": receipts,
    }
    RECEIPTS.parent.mkdir(parents=True, exist_ok=True)
    RECEIPTS.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    for receipt in receipts:
        print(f"{'PASS' if receipt['passed'] else 'FAIL'}: {receipt['scenario']} (exit {receipt['exit_code']})")
    print(f"Raw receipts: {RECEIPTS}")
    return 0 if all(receipt["passed"] for receipt in receipts) else 1


if __name__ == "__main__":
    raise SystemExit(main())
