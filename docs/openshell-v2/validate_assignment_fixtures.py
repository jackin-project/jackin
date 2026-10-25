#!/usr/bin/env python3
"""Record causal fixtures for per-task acceptance result validation."""
from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parent
VALIDATOR = ROOT / "validate_pack.py"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


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


def set_task_status(pack: Path, task_id: str, status: str) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    task = next(task for task in graph["tasks"] if task["id"] == task_id)
    task["status"] = status
    graph_path.write_text(json.dumps(graph, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def set_shared_summary_with_blocked_owners(
    u02: str, t32: str, summary: str,
) -> Callable[[Path], None]:
    def mutate(pack: Path) -> None:
        set_task_status(pack, "U02", "BLOCKED")
        set_task_status(pack, "T32", "BLOCKED")
        update_cases(pack, set_shared_summary(u02, t32, summary))
    return mutate


def all_pass(pack: Path) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    for task in graph["tasks"]:
        task["status"] = "PASS"
    graph_path.write_text(json.dumps(graph, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    update_cases(pack, mark_all_cases_pass)


def inspect_report(stdout: str, exit_code: int) -> tuple[dict[str, Any] | None, list[str]]:
    if not stdout.strip():
        return None, ["stdout is empty"]
    try:
        report = json.loads(stdout)
    except json.JSONDecodeError:
        return None, ["stdout is not valid JSON"]
    if not isinstance(report, dict):
        return None, ["report must be a JSON object"]

    required = {"passed", "scope", "not_checked", "counts", "checks", "failures"}
    missing = sorted(required - set(report))
    if missing:
        return report, [f"report is missing required fields: {', '.join(missing)}"]

    type_errors = []
    if type(report["passed"]) is not bool:
        type_errors.append("passed")
    if not isinstance(report["scope"], str):
        type_errors.append("scope")
    if not isinstance(report["not_checked"], list) or any(
        not isinstance(item, str) for item in report["not_checked"]
    ):
        type_errors.append("not_checked")
    if not isinstance(report["counts"], dict):
        type_errors.append("counts")
    if not isinstance(report["checks"], list):
        type_errors.append("checks")
    if not isinstance(report["failures"], list) or any(
        not isinstance(item, str) for item in report["failures"]
    ):
        type_errors.append("failures")
    if type_errors:
        return report, [f"report has invalid field types: {', '.join(type_errors)}"]

    errors = []
    if report["scope"] != "Static planning-pack checks only":
        errors.append("scope does not match the validator contract")
    expected_not_checked = [
        "Product compilation",
        "Runtime capabilities",
        "Independent code review",
        "Full ASD-STE100 dictionary conformity",
    ]
    if report["not_checked"] != expected_not_checked:
        errors.append("not_checked does not match the validator contract")

    expected_count_names = {
        "tasks", "cases", "specification_sections", "task_ready_groups", "checked_sentences",
    }
    counts = report["counts"]
    if counts:
        if set(counts) != expected_count_names or any(
            type(value) is not int or value < 0 for value in counts.values()
        ):
            errors.append("counts have invalid keys or values")
    else:
        early_required_file_failure = (
            not report["passed"]
            and report["failures"] == ["Required pack files exist"]
            and isinstance(report["checks"], list)
            and len(report["checks"]) == 1
            and isinstance(report["checks"][0], dict)
            and report["checks"][0].get("check") == "Required pack files exist"
            and report["checks"][0].get("passed") is False
        )
        if not early_required_file_failure:
            errors.append("counts are empty outside the required-file failure report")

    check_names = []
    malformed_checks = False
    for check in report["checks"]:
        if (not isinstance(check, dict)
                or set(check) != {"check", "passed", "detail"}
                or not isinstance(check.get("check"), str)
                or not check.get("check")
                or type(check.get("passed")) is not bool
                or "detail" not in check):
            malformed_checks = True
            break
        check_names.append(check["check"])
    if malformed_checks or not check_names or len(check_names) != len(set(check_names)):
        errors.append("checks contain malformed or duplicate records")

    failed_checks = [
        check["check"] for check in report["checks"]
        if (isinstance(check, dict)
                and type(check.get("passed")) is bool
                and not check["passed"]
                and isinstance(check.get("check"), str))
    ]
    if report["failures"] != failed_checks:
        errors.append("failures do not match failed checks")
    if report["passed"] is not (not report["failures"]):
        errors.append("passed does not match failures")
    expected_exit = 0 if report["passed"] else 1
    if exit_code != expected_exit:
        errors.append("exit code does not match passed")
    return report, errors


def render_case(case: dict[str, Any]) -> str:
    tasks = ", ".join(case["tasks"])
    assignments = "; ".join(f"{task_id}=NOT_RUN" for task_id in case["tasks"])
    return "\n".join([
        f"### {case['id']} — {case['title']}",
        "",
        f"**Evidence:** {case['level']}. **Tasks:** {tasks}. **Initial status:** NOT_RUN. **Initial assignment status:** {assignments}.",
        "",
        f"**Setup:** {case['setup']}",
        "",
        f"**Action:** {case['action']}",
        "",
        f"**Required result:** {case['expected']}",
    ])


def sync_case_block(pack: Path, case: dict[str, Any]) -> None:
    path = pack / "ACCEPTANCE.md"
    document = path.read_text(encoding="utf-8")
    heading = f"### {case['id']} — {case['title']}"
    matches = list(re.finditer(r"(?m)^" + re.escape(heading) + r"$", document))
    if len(matches) != 1:
        raise ValueError(f"Expected one acceptance block for {case['id']}")
    start = matches[0].start()
    following = re.search(r"(?m)^### ", document[matches[0].end():])
    end = matches[0].end() + following.start() if following else len(document)
    suffix = document[end:]
    separator = "\n\n" if suffix else "\n"
    document = document[:start] + render_case(case).rstrip() + separator + suffix.lstrip("\n")
    path.write_text(document.rstrip() + "\n", encoding="utf-8")


def fake_validator(temp_dir: Path, stdout: str, stderr: str = "", exit_code: int = 0) -> Path:
    path = temp_dir / "fixture-validator.py"
    source = (
        "import sys\n"
        f"sys.stdout.write({stdout!r})\n"
        f"sys.stderr.write({stderr!r})\n"
        f"raise SystemExit({exit_code})\n"
    )
    path.write_text(source, encoding="utf-8")
    return path


def run_fixture(name: str, mutate: Callable[[Path], None], expected_exit: int,
                expected_failures: list[str], initial: bool = False,
                validator_executable: Path | None = None,
                fake_stdout: str | None = None, fake_stderr: str = "",
                expect_report_match: bool = True,
                expected_report_errors: list[str] | None = None) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="jackin-assignment-") as temp_dir:
        temporary_root = Path(temp_dir)
        pack = temporary_root / "pack"
        shutil.copytree(ROOT, pack, ignore=shutil.ignore_patterns("__pycache__"))
        mutate(pack)
        validator_copy = pack / "validate_pack.py"
        if fake_stdout is not None:
            executable = fake_validator(temporary_root, fake_stdout, fake_stderr, expected_exit)
        else:
            executable = (validator_executable or validator_copy).resolve()
        command = [sys.executable, str(executable), "--root", str(pack), "--json"]
        if initial:
            command.append("--initial")
        process = subprocess.run(command, capture_output=True, text=True, check=False)
        report, report_errors = inspect_report(process.stdout, process.returncode)
        actual_failures = report.get("failures") if isinstance(report, dict) else None
        expected_report_passed = expected_exit == 0
        report_match = (
            process.returncode == expected_exit
            and not report_errors
            and isinstance(report, dict)
            and report.get("passed") is expected_report_passed
            and actual_failures == expected_failures
        )
        errors_match = expected_report_errors is None or report_errors == expected_report_errors
        passed = (report_match is expect_report_match) and errors_match
        return {
            "scenario": name,
            "argv": command,
            "exit_code": process.returncode,
            "expected_exit_code": expected_exit,
            "validator_executable": str(executable),
            "validator_executable_sha256": sha256(executable),
            "validator_copy_present": validator_copy.is_file(),
            "validator_copy_sha256": sha256(validator_copy) if validator_copy.is_file() else None,
            "report_match": report_match,
            "expected_report_match": expect_report_match,
            "report_errors": report_errors,
            "expected_report_errors": expected_report_errors,
            "expected_failures": expected_failures,
            "actual_failures": actual_failures,
            "passed": passed,
            "stdout": process.stdout,
            "stderr": process.stderr,
            "fixture_sha256": {
                "task-graph.json": sha256(pack / "task-graph.json"),
                "acceptance-cases.json": sha256(pack / "acceptance-cases.json"),
                "ACCEPTANCE.md": sha256(pack / "ACCEPTANCE.md"),
            },
        }


def main() -> int:
    receipts: list[dict[str, Any]] = []

    receipts.append(run_fixture(
        "shared PASS summary requires every owner",
        set_shared_summary_with_blocked_owners("PASS", "PASS", "PASS"),
        0, [],
    ))
    receipts.append(run_fixture(
        "task PASS with its own assignment results",
        all_pass,
        0, [],
    ))
    receipts.append(run_fixture(
        "mixed PASS and NOT_RUN summarizes as NOT_RUN",
        set_shared_summary_with_blocked_owners("PASS", "NOT_RUN", "NOT_RUN"),
        0, [],
    ))
    receipts.append(run_fixture(
        "mixed PASS and BLOCKED summarizes as BLOCKED",
        set_shared_summary_with_blocked_owners("PASS", "BLOCKED", "BLOCKED"),
        0, [],
    ))
    receipts.append(run_fixture(
        "FAIL takes precedence over BLOCKED",
        set_shared_summary_with_blocked_owners("BLOCKED", "FAIL", "FAIL"),
        0, [],
    ))

    def unstarted_assignment(status: str) -> Callable[[Path], None]:
        summaries = {"PASS": "NOT_RUN", "FAIL": "FAIL", "BLOCKED": "BLOCKED"}
        return lambda pack: update_cases(
            pack, set_shared_summary(status, "NOT_RUN", summaries[status]),
        )

    for status in ("PASS", "FAIL", "BLOCKED"):
        receipts.append(run_fixture(
            f"NOT_STARTED owner cannot have {status} assignment",
            unstarted_assignment(status),
            1, ["NOT_STARTED tasks have NOT_RUN assignments"],
        ))

    def blocked_owner_with_prior_pass(pack: Path) -> None:
        set_task_status(pack, "U02", "BLOCKED")
        update_cases(pack, set_shared_summary("PASS", "NOT_RUN", "NOT_RUN"))
    receipts.append(run_fixture(
        "BLOCKED owner can retain its passing assignment while T32 waits",
        blocked_owner_with_prior_pass,
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
        set_shared_summary_with_blocked_owners("PASS", "NOT_RUN", "PASS"),
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

    def duplicate_case_owner(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            cases["A01"]["tasks"].append(cases["A01"]["tasks"][0])
        update_cases(pack, update)
        sync_case_block(pack, case_map(pack)["A01"])
    receipts.append(run_fixture(
        "duplicate case owner fails when its Markdown block is synchronized",
        duplicate_case_owner,
        1, ["Acceptance cases have unique string task owners"],
    ))

    def remove_validator_copy(pack: Path) -> None:
        (pack / "validate_pack.py").unlink()
    receipts.append(run_fixture(
        "copied pack without validator fails under the real external validator",
        remove_validator_copy,
        1, ["Required pack files exist"], validator_executable=VALIDATOR,
    ))

    def mutate_acceptance_prefix(pack: Path) -> None:
        path = pack / "ACCEPTANCE.md"
        document = path.read_bytes()
        marker = b"## Detailed cases\n"
        prefix_end = document.index(marker)
        prefix = document[:prefix_end]
        old = b"Do not remove a row silently"
        if old not in prefix:
            raise ValueError("Expected target-row policy text in acceptance prefix")
        path.write_bytes(prefix.replace(old, b"Remove a row silently", 1) + document[prefix_end:])
    receipts.append(run_fixture(
        "acceptance prefix mutation violates its fixed checksum",
        mutate_acceptance_prefix,
        1, ["Acceptance policy before detailed cases matches fixed checksum"],
    ))

    def delete_acceptance_prefix_line(pack: Path) -> None:
        path = pack / "ACCEPTANCE.md"
        document = path.read_bytes()
        marker = b"## Detailed cases\n"
        prefix_end = document.index(marker)
        lines = document[:prefix_end].splitlines(keepends=True)
        target = b"Use `--case` repeatedly for several selected cases.\n"
        if target not in lines:
            raise ValueError("Expected harness policy line in acceptance prefix")
        lines.remove(target)
        path.write_bytes(b"".join(lines) + document[prefix_end:])
    receipts.append(run_fixture(
        "acceptance prefix deletion violates its fixed checksum",
        delete_acceptance_prefix_line,
        1, ["Acceptance policy before detailed cases matches fixed checksum"],
    ))

    def assignment_progress(pack: Path) -> None:
        update_cases(pack, set_shared_summary("PASS", "NOT_RUN", "NOT_RUN"))
    receipts.append(run_fixture(
        "initial mode rejects assignment progress", assignment_progress,
        1, ["NOT_STARTED tasks have NOT_RUN assignments",
            "Initial task, case, and assignment statuses claim no execution"], initial=True,
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
        all_pass(pack)
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        tasks = {task["id"]: task for task in graph["tasks"]}
        def depends_on_any(task_id: str, roots: set[str]) -> bool:
            pending = list(tasks[task_id]["depends_on"])
            seen: set[str] = set()
            while pending:
                dependency = pending.pop()
                if dependency in roots:
                    return True
                if dependency not in seen:
                    seen.add(dependency)
                    pending.extend(tasks[dependency]["depends_on"])
            return False

        for task in graph["tasks"]:
            if task["id"] == "T27":
                task["status"] = "BLOCKED"
            elif task["id"] != "T33" and depends_on_any(task["id"], {"T27", "T33"}):
                task["status"] = "BLOCKED"
        tasks["T33"]["status"] = "RUNNING"
        assert all(tasks[dependency]["status"] == "PASS"
                   for dependency in tasks["T33"]["depends_on"] if dependency != "T27")
        graph_path.write_text(json.dumps(graph, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
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

    def validator_report(passed: bool, failures: list[str], checks: list[dict[str, Any]]) -> str:
        return json.dumps({
            "passed": passed,
            "scope": "Static planning-pack checks only",
            "not_checked": [
                "Product compilation",
                "Runtime capabilities",
                "Independent code review",
                "Full ASD-STE100 dictionary conformity",
            ],
            "counts": {
                "tasks": 46,
                "cases": 125,
                "specification_sections": 30,
                "task_ready_groups": 20,
                "checked_sentences": 1523,
            },
            "checks": checks,
            "failures": failures,
        })

    def parser_fixture(name: str, stdout: str, report_errors: list[str], exit_code: int = 0,
                       expected_exit: int = 0, expected_failures: list[str] | None = None) -> None:
        receipts.append(run_fixture(
            name,
            lambda _pack: None,
            expected_exit,
            expected_failures or [],
            fake_stdout=stdout,
            expect_report_match=False,
            expected_report_errors=report_errors,
        ))

    parser_fixture("empty validator stdout is rejected", "", ["stdout is empty"])
    parser_fixture("invalid JSON validator stdout is rejected", "{not-json", ["stdout is not valid JSON"])
    parser_fixture("non-object validator JSON is rejected", "[]", ["report must be a JSON object"])
    parser_fixture(
        "validator report with missing fields is rejected",
        json.dumps({"passed": True}),
        ["report is missing required fields: checks, counts, failures, not_checked, scope"],
    )
    wrong_types = {
        "passed": "true",
        "scope": None,
        "not_checked": "none",
        "counts": [],
        "checks": {},
        "failures": 1,
    }
    parser_fixture(
        "validator report with wrong field types is rejected",
        json.dumps(wrong_types),
        ["report has invalid field types: passed, scope, not_checked, counts, checks, failures"],
    )
    inconsistent_counts = json.loads(validator_report(True, [], [
        {"check": "check A", "passed": True, "detail": None},
    ]))
    inconsistent_counts["counts"] = {}
    parser_fixture(
        "validator report with inconsistent counts is rejected",
        json.dumps(inconsistent_counts),
        ["counts are empty outside the required-file failure report"],
    )
    parser_fixture(
        "contradictory passed and failures fields are rejected",
        validator_report(True, ["check A"], [{"check": "check A", "passed": False, "detail": None}]),
        ["passed does not match failures"],
    )
    parser_fixture(
        "check list inconsistent with failures is rejected even at exit zero",
        validator_report(True, [], [{"check": "check A", "passed": False, "detail": None}]),
        ["failures do not match failed checks"],
    )
    parser_fixture(
        "malformed check records are rejected",
        validator_report(True, [], [{"check": "check A", "passed": True}]),
        ["checks contain malformed or duplicate records"],
    )
    parser_fixture(
        "duplicate check names are rejected",
        validator_report(True, [], [
            {"check": "check A", "passed": True, "detail": None},
            {"check": "check A", "passed": True, "detail": None},
        ]),
        ["checks contain malformed or duplicate records"],
    )
    wrong_scope = json.loads(validator_report(True, [], [
        {"check": "check A", "passed": True, "detail": None},
    ]))
    wrong_scope["scope"] = "Runtime tests passed"
    parser_fixture(
        "validator report with mismatched scope is rejected",
        json.dumps(wrong_scope),
        ["scope does not match the validator contract"],
    )
    parser_fixture(
        "report exit code must agree with passed even at exit zero",
        validator_report(False, ["check A"], [{"check": "check A", "passed": False, "detail": None}]),
        ["exit code does not match passed"],
    )
    parser_fixture(
        "unexpected extra failure name does not match exact expectation",
        validator_report(False, ["expected failure", "unexpected failure"], [
            {"check": "expected failure", "passed": False, "detail": None},
            {"check": "unexpected failure", "passed": False, "detail": None},
        ]),
        [], exit_code=1, expected_exit=1, expected_failures=["expected failure"],
    )

    payload = {
        "passed": all(receipt["passed"] for receipt in receipts),
        "scope": "Static validator fixtures only. No product or runtime tests are performed.",
        "source_sha256": {
            "validate_pack.py": sha256(VALIDATOR),
            "validate_assignment_fixtures.py": sha256(Path(__file__)),
            "task-graph.json": sha256(ROOT / "task-graph.json"),
            "acceptance-cases.json": sha256(ROOT / "acceptance-cases.json"),
            "ACCEPTANCE.md": sha256(ROOT / "ACCEPTANCE.md"),
        },
        "fixtures": receipts,
    }
    print(json.dumps(payload, indent=2, ensure_ascii=False))
    return 0 if payload["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
