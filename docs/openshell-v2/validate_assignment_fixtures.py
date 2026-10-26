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
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT))
from validate_pack import (  # noqa: E402
    DuplicateJSONKeyError,
    canonical_check_inventory,
    render_task_card,
    render_task_packet,
    strict_json_loads,
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot_files(root: Path) -> dict[str, str]:
    return {
        path.relative_to(root).as_posix(): sha256(path)
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def input_state(digest: str | None) -> dict[str, str | None]:
    if digest is None:
        return {"state": "missing", "sha256": None}
    return {"state": "present", "sha256": digest}


def path_state(path: Path) -> dict[str, str | None]:
    return input_state(sha256(path) if path.is_file() else None)


def snapshot_states(snapshot: dict[str, str]) -> dict[str, dict[str, str | None]]:
    return {path: input_state(digest) for path, digest in snapshot.items()}


def snapshot_changes(before: dict[str, str], after: dict[str, str]) -> dict[str, Any]:
    return {
        path: {"before": input_state(before.get(path)), "after": input_state(after.get(path))}
        for path in sorted(set(before) | set(after))
        if before.get(path) != after.get(path)
    }


def read_json(path: Path) -> dict[str, Any]:
    value = strict_json_loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"Expected JSON object: {path}")
    return value


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def normalize_status_fields(pack: Path) -> tuple[list[dict[str, Any]], list[str]]:
    changes: list[dict[str, Any]] = []
    errors: list[str] = []

    for relative_path in ("task-graph.json", "acceptance-cases.json"):
        path = pack / relative_path
        try:
            data = read_json(path)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            errors.append(f"{relative_path}: {error}")
            continue
        changed = False

        def set_existing_status(record: dict[str, Any], field_path: str, target: str) -> None:
            nonlocal changed
            if "status" not in record or record["status"] == target:
                return
            changes.append({"path": relative_path, "field": f"{field_path}.status",
                            "before": record["status"], "after": target})
            record["status"] = target
            changed = True

        if relative_path == "task-graph.json":
            tasks = data.get("tasks")
            if isinstance(tasks, list):
                for index, task in enumerate(tasks):
                    if isinstance(task, dict):
                        set_existing_status(task, f"tasks[{index}]", "NOT_STARTED")
        else:
            set_existing_status(data, "root", "NOT_RUN")
            cases = data.get("cases")
            if isinstance(cases, list):
                for case_index, case in enumerate(cases):
                    if not isinstance(case, dict):
                        continue
                    set_existing_status(case, f"cases[{case_index}]", "NOT_RUN")
                    assignments = case.get("assignment_results")
                    if isinstance(assignments, list):
                        for assignment_index, assignment in enumerate(assignments):
                            if isinstance(assignment, dict):
                                set_existing_status(
                                    assignment,
                                    f"cases[{case_index}].assignment_results[{assignment_index}]",
                                    "NOT_RUN",
                                )
        if changed:
            write_json(path, data)
    return changes, errors


def derive_expected_counts(pack: Path) -> dict[str, int] | None:
    try:
        graph = read_json(pack / "task-graph.json")
        cases_file = read_json(pack / "acceptance-cases.json")
        tasks = graph["tasks"]
        cases = cases_file["cases"]
        spec_lines = (pack / "SPEC.md").read_text(encoding="utf-8").splitlines()
        goal_lines = (pack / "GOAL.md").read_text(encoding="utf-8").splitlines()
        if not isinstance(tasks, list) or not isinstance(cases, list):
            return None

        dependency_lists_valid = True
        for task in tasks:
            if not isinstance(task, dict):
                return None
            dependencies = task.get("depends_on")
            if (not isinstance(dependencies, list)
                    or any(not isinstance(dependency, str) for dependency in dependencies)
                    or len(dependencies) != len(set(dependencies))):
                dependency_lists_valid = False
        task_ids = [task["id"] for task in tasks]
        task_set = set(task_ids)
        ready_groups = 0
        if dependency_lists_valid:
            done: set[str] = set()
            while len(done) < len(task_set):
                ready = [task["id"] for task in tasks
                         if task["id"] not in done and set(task["depends_on"]).issubset(done)]
                if not ready:
                    break
                ready_groups += 1
                done.update(ready)

        sentence_count = 0

        def count_sentences(value: str) -> int:
            return sum(bool(sentence.strip())
                       for sentence in re.split(r"(?<=[.!?])\s+", value))

        for task in tasks:
            for field in ("steps", "acceptance", "remove_or_replace", "stop_conditions"):
                for item in task[field]:
                    sentence_count += count_sentences(item)
        for case in cases:
            for field in ("setup", "action", "expected"):
                sentence_count += count_sentences(case[field])
        for line in goal_lines:
            if not line.strip() or line.startswith(("#", "/goal")):
                continue
            sentence_count += count_sentences(line)

        specification_sections = sum(
            bool(re.match(r"^#{1,2} ", line)) for line in spec_lines
        )
        return {
            "tasks": len(tasks),
            "cases": len(cases),
            "specification_sections": specification_sections,
            "task_ready_groups": ready_groups,
            "checked_sentences": sentence_count,
        }
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError):
        return None


def status_summary(statuses: list[str]) -> str:
    if "FAIL" in statuses:
        return "FAIL"
    if "BLOCKED" in statuses:
        return "BLOCKED"
    if statuses and all(status == "PASS" for status in statuses):
        return "PASS"
    return "NOT_RUN"


def task_map(pack: Path) -> dict[str, dict[str, Any]]:
    return {task["id"]: task for task in read_json(pack / "task-graph.json")["tasks"]}


def case_map(pack: Path) -> dict[str, dict[str, Any]]:
    return {case["id"]: case for case in read_json(pack / "acceptance-cases.json")["cases"]}


def update_cases(pack: Path, update: Callable[[dict[str, dict[str, Any]]], None]) -> None:
    path = pack / "acceptance-cases.json"
    data = read_json(path)
    cases = {case["id"]: case for case in data["cases"]}
    update(cases)
    data["status"] = status_summary([case["status"] for case in data["cases"]])
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


def set_single_summary(task_id: str, status: str) -> Callable[[dict[str, dict[str, Any]]], None]:
    def update(cases: dict[str, dict[str, Any]]) -> None:
        case = cases["A01"]
        set_assignment(case, task_id, status)
        case["status"] = status
    return update


def set_task_status(pack: Path, task_id: str, status: str) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    task = next(task for task in graph["tasks"] if task["id"] == task_id)
    task["status"] = status
    graph_path.write_text(json.dumps(graph, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def set_task_states(pack: Path, statuses: dict[str, str]) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    tasks = {task["id"]: task for task in graph["tasks"]}
    for task_id, status in statuses.items():
        tasks[task_id]["status"] = status

    nonpass_roots = {task_id for task_id, status in statuses.items() if status != "PASS"}
    descendants: set[str] = set()
    frontier = list(nonpass_roots)
    while frontier:
        dependency = frontier.pop()
        for task in graph["tasks"]:
            task_id = task["id"]
            if dependency in task["depends_on"] and task_id not in descendants:
                descendants.add(task_id)
                frontier.append(task_id)
    descendants.difference_update(statuses)
    for task_id in descendants:
        tasks[task_id]["status"] = "BLOCKED"
    write_json(graph_path, graph)

    def update(cases: dict[str, dict[str, Any]]) -> None:
        for case in cases.values():
            for result in case["assignment_results"]:
                if result["task_id"] in descendants:
                    result["status"] = "NOT_RUN"
            case["status"] = status_summary([row["status"] for row in case["assignment_results"]])
    update_cases(pack, update)


def sync_task_packet(pack: Path, task_id: str) -> None:
    graph = read_json(pack / "task-graph.json")
    task = next(task for task in graph["tasks"] if task["id"] == task_id)
    rendered_packet = render_task_packet(task).strip()
    rendered_card = render_task_card(task).strip()
    marker = f"### {task_id} — {task['title']}"

    card_path = pack / task["task_file"]
    card_path.write_text(rendered_packet + "\n", encoding="utf-8")

    plan_path = pack / "PLAN.md"
    plan = plan_path.read_text(encoding="utf-8")
    matches = list(re.finditer(r"(?m)^" + re.escape(marker) + r"$", plan))
    if len(matches) != 1:
        raise ValueError(f"Expected one PLAN block for {task_id}")
    start = matches[0].start()
    following = re.search(r"(?m)^### ", plan[matches[0].end():])
    end = matches[0].end() + following.start() if following else len(plan)
    plan_path.write_text(plan[:start] + rendered_card + "\n\n" + plan[end:].lstrip("\n"), encoding="utf-8")


def all_pass(pack: Path) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    for task in graph["tasks"]:
        task["status"] = "PASS"
    graph_path.write_text(json.dumps(graph, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    update_cases(pack, mark_all_cases_pass)


def inspect_report(stdout: str, exit_code: int, initial: bool = False) -> tuple[dict[str, Any] | None, list[str]]:
    if not stdout.strip():
        return None, ["stdout is empty"]
    try:
        report = strict_json_loads(stdout)
    except DuplicateJSONKeyError as error:
        return None, [str(error)]
    except json.JSONDecodeError:
        return None, ["stdout is not valid JSON"]
    if not isinstance(report, dict):
        return None, ["report must be a JSON object"]

    required = {"passed", "initial", "scope", "not_checked", "counts", "checks", "failures"}
    missing = sorted(required - set(report))
    if missing:
        return report, [f"report is missing required fields: {', '.join(missing)}"]
    unexpected = sorted(set(report) - required)

    type_errors = []
    if type(report["passed"]) is not bool:
        type_errors.append("passed")
    if type(report["initial"]) is not bool:
        type_errors.append("initial")
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
    if unexpected:
        errors.append(f"report has unexpected fields: {', '.join(unexpected)}")
    if report["initial"] is not initial:
        errors.append("initial flag does not match invocation")
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
    check_records = report["checks"]
    early_required_file_failure = (
        not report["passed"]
        and counts == {}
        and report["failures"] == ["Required pack files exist"]
        and len(check_records) == 1
        and isinstance(check_records[0], dict)
        and check_records[0].get("check") == "Required pack files exist"
        and check_records[0].get("passed") is False
        and isinstance(check_records[0].get("detail"), list)
        and bool(check_records[0]["detail"])
        and all(isinstance(item, str) and item for item in check_records[0]["detail"])
    )
    structured_input_error = (
        not report["passed"]
        and counts == {}
        and report["failures"] == ["Planning pack input is valid"]
        and len(check_records) == 1
        and isinstance(check_records[0], dict)
        and check_records[0].get("check") == "Planning pack input is valid"
        and check_records[0].get("passed") is False
        and isinstance(check_records[0].get("detail"), str)
        and bool(check_records[0]["detail"])
    )
    if counts:
        if set(counts) != expected_count_names or any(
            type(value) is not int or value < 0 for value in counts.values()
        ):
            errors.append("counts have invalid keys or values")
    else:
        if not early_required_file_failure and not structured_input_error:
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
    malformed_checks = malformed_checks or not check_names or len(check_names) != len(set(check_names))
    if malformed_checks:
        errors.append("checks contain malformed or duplicate records")
    elif not early_required_file_failure and not structured_input_error:
        if check_names != list(canonical_check_inventory(initial)):
            errors.append("checks do not match the canonical ordered inventory")

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
    if structured_input_error:
        if exit_code != 2:
            errors.append("exit code does not match structured input-error report")
    else:
        expected_exit = 0 if report["passed"] else 1
        if exit_code != expected_exit:
            errors.append("exit code does not match passed")
    return report, errors


def sync_trace_table(pack: Path) -> None:
    trace_path = pack / "traceability.json"
    trace = read_json(trace_path)
    trace_doc_path = pack / "TRACEABILITY.md"
    document = trace_doc_path.read_text(encoding="utf-8")
    header = "| Specification section | Lines | Responsible tasks |"
    heading = "## Source classes"
    if document.count(header) != 1 or document.count(heading) != 1:
        raise ValueError("Expected one traceability table and source-classes heading")
    prefix, suffix = document.split(header, 1)
    _, remainder = suffix.split(heading, 1)
    rows = [
        header,
        "| --- | --- | --- |",
        *[
            f"| {section['heading']} | {section['start_line']}–{section['end_line']} | "
            f"{', '.join(section['tasks'])} |"
            for section in trace["sections"]
        ],
    ]
    trace_doc_path.write_text(prefix + "\n".join(rows) + "\n\n" + heading + remainder,
                             encoding="utf-8")


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
                expected_report_errors: list[str] | None = None,
                expected_report_details: dict[str, Any] | None = None,
                expected_execution_errors: list[str] | None = None,
                pre_normalization_mutation: Callable[[Path], None] | None = None,
                expected_normalization_diff: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="jackin-assignment-") as temp_dir:
        temporary_root = Path(temp_dir)
        pack = temporary_root / "pack"
        shutil.copytree(ROOT, pack)
        source_inputs = snapshot_files(pack)
        if pre_normalization_mutation is not None:
            pre_normalization_mutation(pack)
        pre_normalization_inputs = snapshot_files(pack)
        normalization_diff, normalization_errors = normalize_status_fields(pack)
        normalized_inputs = snapshot_files(pack)
        mutate(pack)
        inputs_after = snapshot_files(pack)
        expected_counts = derive_expected_counts(pack)
        validator_copy = pack / "validate_pack.py"
        if fake_stdout is not None:
            executable = fake_validator(temporary_root, fake_stdout, fake_stderr, expected_exit)
        else:
            executable = (validator_executable or validator_copy).resolve()
        command = [sys.executable, str(executable), "--root", str(pack), "--json"]
        if initial:
            command.append("--initial")
        execution_inputs_before = snapshot_files(pack)
        executable_before = path_state(executable)
        validator_copy_before = path_state(validator_copy)
        process = subprocess.run(command, capture_output=True, text=True, check=False)
        execution_inputs_after = snapshot_files(pack)
        executable_after = path_state(executable)
        validator_copy_after = path_state(validator_copy)
        report, report_errors = inspect_report(process.stdout, process.returncode, initial=initial)
        if report is not None and not report_errors and isinstance(report.get("counts"), dict):
            if report["counts"] and expected_counts is None:
                report_errors.append("counts cannot be derived from tested fixture inputs")
            elif report["counts"] and report["counts"] != expected_counts:
                report_errors.append("counts do not match tested fixture inputs")
        actual_failures = report.get("failures") if isinstance(report, dict) else None
        expected_report_passed = expected_exit == 0
        check_records = report.get("checks") if isinstance(report, dict) else None
        actual_report_details = (
            {
                record.get("check"): record.get("detail")
                for record in check_records
                if isinstance(record, dict) and isinstance(record.get("check"), str)
            }
            if isinstance(check_records, list) else {}
        )
        detail_errors = []
        if expected_report_details is not None:
            for check_name, expected_detail in expected_report_details.items():
                if actual_report_details.get(check_name) != expected_detail:
                    detail_errors.append({
                        "check": check_name,
                        "expected": expected_detail,
                        "actual": actual_report_details.get(check_name),
                    })
        execution_changes = snapshot_changes(execution_inputs_before, execution_inputs_after)
        execution_errors = [
            f"pack input changed during validator execution: {path}"
            for path in execution_changes
        ]
        if executable_before != executable_after:
            execution_errors.append("validator executable changed during execution")
        if executable_before["state"] == "missing":
            execution_errors.append("validator executable was missing before execution")
        if executable_after["state"] == "missing":
            execution_errors.append("validator executable was missing after execution")
        expected_execution_errors = expected_execution_errors or []
        report_match = (
            process.returncode == expected_exit
            and not report_errors
            and isinstance(report, dict)
            and report.get("passed") is expected_report_passed
            and actual_failures == expected_failures
            and not detail_errors
        )
        errors_match = expected_report_errors is None or report_errors == expected_report_errors
        execution_match = execution_errors == expected_execution_errors
        normalization_match = (
            expected_normalization_diff is None
            or normalization_diff == expected_normalization_diff
        )
        passed = (
            (report_match is expect_report_match)
            and errors_match
            and execution_match
            and normalization_match
            and not normalization_errors
        )
        return {
            "scenario": name,
            "argv": command,
            "exit_code": process.returncode,
            "expected_exit_code": expected_exit,
            "validator_executable": str(executable),
            "validator_executable_before": executable_before,
            "validator_executable_after": executable_after,
            "validator_executable_sha256": executable_after["sha256"],
            "validator_copy_before": validator_copy_before,
            "validator_copy_after": validator_copy_after,
            "validator_copy_present": validator_copy.is_file(),
            "validator_copy_sha256": sha256(validator_copy) if validator_copy.is_file() else None,
            "report_match": report_match,
            "expected_report_match": expect_report_match,
            "report_errors": report_errors,
            "expected_report_errors": expected_report_errors,
            "report_details_match": not detail_errors,
            "report_detail_errors": detail_errors,
            "expected_report_details": expected_report_details,
            "actual_report_details": actual_report_details,
            "expected_counts": expected_counts,
            "actual_counts": report.get("counts") if isinstance(report, dict) else None,
            "expected_failures": expected_failures,
            "actual_failures": actual_failures,
            "expected_execution_errors": expected_execution_errors,
            "execution_errors": execution_errors,
            "execution_integrity_match": execution_match,
            "passed": passed,
            "stdout": process.stdout,
            "stderr": process.stderr,
            "source_input_sha256": snapshot_states(source_inputs),
            "normalization_diff": normalization_diff,
            "normalization_errors": normalization_errors,
            "expected_normalization_diff": expected_normalization_diff,
            "normalization_match": normalization_match,
            "pre_normalization_fixture_mutations": snapshot_changes(source_inputs,
                                                                      pre_normalization_inputs),
            "normalized_input_sha256": snapshot_states(normalized_inputs),
            "fixture_mutations": snapshot_changes(normalized_inputs, inputs_after),
            "pre_execution_input_sha256": snapshot_states(execution_inputs_before),
            "post_execution_input_sha256": snapshot_states(execution_inputs_after),
            "execution_input_changes": execution_changes,
            "tested_inputs": {
                path: input_state(inputs_after.get(path))
                for path in sorted(set(source_inputs) | set(inputs_after))
            },
            "changed_inputs": snapshot_changes(source_inputs, inputs_after),
            "fixture_sha256": {
                "task-graph.json": inputs_after.get("task-graph.json"),
                "acceptance-cases.json": inputs_after.get("acceptance-cases.json"),
                "ACCEPTANCE.md": inputs_after.get("ACCEPTANCE.md"),
            },
        }


def validate_original_board() -> dict[str, Any]:
    command = [sys.executable, str(VALIDATOR), "--root", str(ROOT), "--json"]
    inputs_before = snapshot_files(ROOT)
    executable_before = path_state(VALIDATOR)
    process = subprocess.run(command, capture_output=True, text=True, check=False)
    inputs_after = snapshot_files(ROOT)
    executable_after = path_state(VALIDATOR)
    report, report_errors = inspect_report(process.stdout, process.returncode, initial=False)
    expected_counts = derive_expected_counts(ROOT)
    if report is not None and not report_errors and isinstance(report.get("counts"), dict):
        if report["counts"] and expected_counts is None:
            report_errors.append("counts cannot be derived from original board inputs")
        elif report["counts"] and report["counts"] != expected_counts:
            report_errors.append("counts do not match original board inputs")
    input_changes = snapshot_changes(inputs_before, inputs_after)
    execution_errors = [
        f"original board input changed during ordinary validation: {path}"
        for path in input_changes
    ]
    if executable_before != executable_after:
        execution_errors.append("original validator executable changed during ordinary validation")
    return {
        "passed": (
            process.returncode == 0
            and not report_errors
            and isinstance(report, dict)
            and report.get("passed") is True
            and not execution_errors
        ),
        "argv": command,
        "exit_code": process.returncode,
        "validator_executable_before": executable_before,
        "validator_executable_after": executable_after,
        "input_sha256_before": snapshot_states(inputs_before),
        "input_sha256_after": snapshot_states(inputs_after),
        "input_changes": input_changes,
        "expected_counts": expected_counts,
        "actual_counts": report.get("counts") if isinstance(report, dict) else None,
        "report_errors": report_errors,
        "execution_errors": execution_errors,
        "report": report,
        "stdout": process.stdout,
        "stderr": process.stderr,
    }


def main() -> int:
    receipts: list[dict[str, Any]] = []
    original_source_before = snapshot_files(ROOT)

    def completed_t32_shared_summary(u02: str, t32: str, summary: str) -> Callable[[Path], None]:
        def mutate(pack: Path) -> None:
            all_pass(pack)
            set_task_states(pack, {"T32": "BLOCKED"})
            update_cases(pack, set_shared_summary(u02, t32, summary))
        return mutate

    def sibling_fail_precedence(pack: Path) -> None:
        all_pass(pack)
        set_task_states(pack, {"T35": "FAIL", "T36": "BLOCKED"})
        def update(cases: dict[str, dict[str, Any]]) -> None:
            case = cases["A117"]
            for task_id, status in {
                "T35": "FAIL", "T36": "BLOCKED", "T37": "PASS", "T38": "NOT_RUN",
            }.items():
                set_assignment(case, task_id, status)
            case["status"] = "FAIL"
        update_cases(pack, update)

    receipts.append(run_fixture(
        "shared PASS summary requires every owner",
        all_pass,
        0, [],
    ))
    receipts.append(run_fixture(
        "task PASS with its own assignment results",
        all_pass,
        0, [],
    ))
    receipts.append(run_fixture(
        "mixed PASS and NOT_RUN summarizes as NOT_RUN",
        completed_t32_shared_summary("PASS", "NOT_RUN", "NOT_RUN"),
        0, [],
    ))
    receipts.append(run_fixture(
        "mixed PASS and BLOCKED summarizes as BLOCKED",
        completed_t32_shared_summary("PASS", "BLOCKED", "BLOCKED"),
        0, [],
    ))
    receipts.append(run_fixture(
        "FAIL takes precedence over BLOCKED",
        sibling_fail_precedence,
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
            1, ["NOT_STARTED and READY tasks have NOT_RUN assignments"],
        ))

    def ready_assignment(status: str) -> Callable[[Path], None]:
        def mutate(pack: Path) -> None:
            set_task_status(pack, "P00", "READY")
            update_cases(pack, set_single_summary("P00", status))
        return mutate

    for status in ("PASS", "FAIL", "BLOCKED"):
        receipts.append(run_fixture(
            f"READY owner cannot have {status} assignment",
            ready_assignment(status),
            1, ["NOT_STARTED and READY tasks have NOT_RUN assignments"],
        ))
    receipts.append(run_fixture(
        "READY owner can retain NOT_RUN assignment",
        ready_assignment("NOT_RUN"),
        0, [],
    ))

    def blocked_owner_with_prior_pass(pack: Path) -> None:
        all_pass(pack)
        set_task_states(pack, {"U02": "BLOCKED"})
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
        completed_t32_shared_summary("PASS", "NOT_RUN", "PASS"),
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
        expected_report_details={"Required pack files exist": ["validate_pack.py"]},
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
        1, ["NOT_STARTED and READY tasks have NOT_RUN assignments",
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

    def seed_progress_before_normalization(pack: Path) -> None:
        set_task_status(pack, "P00", "PASS")
        update_cases(pack, set_single_summary("P00", "PASS"))
    receipts.append(run_fixture(
        "temporary normalization changes only P00 and A01 status fields",
        lambda _pack: None,
        0, [], initial=True,
        pre_normalization_mutation=seed_progress_before_normalization,
        expected_normalization_diff=[
            {"path": "task-graph.json", "field": "tasks[0].status",
             "before": "PASS", "after": "NOT_STARTED"},
            {"path": "acceptance-cases.json", "field": "cases[0].status",
             "before": "PASS", "after": "NOT_RUN"},
            {"path": "acceptance-cases.json", "field": "cases[0].assignment_results[0].status",
             "before": "PASS", "after": "NOT_RUN"},
        ],
    ))

    def consistent_progressed_p00_a01(pack: Path) -> None:
        set_task_status(pack, "P00", "PASS")
        update_cases(pack, set_single_summary("P00", "PASS"))
    receipts.append(run_fixture(
        "consistent progressed P00 and A01 board passes ordinary validation",
        consistent_progressed_p00_a01,
        0, [],
    ))

    def missing_a01_assignment_control(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            cases["A01"]["assignment_results"].clear()
        update_cases(pack, update)
    receipts.append(run_fixture(
        "invalid temporary source board with missing A01 assignment is not repaired",
        lambda _pack: None,
        1, ["Assignment results have exact unique case owners and valid statuses"],
        pre_normalization_mutation=missing_a01_assignment_control,
    ))

    def t33_without_checkpoint_dependency(pack: Path) -> None:
        all_pass(pack)
        set_task_states(pack, {"T27": "BLOCKED", "T33": "RUNNING"})
        tasks = task_map(pack)
        assert all(tasks[dependency]["status"] == "PASS"
                   for dependency in tasks["T33"]["depends_on"] if dependency != "T27")
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

    def failed_task_with_unmet_dependencies(pack: Path) -> None:
        set_task_status(pack, "T40", "FAIL")
        def update(cases: dict[str, dict[str, Any]]) -> None:
            for case_id in ("A123", "A124", "A125"):
                case = cases[case_id]
                for result in case["assignment_results"]:
                    result["status"] = "PASS"
                case["status"] = "PASS"
        update_cases(pack, update)
    receipts.append(run_fixture(
        "FAIL task must wait for completed dependencies",
        failed_task_with_unmet_dependencies,
        1, ["Task progress waits for completed dependencies"],
    ))

    def failed_task_after_dependencies_with_unfinished_cases(pack: Path) -> None:
        all_pass(pack)
        set_task_status(pack, "T40", "FAIL")
        def update(cases: dict[str, dict[str, Any]]) -> None:
            for case_id in ("A123", "A124", "A125"):
                case = cases[case_id]
                status = "PASS" if case_id == "A124" else "NOT_RUN"
                for result in case["assignment_results"]:
                    result["status"] = status
                case["status"] = status
        update_cases(pack, update)
    receipts.append(run_fixture(
        "FAIL task remains valid after completed dependencies with NOT_RUN and PASS cases",
        failed_task_after_dependencies_with_unfinished_cases,
        0, [],
    ))

    def blocked_task_with_unmet_dependencies(pack: Path) -> None:
        set_task_status(pack, "T40", "BLOCKED")
    receipts.append(run_fixture(
        "BLOCKED task may wait on an incomplete dependency with NOT_RUN cases",
        blocked_task_with_unmet_dependencies,
        0, [],
    ))

    def blocked_task_with_incomplete_dependencies_and_result(status: str) -> Callable[[Path], None]:
        def mutate(pack: Path) -> None:
            set_task_status(pack, "T32", "BLOCKED")
            def update(cases: dict[str, dict[str, Any]]) -> None:
                case = cases["A22"]
                set_assignment(case, "T32", status)
                case["status"] = status_summary([row["status"] for row in case["assignment_results"]])
            update_cases(pack, update)
        return mutate

    for status in ("PASS", "FAIL", "BLOCKED"):
        receipts.append(run_fixture(
            f"BLOCKED T32 with incomplete dependencies cannot retain {status} assignments",
            blocked_task_with_incomplete_dependencies_and_result(status),
            1, ["BLOCKED tasks with incomplete dependencies have NOT_RUN assignments"],
        ))

    def blocked_t32_with_incomplete_dependencies_not_run(pack: Path) -> None:
        set_task_status(pack, "T32", "BLOCKED")
    receipts.append(run_fixture(
        "BLOCKED T32 with incomplete dependencies keeps NOT_RUN assignments",
        blocked_t32_with_incomplete_dependencies_not_run,
        0, [],
    ))

    def blocked_task_after_completed_dependencies_retains_results(pack: Path) -> None:
        all_pass(pack)
        set_task_status(pack, "T40", "BLOCKED")
        def update(cases: dict[str, dict[str, Any]]) -> None:
            for case_id, status in (("A123", "PASS"), ("A124", "FAIL"), ("A125", "BLOCKED")):
                case = cases[case_id]
                set_assignment(case, "T40", status)
                case["status"] = status
        update_cases(pack, update)
    receipts.append(run_fixture(
        "BLOCKED task with completed dependencies retains prior case results",
        blocked_task_after_completed_dependencies_retains_results,
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

    def duplicate_task_case_reference(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        p04 = next(task for task in graph["tasks"] if task["id"] == "P04")
        p04["test_cases"].append(p04["test_cases"][0])
        write_json(graph_path, graph)
        sync_task_packet(pack, "P04")
    receipts.append(run_fixture(
        "duplicate task case reference is rejected with synchronized task cards",
        duplicate_task_case_reference,
        1, ["Task test_cases contain unique string IDs"],
    ))

    def nonstring_task_case_reference(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        p04 = next(task for task in graph["tasks"] if task["id"] == "P04")
        p04["test_cases"].append(123)
        write_json(graph_path, graph)
        sync_task_packet(pack, "P04")
    receipts.append(run_fixture(
        "non-string task case reference is rejected before ownership set logic",
        nonstring_task_case_reference,
        1, ["Task test_cases contain unique string IDs"],
    ))

    def stale_manifest_summary(pack: Path, status: str) -> None:
        path = pack / "acceptance-cases.json"
        data = read_json(path)
        data["status"] = status
        write_json(path, data)
    receipts.append(run_fixture(
        "acceptance manifest root status cannot claim PASS for incomplete cases",
        lambda pack: stale_manifest_summary(pack, "PASS"),
        1, ["Acceptance manifest status matches the case summary"],
    ))
    receipts.append(run_fixture(
        "acceptance manifest root status rejects undocumented values",
        lambda pack: stale_manifest_summary(pack, "COMPLETE"),
        1, ["Acceptance manifest status uses documented values",
            "Acceptance manifest status matches the case summary"],
    ))

    def manifest_failure_precedence(pack: Path) -> None:
        all_pass(pack)
        set_task_states(pack, {"T35": "FAIL", "T36": "BLOCKED"})

        def update(cases: dict[str, dict[str, Any]]) -> None:
            for case_id, task_id, status in (
                ("A55", "T35", "FAIL"), ("A112", "T36", "BLOCKED"),
            ):
                case = cases[case_id]
                set_assignment(case, task_id, status)
                case["status"] = status
        update_cases(pack, update)
    receipts.append(run_fixture(
        "manifest summary gives FAIL precedence over BLOCKED",
        manifest_failure_precedence,
        0, [],
    ))

    def corrupt_completion_rule(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        next(task for task in graph["tasks"] if task["id"] == "T14")["completion_rule"] = "All checks pass."
        write_json(graph_path, graph)
    receipts.append(run_fixture(
        "task completion rule cannot diverge from the shared evidence contract",
        corrupt_completion_rule,
        1, ["Task completion rules match the fixed evidence contract"],
    ))

    def set_review_rule(pack: Path, review_rule: str) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        next(task for task in graph["tasks"] if task["id"] == "T14")["review_rule"] = review_rule
        write_json(graph_path, graph)
        sync_task_packet(pack, "T14")

    receipts.append(run_fixture(
        "task review rule rejects self-review after synchronized cards",
        lambda pack: set_review_rule(
            pack,
            "The task owner reviews the result. The integration subagent reruns affected tests after merge.",
        ),
        1, ["Task review rules match the different-subagent and post-merge contract"],
    ))
    receipts.append(run_fixture(
        "task review rule requires a post-merge integration rerun",
        lambda pack: set_review_rule(pack, "A different subagent reviews the result."),
        1, ["Task review rules match the different-subagent and post-merge contract"],
    ))

    def remove_standard_task_artifact(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(task for task in graph["tasks"] if task["id"] == "P00")
        task["required_artifacts"].remove("evidence/openshell-v2/P00/review.json")
        write_json(graph_path, graph)
        sync_task_packet(pack, "P00")
    receipts.append(run_fixture(
        "canonical review receipt cannot be removed with synchronized cards",
        remove_standard_task_artifact,
        1, ["Task required artifacts match canonical evidence set and report path"],
    ))

    def relocate_report_path(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(task for task in graph["tasks"] if task["id"] == "P00")
        task["report_path"] = "evidence/elsewhere/P00/implementation.json"
        write_json(graph_path, graph)
        sync_task_packet(pack, "P00")
    receipts.append(run_fixture(
        "report path cannot move from its canonical task receipt directory",
        relocate_report_path,
        1, ["Task required artifacts match canonical evidence set and report path"],
    ))

    def remove_upstream_change_artifact(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(task for task in graph["tasks"] if task["id"] == "U00")
        task["required_artifacts"].remove("evidence/openshell-v2/U00/upstream-change.json")
        write_json(graph_path, graph)
        sync_task_packet(pack, "U00")
    receipts.append(run_fixture(
        "upstream task keeps its task-specific change receipt",
        remove_upstream_change_artifact,
        1, ["Task required artifacts match canonical evidence set and report path"],
    ))

    def remove_early_live_artifact(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(task for task in graph["tasks"] if task["id"] == "P02")
        task["required_artifacts"].remove("evidence/openshell-v2/P02/early-live-records.json")
        write_json(graph_path, graph)
        sync_task_packet(pack, "P02")
    receipts.append(run_fixture(
        "P02 keeps the required early LIVE records with synchronized cards",
        remove_early_live_artifact,
        1, ["Task required artifacts match canonical evidence set and report path"],
    ))

    def weaken_early_live_task(pack: Path, task_id: str, field: str, exact_text: str) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(task for task in graph["tasks"] if task["id"] == task_id)
        task[field].remove(exact_text)
        write_json(graph_path, graph)
        sync_task_packet(pack, task_id)
    receipts.append(run_fixture(
        "P02 cannot omit actual early LIVE execution",
        lambda pack: weaken_early_live_task(
            pack, "P02", "steps",
            "Record actual P02 LIVE runs for A05 and A06 before P03 starts.",
        ),
        1, ["P02 bootstrap records, P04 imports, and T12 reruns A05/A06"],
    ))
    receipts.append(run_fixture(
        "T12 cannot reuse the earlier P02 LIVE results",
        lambda pack: weaken_early_live_task(
            pack, "T12", "steps",
            "Run separate fresh A05 and A06 LIVE tests with new run IDs after the P04 harness is available.",
        ),
        1, ["P02 bootstrap records, P04 imports, and T12 reruns A05/A06"],
    ))
    receipts.append(run_fixture(
        "P04 must validate and import early records without closing T12",
        lambda pack: weaken_early_live_task(
            pack, "P04", "steps",
            "Validate and import the P02 A05 and A06 records without assigning them to T12.",
        ),
        1, ["P02 bootstrap records, P04 imports, and T12 reruns A05/A06"],
    ))

    def weaken_registration_scope(pack: Path, task_id: str, old: str, new: str) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(task for task in graph["tasks"] if task["id"] == task_id)
        task["write_scope"].remove(old)
        task["write_scope"].append(new)
        write_json(graph_path, graph)
        sync_task_packet(pack, task_id)
    receipts.append(run_fixture(
        "P04 registration is limited to enum and dispatch through Integration",
        lambda pack: weaken_registration_scope(
            pack, "P04",
            "crates/tools/jackin-xtask/src/main.rs: v2 command enum and dispatch only, through the Integration subagent",
            "crates/tools/jackin-xtask/src/main.rs",
        ),
        1, ["P04 harness command registration and P05 build wiring scopes agree"],
    ))
    receipts.append(run_fixture(
        "P05 retains only workspace and build graph registration",
        lambda pack: weaken_registration_scope(
            pack, "P05",
            "crates/tools/jackin-xtask and test-support workspace and build graph registration only",
            "crates/tools/jackin-xtask and test-support registration",
        ),
        1, ["P04 harness command registration and P05 build wiring scopes agree"],
    ))
    receipts.append(run_fixture(
        "P05 must register xtask and test support in the workspace graph",
        lambda pack: weaken_early_live_task(
            pack, "P05", "steps",
            "Register jackin-xtask and test-support in the production workspace and build graph.",
        ),
        1, ["P04 harness command registration and P05 build wiring scopes agree"],
    ))

    def mutate_release_protocol(pack: Path, change: Callable[[dict[str, Any]], None]) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        change(graph["release_gate_protocol"])
        write_json(graph_path, graph)

    def weaken_prior_tasks(protocol: dict[str, Any]) -> None:
        protocol["preflight"]["required_tasks"] = "T39 only"
    receipts.append(run_fixture(
        "release preflight cannot weaken the prior-task prerequisite",
        lambda pack: mutate_release_protocol(pack, weaken_prior_tasks),
        1, ["Release gate protocol matches fixed semantics"],
    ))

    def omit_final_case(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["required_t40_cases"].remove("A125")
    receipts.append(run_fixture(
        "release finalization cannot omit A125",
        lambda pack: mutate_release_protocol(pack, omit_final_case),
        1, ["Release gate protocol matches fixed semantics",
            "Release gate references name T40 and its owned final cases"],
    ))

    def omit_final_review(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["required_evidence"].remove("Independent T40 review")
    receipts.append(run_fixture(
        "release finalization requires independent T40 review",
        lambda pack: mutate_release_protocol(pack, omit_final_review),
        1, ["Release gate protocol matches fixed semantics"],
    ))

    def omit_merged_checks(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["required_evidence"].remove("Merged-source checks")
    receipts.append(run_fixture(
        "release finalization requires merged-source checks",
        lambda pack: mutate_release_protocol(pack, omit_merged_checks),
        1, ["Release gate protocol matches fixed semantics"],
    ))

    def allow_preflight_approval(protocol: dict[str, Any]) -> None:
        protocol["preflight"]["may_approve_release"] = True
    receipts.append(run_fixture(
        "release preflight cannot issue approval",
        lambda pack: mutate_release_protocol(pack, allow_preflight_approval),
        1, ["Release gate protocol matches fixed semantics"],
    ))

    def weaken_p04_protocol(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        p04 = next(task for task in graph["tasks"] if task["id"] == "P04")
        p04["steps"].remove("Implement the fixed release_gate_protocol in the evidence harness.")
        write_json(graph_path, graph)
        sync_task_packet(pack, "P04")
    receipts.append(run_fixture(
        "P04 task card retains the fixed protocol implementation requirement",
        weaken_p04_protocol,
        1, ["P04, T40, A123, and A125 mirror the fixed release gate contract"],
    ))

    def weaken_t40_finalization(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        t40 = next(task for task in graph["tasks"] if task["id"] == "T40")
        t40["acceptance"].remove("A failed prerequisite writes neither a completion record nor approval.")
        write_json(graph_path, graph)
        sync_task_packet(pack, "T40")
    receipts.append(run_fixture(
        "T40 card retains fail-closed finalization conditions",
        weaken_t40_finalization,
        1, ["P04, T40, A123, and A125 mirror the fixed release gate contract"],
    ))

    def omit_actual_t40_case_results(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        t40 = next(task for task in graph["tasks"] if task["id"] == "T40")
        t40["steps"].remove(
            "Run A124 and A125 against the actual candidate. Record actual results for all three T40 cases."
        )
        write_json(graph_path, graph)
        sync_task_packet(pack, "T40")
    receipts.append(run_fixture(
        "T40 requires actual results for A123 through A125",
        omit_actual_t40_case_results,
        1, ["P04, T40, A123, and A125 mirror the fixed release gate contract"],
    ))

    def weaken_a123_refusals(pack: Path) -> None:
        path = pack / "acceptance-cases.json"
        data = read_json(path)
        case = next(row for row in data["cases"] if row["id"] == "A123")
        case["action"] = "Run the full evidence validator and release gate."
        write_json(path, data)
        sync_case_block(pack, case)
    receipts.append(run_fixture(
        "A123 keeps prerequisite refusal assertions and finalization contract",
        weaken_a123_refusals,
        1, ["P04, T40, A123, and A125 mirror the fixed release gate contract"],
    ))

    def remove_static_gate_scope(pack: Path) -> None:
        path = pack / "PACK-CHECKS.md"
        text = path.read_text(encoding="utf-8")
        sentence = ("These release-gate checks validate static planning structure only. "
                    "They do not run a release preflight, test a candidate, or issue approval.")
        if text.count(sentence) != 1:
            raise ValueError("Expected exactly one static release-gate scope sentence")
        path.write_text(text.replace(sentence + "\n", "", 1), encoding="utf-8")
    receipts.append(run_fixture(
        "pack report cannot claim runtime finalizer verification",
        remove_static_gate_scope,
        1, ["PACK-CHECKS limits release-gate claims to static structure"],
    ))

    def restore_merged_diff_cycle(pack: Path) -> None:
        path = pack / "SUBAGENTS.md"
        text = path.read_text(encoding="utf-8")
        current = "Resolve review findings before the integration merge."
        if text.count(current) != 1:
            raise ValueError("Expected one pre-merge finding-resolution instruction")
        path.write_text(text.replace(current, "Resolve review findings after the integration merge.", 1),
                        encoding="utf-8")
    receipts.append(run_fixture(
        "review packet requires candidate review before merge",
        restore_merged_diff_cycle,
        1, ["Review packet uses candidate input and post-merge completion stages"],
    ))

    def remove_p04_qualification_owner(pack: Path) -> None:
        path = pack / "SUBAGENTS.md"
        text = path.read_text(encoding="utf-8")
        current = "| Qualification subagent | P04, U08, and T35 through T38. |"
        replacement = "| Qualification subagent | U08 and T35 through T38. |"
        if text.count(current) != 1:
            raise ValueError("Expected one Qualification row with P04")
        path.write_text(text.replace(current, replacement, 1), encoding="utf-8")
    receipts.append(run_fixture(
        "P04 remains owned by the Qualification subagent",
        remove_p04_qualification_owner,
        1, ["P04 is listed under the Qualification subagent",
            "Subagent task assignments exactly match graph owners"],
    ))

    def missing_intersecting_trace_owner(pack: Path) -> None:
        trace_path = pack / "traceability.json"
        trace = read_json(trace_path)
        section = next(row for row in trace["sections"] if row["heading"] == "Portable role ABI")
        section["tasks"].remove("T14")
        write_json(trace_path, trace)
        sync_trace_table(pack)
    receipts.append(run_fixture(
        "cited task is required in every intersecting section even when mapped elsewhere",
        missing_intersecting_trace_owner,
        1, ["Task citations are represented in traceability"],
    ))

    def malformed_source_citation(pack: Path, value: str) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(task for task in graph["tasks"] if task["id"] == "T14")
        previous = task["source_basis"]
        task["source_basis"] = value
        write_json(graph_path, graph)
        for path in (pack / "PLAN.md", pack / task["task_file"]):
            document = path.read_text(encoding="utf-8")
            old_line = f"**Source basis:** {previous}."
            new_line = f"**Source basis:** {value}."
            if document.count(old_line) != 1:
                raise ValueError(f"Expected one source-basis line in {path.name}")
            path.write_text(document.replace(old_line, new_line, 1), encoding="utf-8")
    receipts.append(run_fixture(
        "malformed task SPEC citation syntax is rejected",
        lambda pack: malformed_source_citation(pack, "SPEC L113-Lbad"),
        1, ["Task SPEC citations use valid syntax"],
    ))
    receipts.append(run_fixture(
        "task SPEC citation beyond the document is rejected",
        lambda pack: malformed_source_citation(pack, "SPEC L550"),
        1, ["Task SPEC citation ranges are within SPEC.md"],
    ))

    def malformed_manifest(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        graph.pop("tasks")
        write_json(graph_path, graph)
    receipts.append(run_fixture(
        "malformed task manifest returns structured input error with exit two",
        malformed_manifest,
        2, ["Planning pack input is valid"],
        expected_report_details={"Planning pack input is valid": "'tasks'"},
    ))

    def validator_report(passed: bool, failures: list[str],
                         checks: list[dict[str, Any]] | None = None,
                         initial: bool = False) -> str:
        if checks is None:
            checks = [
                {"check": name, "passed": True, "detail": None}
                for name in canonical_check_inventory(initial)
            ]
        return json.dumps({
            "passed": passed,
            "initial": initial,
            "scope": "Static planning-pack checks only",
            "not_checked": [
                "Product compilation",
                "Runtime capabilities",
                "Independent code review",
                "Full ASD-STE100 dictionary conformity",
            ],
            "counts": derive_expected_counts(ROOT) or {},
            "checks": checks,
            "failures": failures,
        })

    def parser_fixture(name: str, stdout: str, report_errors: list[str], exit_code: int = 0,
                       expected_exit: int = 0, expected_failures: list[str] | None = None,
                       initial: bool = False, expect_report_match: bool = False) -> None:
        receipts.append(run_fixture(
            name,
            lambda _pack: None,
            expected_exit,
            expected_failures or [],
            initial=initial,
            fake_stdout=stdout,
            expect_report_match=expect_report_match,
            expected_report_errors=report_errors,
        ))

    receipts.append(run_fixture(
        "ordinary baseline report contains the complete ordered check inventory",
        lambda _pack: None, 0, [],
    ))
    receipts.append(run_fixture(
        "initial baseline report contains ordinary and initial-only check inventories",
        lambda _pack: None, 0, [], initial=True,
    ))

    parser_fixture("empty validator stdout is rejected", "", ["stdout is empty"])
    parser_fixture("invalid JSON validator stdout is rejected", "{not-json", ["stdout is not valid JSON"])
    parser_fixture("non-object validator JSON is rejected", "[]", ["report must be a JSON object"])
    parser_fixture(
        "validator report with missing fields is rejected",
        json.dumps({"passed": True}),
        ["report is missing required fields: checks, counts, failures, initial, not_checked, scope"],
    )
    wrong_types = {
        "passed": "true",
        "initial": "false",
        "scope": None,
        "not_checked": "none",
        "counts": [],
        "checks": {},
        "failures": 1,
    }
    parser_fixture(
        "validator report with wrong field types is rejected",
        json.dumps(wrong_types),
        ["report has invalid field types: passed, initial, scope, not_checked, counts, checks, failures"],
    )
    inconsistent_counts = json.loads(validator_report(True, []))
    inconsistent_counts["counts"] = {}
    parser_fixture(
        "validator report with inconsistent counts is rejected",
        json.dumps(inconsistent_counts),
        ["counts are empty outside the required-file failure report"],
    )
    contradictory_passed = json.loads(validator_report(True, []))
    contradictory_passed["checks"][0]["passed"] = False
    contradictory_passed["failures"] = [contradictory_passed["checks"][0]["check"]]
    parser_fixture("contradictory passed and failures fields are rejected",
                   json.dumps(contradictory_passed), ["passed does not match failures"])
    inconsistent_failure_list = json.loads(validator_report(True, []))
    inconsistent_failure_list["checks"][0]["passed"] = False
    parser_fixture(
        "check list inconsistent with failures is rejected even at exit zero",
        json.dumps(inconsistent_failure_list),
        ["failures do not match failed checks"],
    )
    malformed_check_report = json.loads(validator_report(True, []))
    malformed_check_report["checks"][0].pop("detail")
    parser_fixture(
        "malformed check records are rejected",
        json.dumps(malformed_check_report),
        ["checks contain malformed or duplicate records"],
    )
    duplicate_check_report = json.loads(validator_report(True, []))
    duplicate_check_report["checks"][1]["check"] = duplicate_check_report["checks"][0]["check"]
    parser_fixture(
        "duplicate check names are rejected",
        json.dumps(duplicate_check_report),
        ["checks contain malformed or duplicate records"],
    )
    wrong_scope = json.loads(validator_report(True, []))
    wrong_scope["scope"] = "Runtime tests passed"
    parser_fixture(
        "validator report with mismatched scope is rejected",
        json.dumps(wrong_scope),
        ["scope does not match the validator contract"],
    )
    exit_mismatch_report = json.loads(validator_report(True, []))
    exit_mismatch_check = exit_mismatch_report["checks"][0]["check"]
    exit_mismatch_report["checks"][0]["passed"] = False
    exit_mismatch_report["passed"] = False
    exit_mismatch_report["failures"] = [exit_mismatch_check]
    parser_fixture("report exit code must agree with passed even at exit zero",
                   json.dumps(exit_mismatch_report), ["exit code does not match passed"])
    input_error_report = json.dumps({
        "passed": False,
        "initial": False,
        "scope": "Static planning-pack checks only",
        "not_checked": [
            "Product compilation",
            "Runtime capabilities",
            "Independent code review",
            "Full ASD-STE100 dictionary conformity",
        ],
        "counts": {},
        "checks": [{"check": "Planning pack input is valid", "passed": False,
                    "detail": "'tasks'"}],
        "failures": ["Planning pack input is valid"],
    })
    parser_fixture(
        "structured input-error report with contradictory exit one is rejected",
        input_error_report,
        ["exit code does not match structured input-error report"],
        exit_code=1, expected_exit=1, expected_failures=["Planning pack input is valid"],
    )
    truncated_success = json.loads(validator_report(True, []))
    truncated_success["checks"].pop()
    parser_fixture(
        "truncated passing report is rejected",
        json.dumps(truncated_success), ["checks do not match the canonical ordered inventory"],
        expect_report_match=False,
    )

    truncated_failure = json.loads(validator_report(True, []))
    dependency_check = "Task progress waits for completed dependencies"
    failure_index = next(i for i, row in enumerate(truncated_failure["checks"])
                         if row["check"] == dependency_check)
    truncated_failure["checks"] = truncated_failure["checks"][:failure_index + 1]
    truncated_failure["checks"][-1]["passed"] = False
    truncated_failure["passed"] = False
    truncated_failure["failures"] = [dependency_check]
    parser_fixture(
        "truncated failing report is rejected even when expected failure appears",
        json.dumps(truncated_failure), ["checks do not match the canonical ordered inventory"],
        exit_code=1, expected_exit=1, expected_failures=[dependency_check],
    )

    missing_middle = json.loads(validator_report(True, []))
    missing_middle["checks"].pop(18)
    parser_fixture(
        "report with a missing middle check is rejected",
        json.dumps(missing_middle), ["checks do not match the canonical ordered inventory"],
        expect_report_match=False,
    )
    reordered_checks = json.loads(validator_report(True, []))
    reordered_checks["checks"][6], reordered_checks["checks"][7] = (
        reordered_checks["checks"][7], reordered_checks["checks"][6],
    )
    parser_fixture(
        "report with reordered checks is rejected",
        json.dumps(reordered_checks), ["checks do not match the canonical ordered inventory"],
        expect_report_match=False,
    )
    extra_check = json.loads(validator_report(True, []))
    extra_check["checks"].append({"check": "Unexpected extra check", "passed": True, "detail": None})
    parser_fixture(
        "report with an extra check is rejected",
        json.dumps(extra_check), ["checks do not match the canonical ordered inventory"],
        expect_report_match=False,
    )
    wrong_initial_tail = json.loads(validator_report(True, [], initial=True))
    wrong_initial_tail["checks"].pop(-2)
    parser_fixture(
        "initial report missing an initial-only tail check is rejected",
        json.dumps(wrong_initial_tail), ["checks do not match the canonical ordered inventory"],
        initial=True, expect_report_match=False,
    )
    initial_flag_mismatch = json.loads(validator_report(True, [], initial=True))
    initial_flag_mismatch["initial"] = False
    parser_fixture(
        "report initial flag must match the validator invocation",
        json.dumps(initial_flag_mismatch), ["initial flag does not match invocation"],
        initial=True, expect_report_match=False,
    )
    extra_failure_report = json.loads(validator_report(True, []))
    first_failure = extra_failure_report["checks"][0]["check"]
    second_failure = extra_failure_report["checks"][1]["check"]
    extra_failure_report["checks"][0]["passed"] = False
    extra_failure_report["checks"][1]["passed"] = False
    extra_failure_report["passed"] = False
    extra_failure_report["failures"] = [first_failure, second_failure]
    parser_fixture(
        "unexpected extra failure name does not match exact expectation",
        json.dumps(extra_failure_report), [],
        exit_code=1, expected_exit=1, expected_failures=[first_failure],
    )

    def omit_integration_owned_tasks_from_assignment_table(pack: Path) -> None:
        path = pack / "SUBAGENTS.md"
        text = path.read_text(encoding="utf-8")
        current = "| Integration subagent | P00, P05, and T39; root Cargo files, lockfile, shared schemas, CI registration, and merges. |"
        replacement = "| Integration subagent | P00 and T39; root Cargo files, lockfile, shared schemas, CI registration, and merges. |"
        if text.count(current) != 1:
            raise ValueError("Expected one complete Integration task row")
        path.write_text(text.replace(current, replacement, 1), encoding="utf-8")
    receipts.append(run_fixture(
        "Integration role row lists every graph-owned task",
        omit_integration_owned_tasks_from_assignment_table,
        1, ["Subagent task assignments exactly match graph owners"],
    ))

    def change_graph_owner_without_assignment_table(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        next(task for task in graph["tasks"] if task["id"] == "P00")["owner"] = "Planning review subagent"
        write_json(graph_path, graph)
        sync_task_packet(pack, "P00")
        index_row = "| P00 | Freeze the work area and source evidence | Integration subagent | None |"
        changed_row = "| P00 | Freeze the work area and source evidence | Planning review subagent | None |"
        for name in ("PLAN.md", "TASKS.md"):
            path = pack / name
            index = path.read_text(encoding="utf-8")
            if index.count(index_row) != 1:
                raise ValueError(f"Expected one P00 row in {name}")
            path.write_text(index.replace(index_row, changed_row, 1), encoding="utf-8")
    receipts.append(run_fixture(
        "role assignment table matches every graph owner, not only Integration",
        change_graph_owner_without_assignment_table,
        1, ["Subagent task assignments exactly match graph owners"],
    ))

    def route_t40_writes_directly(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        t40 = next(task for task in graph["tasks"] if task["id"] == "T40")
        t40["write_scope"][2] = "Write T40 status, final acceptance summary, completion, receipt, and approval directly"
        t40["steps"][-1] = "Write T40 PASS, the final acceptance summary, completion, finalization receipt, and approval together."
        write_json(graph_path, graph)
        sync_task_packet(pack, "T40")
    receipts.append(run_fixture(
        "T40 cannot bypass Integration for shared finalization writes",
        route_t40_writes_directly,
        1, ["P04, T40, A123, and A125 mirror the fixed release gate contract",
            "T40 finalization writes route through Integration"],
    ))

    def assign_finalization_writer_to_t40(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["finalization_writer"] = "Independent review subagent"
    receipts.append(run_fixture(
        "release finalization keeps shared writes with Integration",
        lambda pack: mutate_release_protocol(pack, assign_finalization_writer_to_t40),
        1, ["Release gate protocol matches fixed semantics"],
    ))

    def omit_conditional_patch_approval(protocol: dict[str, Any]) -> None:
        protocol["finalization"].pop("conditional_patch_approval")
    receipts.append(run_fixture(
        "release finalization requires conditional approval for supported patched tuples",
        lambda pack: mutate_release_protocol(pack, omit_conditional_patch_approval),
        1, ["Release gate protocol matches fixed semantics"],
    ))

    def require_approval_for_unpatched_tuples(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["conditional_patch_approval"]["unpatched_tuples_require_record"] = True
    receipts.append(run_fixture(
        "unpatched supported tuples do not require patch-approval records",
        lambda pack: mutate_release_protocol(pack, require_approval_for_unpatched_tuples),
        1, ["Release gate protocol matches fixed semantics"],
    ))

    def weaken_a125_patch_approval(pack: Path) -> None:
        path = pack / "acceptance-cases.json"
        data = read_json(path)
        case = next(row for row in data["cases"] if row["id"] == "A125")
        case["expected"] = "Artifacts match the approved source. Patched tuples are explicit. No unauthorized publication occurs."
        write_json(path, data)
        sync_case_block(pack, case)
    receipts.append(run_fixture(
        "A125 verifies conditional maintainer approval for each patched tuple",
        weaken_a125_patch_approval,
        1, ["A125 verifies conditional patched-tuple approval"],
    ))

    def remove_c01_source_row(pack: Path) -> None:
        path = pack / "CONTRACTS.md"
        document = path.read_text(encoding="utf-8")
        row = "| Historical experiment source | `a7689b48e2b6ef2ca5a4367ee27527887cf45ce8` |\n"
        if document.count(row) != 1:
            raise ValueError("Expected one historical C01 source row")
        path.write_text(document.replace(row, "", 1), encoding="utf-8")
    receipts.append(run_fixture(
        "C01 source table rejects a missing historical reference",
        remove_c01_source_row,
        1, ["C01 source pin table matches all four fixed references"],
    ))

    def duplicate_c01_source_row(pack: Path) -> None:
        path = pack / "CONTRACTS.md"
        document = path.read_text(encoding="utf-8")
        row = "| Jackin source baseline | `faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e` |"
        if document.count(row) != 1:
            raise ValueError("Expected one Jackin source row")
        path.write_text(document.replace(row, row + "\n" + row, 1), encoding="utf-8")
    receipts.append(run_fixture(
        "C01 source table rejects a duplicate reference",
        duplicate_c01_source_row,
        1, ["C01 source pin table matches all four fixed references"],
    ))

    def add_c01_source_row(pack: Path) -> None:
        path = pack / "CONTRACTS.md"
        document = path.read_text(encoding="utf-8")
        marker = "| --- | --- |\n"
        start = document.index("## C01. Source and compatibility pins")
        end = document.index("## C02. Production crate graph", start)
        section = document[start:end]
        if section.count(marker) != 1:
            raise ValueError("Expected one C01 source table separator")
        changed = section.replace(
            marker, marker + "| Unexpected source | `deadbeef` |\n", 1,
        )
        path.write_text(document[:start] + changed + document[end:], encoding="utf-8")
    receipts.append(run_fixture(
        "C01 source table rejects an extra reference",
        add_c01_source_row,
        1, ["C01 source pin table matches all four fixed references"],
    ))

    def alter_c01_comparator_version(pack: Path) -> None:
        path = pack / "CONTRACTS.md"
        document = path.read_text(encoding="utf-8")
        old = "| OpenShell release comparator | `v0.1.5`,"
        if document.count(old) != 1:
            raise ValueError("Expected one release comparator version")
        path.write_text(document.replace(old, "| OpenShell release comparator | `v0.1.6`,", 1),
                        encoding="utf-8")
    receipts.append(run_fixture(
        "C01 source table rejects an altered release version",
        alter_c01_comparator_version,
        1, ["C01 source pin table matches all four fixed references"],
    ))

    def alter_c01_comparator_sha(pack: Path) -> None:
        path = pack / "CONTRACTS.md"
        document = path.read_text(encoding="utf-8")
        old = "bdb956f2cd65a635f1294fccf1d138d263bf650f"
        if document.count(old) != 1:
            raise ValueError("Expected one release comparator commit")
        path.write_text(document.replace(old, "bdb956f2cd65a635f1294fccf1d138d263bf650e", 1),
                        encoding="utf-8")
    receipts.append(run_fixture(
        "C01 source table rejects an altered release comparator commit",
        alter_c01_comparator_sha,
        1, ["C01 source pin table matches all four fixed references"],
    ))

    def alter_c01_historical_sha(pack: Path) -> None:
        path = pack / "CONTRACTS.md"
        document = path.read_text(encoding="utf-8")
        old = "a7689b48e2b6ef2ca5a4367ee27527887cf45ce8"
        if document.count(old) != 1:
            raise ValueError("Expected one historical source commit")
        path.write_text(document.replace(old, "a7689b48e2b6ef2ca5a4367ee27527887cf45ce9", 1),
                        encoding="utf-8")
    receipts.append(run_fixture(
        "C01 source table rejects an altered historical commit",
        alter_c01_historical_sha,
        1, ["C01 source pin table matches all four fixed references"],
    ))

    def mutate_c02_bytes(pack: Path, old: bytes, new: bytes, scenario: str) -> None:
        path = pack / "CONTRACTS.md"
        document = path.read_bytes()
        if document.count(old) != 1:
            raise ValueError(f"Expected one C02 input for {scenario}")
        path.write_bytes(document.replace(old, new, 1))

    def remove_c02_row(pack: Path) -> None:
        mutate_c02_bytes(
            pack,
            b"| `crates/core/jackin-domain` | Typed records, IDs, errors, receipts, and application ports. |\n",
            b"",
            "deleted row",
        )
    receipts.append(run_fixture(
        "C02 digest rejects a deleted production crate row",
        remove_c02_row,
        1, ["C02 production crate graph and boundary section matches fixed bytes"],
    ))

    def alter_c02_row(pack: Path) -> None:
        mutate_c02_bytes(
            pack,
            b"| `crates/adapters/jackin-openshell` | The only sandbox execution adapter. |\n",
            b"| `crates/adapters/jackin-openshell` | One of several sandbox execution adapters. |\n",
            "altered row",
        )
    receipts.append(run_fixture(
        "C02 digest rejects an altered crate responsibility",
        alter_c02_row,
        1, ["C02 production crate graph and boundary section matches fixed bytes"],
    ))

    def weaken_c02_boundary(pack: Path) -> None:
        mutate_c02_bytes(
            pack,
            b"Service crates depend on domain ports, not concrete adapters.\n",
            b"Service crates can depend on concrete adapters directly.\n",
            "weakened boundary",
        )
    receipts.append(run_fixture(
        "C02 digest rejects a weakened dependency boundary",
        weaken_c02_boundary,
        1, ["C02 production crate graph and boundary section matches fixed bytes"],
    ))

    def duplicate_c02_heading(pack: Path) -> None:
        mutate_c02_bytes(
            pack, b"## C02. Production crate graph\n",
            b"## C02. Production crate graph\n## C02. Production crate graph\n",
            "duplicate C02 heading",
        )
    receipts.append(run_fixture(
        "C02 digest rejects a duplicate section anchor",
        duplicate_c02_heading,
        1, ["C01 source pin table matches all four fixed references",
            "C02 production crate graph and boundary section matches fixed bytes"],
    ))

    def remove_c02_heading(pack: Path) -> None:
        mutate_c02_bytes(
            pack, b"## C02. Production crate graph\n", b"", "missing C02 heading",
        )
    receipts.append(run_fixture(
        "C02 digest rejects a missing section anchor",
        remove_c02_heading,
        1, ["C01 source pin table matches all four fixed references",
            "C02 production crate graph and boundary section matches fixed bytes"],
    ))

    def remove_c03_boundary(pack: Path) -> None:
        mutate_c02_bytes(
            pack, b"## C03. Configuration and identifiers\n", b"", "missing C03 boundary",
        )
    receipts.append(run_fixture(
        "C02 digest rejects a missing end boundary",
        remove_c03_boundary,
        1, ["C02 production crate graph and boundary section matches fixed bytes"],
    ))

    def duplicate_c03_boundary(pack: Path) -> None:
        mutate_c02_bytes(
            pack,
            b"## C03. Configuration and identifiers\n",
            b"## C03. Configuration and identifiers\n## C03. Configuration and identifiers\n",
            "duplicate C03 boundary",
        )
    receipts.append(run_fixture(
        "C02 digest rejects a duplicate end boundary",
        duplicate_c03_boundary,
        1, ["C02 production crate graph and boundary section matches fixed bytes"],
    ))

    def edit_task_packet(pack: Path, update: Callable[[str], str]) -> None:
        path = pack / "tasks/P00.md"
        document = path.read_text(encoding="utf-8")
        path.write_text(update(document), encoding="utf-8")

    def remove_task_packet_preamble(pack: Path) -> None:
        edit_task_packet(pack, lambda document: document[document.index("### P00 —"):])
    receipts.append(run_fixture(
        "individual task packet rejects a missing instruction preamble",
        remove_task_packet_preamble,
        1, ["Task graph fields and canonical preamble match individual task packets"],
    ))

    def remove_p00_task_packet(pack: Path) -> None:
        (pack / "tasks/P00.md").unlink()
    receipts.append(run_fixture(
        "missing task packet input identifies P00",
        remove_p00_task_packet,
        1, ["Task graph fields and canonical preamble match individual task packets"],
        expected_report_details={
            "Task graph fields and canonical preamble match individual task packets": ["P00"],
        },
    ))

    def change_task_packet_id(pack: Path) -> None:
        def update(document: str) -> str:
            old = "# Task packet: P00"
            if document.count(old) != 1:
                raise ValueError("Expected one packet ID")
            return document.replace(old, "# Task packet: P01", 1)
        edit_task_packet(pack, update)
    receipts.append(run_fixture(
        "individual task packet rejects a mismatched packet ID",
        change_task_packet_id,
        1, ["Task graph fields and canonical preamble match individual task packets"],
    ))

    def weaken_packet_lease(pack: Path) -> None:
        def update(document: str) -> str:
            old = "Use an exclusive file lease from the coordinator."
            if document.count(old) != 1:
                raise ValueError("Expected one exclusive lease instruction")
            return document.replace(old, "Use a shared file lease from the coordinator.", 1)
        edit_task_packet(pack, update)
    receipts.append(run_fixture(
        "individual task packet rejects a weakened file lease instruction",
        weaken_packet_lease,
        1, ["Task graph fields and canonical preamble match individual task packets"],
    ))

    def weaken_packet_dependency_instruction(pack: Path) -> None:
        def update(document: str) -> str:
            old = "Check dependencies in [../task-graph.json](../task-graph.json) before starting work."
            if document.count(old) != 1:
                raise ValueError("Expected one dependency instruction")
            return document.replace(old, "Review task dependencies when convenient.", 1)
        edit_task_packet(pack, update)
    receipts.append(run_fixture(
        "individual task packet rejects a weakened dependency instruction",
        weaken_packet_dependency_instruction,
        1, ["Task graph fields and canonical preamble match individual task packets"],
    ))

    def weaken_packet_authorization(pack: Path) -> None:
        def update(document: str) -> str:
            old = "The task card does not authorize unrelated changes or production actions."
            if document.count(old) != 1:
                raise ValueError("Expected one task authorization instruction")
            return document.replace(
                old, "The task card authorizes unrelated changes and production actions.", 1,
            )
        edit_task_packet(pack, update)
    receipts.append(run_fixture(
        "individual task packet rejects weakened change authorization",
        weaken_packet_authorization,
        1, ["Task graph fields and canonical preamble match individual task packets"],
    ))

    def add_conflicting_packet_instruction(pack: Path) -> None:
        def update(document: str) -> str:
            marker = "\n### P00 —"
            if document.count(marker) != 1:
                raise ValueError("Expected one P00 card heading")
            return document.replace(
                marker, "\nIgnore the file lease and edit unrelated production files.\n### P00 —", 1,
            )
        edit_task_packet(pack, update)
    receipts.append(run_fixture(
        "individual task packet rejects an extra conflicting instruction",
        add_conflicting_packet_instruction,
        1, ["Task graph fields and canonical preamble match individual task packets"],
    ))

    def set_task_field(pack: Path, task_id: str, field: str, value: Any,
                       sync_packet: bool = False) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == task_id)
        task[field] = value
        write_json(graph_path, graph)
        if sync_packet:
            sync_task_packet(pack, task_id)

    path_contract_check = "Task file and test write scopes match canonical paths"
    for scenario, task_file in (
        ("relocated", "tasks/archive/P00.md"),
        ("traversing", "tasks/../PLAN.md"),
        ("absolute", "/tmp/P00.md"),
        ("colliding", "tasks/P01.md"),
    ):
        receipts.append(run_fixture(
            f"task file path rejects {scenario} locations",
            lambda pack, value=task_file: set_task_field(pack, "P00", "task_file", value),
            1, [path_contract_check],
        ))

    for scenario, test_scope in (
        ("relocated", "tests/v2/archive/P00/"),
        ("traversing", "tests/v2/tasks/../P01/"),
        ("absolute", "/tmp/tests/P00/"),
        ("colliding", "tests/v2/tasks/P01/"),
    ):
        receipts.append(run_fixture(
            f"task test-write scope rejects {scenario} locations",
            lambda pack, value=test_scope: set_task_field(
                pack, "P00", "test_write_scope", value, sync_packet=True,
            ),
            1, [path_contract_check],
        ))

    def empty_root_dependencies(pack: Path) -> None:
        set_task_field(pack, "P00", "depends_on", [])
    receipts.append(run_fixture(
        "empty root dependency list remains valid",
        empty_root_dependencies,
        0, [],
    ))

    dependency_contract_check = "Dependency lists contain unique string task IDs"
    dependency_mirror_failures = [
        "Task graph fields match TASKS index table",
        "Task graph fields match PLAN task index table",
        "Task graph fields match PLAN task cards",
        "Task graph fields and canonical preamble match individual task packets",
    ]
    for scenario, dependencies in (
        ("duplicate IDs", ["P00", "P00"]),
        ("non-string entries", ["P00", {"task": "P00"}]),
        ("a non-list value", "P00"),
    ):
        receipts.append(run_fixture(
            f"dependency list rejects {scenario} before DAG operations",
            lambda pack, value=dependencies: set_task_field(pack, "P01", "depends_on", value),
            1, [dependency_contract_check, *dependency_mirror_failures],
        ))

    dependency_graph_failures = [
        "Dependencies reference other known tasks",
        "Dependency graph has no cycle",
        "Stored ready groups match dependencies without duplicate task IDs",
        *dependency_mirror_failures,
    ]
    receipts.append(run_fixture(
        "unknown dependency retains reference and readiness diagnostics",
        lambda pack: set_task_field(pack, "P01", "depends_on", ["P99"]),
        1, dependency_graph_failures,
    ))
    receipts.append(run_fixture(
        "self dependency retains reference and cycle diagnostics",
        lambda pack: set_task_field(pack, "P01", "depends_on", ["P01"]),
        1, dependency_graph_failures,
    ))

    def add_dependency_cycle(pack: Path) -> None:
        set_task_field(pack, "P00", "depends_on", ["P01"])
        set_task_field(pack, "P01", "depends_on", ["P00"])
    cycle_failures = [
        "Dependency graph has no cycle",
        "Stored ready groups match dependencies without duplicate task IDs",
        *dependency_mirror_failures,
    ]
    receipts.append(run_fixture(
        "dependency cycle retains cycle and readiness diagnostics",
        add_dependency_cycle,
        1, cycle_failures,
    ))

    def change_case_level(pack: Path, case_id: str, level: str) -> None:
        path = pack / "acceptance-cases.json"
        data = read_json(path)
        case = next(row for row in data["cases"] if row["id"] == case_id)
        case["level"] = level
        write_json(path, data)
        sync_case_block(pack, case)
    level_contract_check = "Acceptance case IDs map to fixed evidence levels"
    receipts.append(run_fixture(
        "valid evidence-level downgrade does not reclassify A11",
        lambda pack: change_case_level(pack, "A11", "BUILD"),
        1, [level_contract_check],
    ))
    receipts.append(run_fixture(
        "valid evidence-level downgrade does not reclassify A50",
        lambda pack: change_case_level(pack, "A50", "UNIT"),
        1, [level_contract_check],
    ))

    goal_contract_check = "GOAL.md matches its fixed checksum"
    def truncate_goal(pack: Path) -> None:
        path = pack / "GOAL.md"
        path.write_bytes(path.read_bytes()[:-1])
    receipts.append(run_fixture(
        "GOAL.md rejects truncation",
        truncate_goal,
        1, [goal_contract_check],
    ))

    def delete_goal(pack: Path) -> None:
        (pack / "GOAL.md").unlink()
    receipts.append(run_fixture(
        "GOAL.md is a required input",
        delete_goal,
        1, ["Required pack files exist"],
        expected_report_details={"Required pack files exist": ["GOAL.md"]},
    ))

    def add_conflicting_goal_clause(pack: Path) -> None:
        path = pack / "GOAL.md"
        with path.open("a", encoding="utf-8") as goal:
            goal.write("\nUse Docker as an additional agent runtime.\n")
    receipts.append(run_fixture(
        "GOAL.md rejects a conflicting execution-control clause",
        add_conflicting_goal_clause,
        1, [goal_contract_check],
    ))

    concurrency_contract_check = "Authoritative concurrency declaration matches graph and subagent limit"
    def edit_subagents(pack: Path, update: Callable[[str], str]) -> None:
        path = pack / "SUBAGENTS.md"
        document = path.read_text(encoding="utf-8")
        path.write_text(update(document), encoding="utf-8")
    def change_concurrency_number(pack: Path) -> None:
        def update(document: str) -> str:
            old = "Use up to eight concurrent subagents."
            if document.count(old) != 1:
                raise ValueError("Expected one authoritative concurrency declaration")
            return document.replace(old, "Use up to seven concurrent subagents.", 1)
        edit_subagents(pack, update)
    receipts.append(run_fixture(
        "authoritative concurrency declaration rejects a changed limit",
        change_concurrency_number,
        1, [concurrency_contract_check],
    ))

    def remove_concurrency_declaration(pack: Path) -> None:
        def update(document: str) -> str:
            old = "Use up to eight concurrent subagents. "
            if document.count(old) != 1:
                raise ValueError("Expected one authoritative concurrency declaration")
            return document.replace(old, "", 1)
        edit_subagents(pack, update)
    receipts.append(run_fixture(
        "authoritative concurrency declaration cannot be missing",
        remove_concurrency_declaration,
        1, [concurrency_contract_check],
    ))

    def duplicate_concurrency_declaration(pack: Path) -> None:
        def update(document: str) -> str:
            old = "Use up to eight concurrent subagents."
            if document.count(old) != 1:
                raise ValueError("Expected one authoritative concurrency declaration")
            return document.replace(old, old + "\nUse up to eight concurrent subagents.", 1)
        edit_subagents(pack, update)
    receipts.append(run_fixture(
        "authoritative concurrency declaration cannot be duplicated",
        duplicate_concurrency_declaration,
        1, [concurrency_contract_check],
    ))

    def add_duplicate_json_member(pack: Path, relative_path: str, member: str) -> None:
        path = pack / relative_path
        document = path.read_text(encoding="utf-8")
        if member not in document:
            raise ValueError(f"Expected JSON member in {relative_path}")
        path.write_text(document.replace(member, member + "\n    " + member, 1), encoding="utf-8")

    for relative_path, top_member, nested_member, manifest_name in (
        ("task-graph.json", '"schema_version": 1,', '"id": "P00",', "task graph"),
        ("acceptance-cases.json", '"schema_version": 1,', '"id": "A01",', "acceptance"),
        ("traceability.json", '"schema_version": 1,',
         '"heading": "Decision and product scope",', "traceability"),
    ):
        receipts.append(run_fixture(
            f"{manifest_name} rejects a duplicate top-level JSON key",
            lambda pack, path=relative_path, key=top_member:
                add_duplicate_json_member(pack, path, key),
            2, ["Planning pack input is valid"],
        ))
        receipts.append(run_fixture(
            f"{manifest_name} rejects a duplicate nested JSON key",
            lambda pack, path=relative_path, key=nested_member:
                add_duplicate_json_member(pack, path, key),
            2, ["Planning pack input is valid"],
        ))

    receipts.append(run_fixture(
        "fixture report parser rejects duplicate top-level JSON keys",
        lambda _pack: None,
        0, [],
        validator_executable=None,
        fake_stdout='{"passed": true, "passed": true}',
        expect_report_match=False,
        expected_report_errors=["duplicate JSON object key: passed"],
    ))
    receipts.append(run_fixture(
        "fixture report parser rejects duplicate nested JSON keys",
        lambda _pack: None,
        0, [],
        fake_stdout='{"counts": {"tasks": 46, "tasks": 46}}',
        expect_report_match=False,
        expected_report_errors=["duplicate JSON object key: tasks"],
    ))

    a13_owner_check = "A13 preserves separate current-graph and final-post-deletion runs"
    a15_owner_check = "A15 separates observation, adapter, and integrated activation owners"
    a16_owner_check = "A16 preserves accepted-revision observation and product-presentation owners"

    def weaken_p05_a13_current_run(pack: Path) -> None:
        graph = read_json(pack / "task-graph.json")
        task = next(row for row in graph["tasks"] if row["id"] == "P05")
        task["steps"] = [step for step in task["steps"] if not step.startswith("Run A13 against the newly created current graph")]
        task["steps"].append("Run A13 only after legacy deletion against the final graph.")
        write_json(pack / "task-graph.json", graph)
        sync_task_packet(pack, "P05")
    receipts.append(run_fixture(
        "P05 cannot defer its A13 run to the final graph",
        weaken_p05_a13_current_run,
        1, [a13_owner_check],
    ))

    def remove_t39_a13_final_rerun(pack: Path) -> None:
        graph = read_json(pack / "task-graph.json")
        task = next(row for row in graph["tasks"] if row["id"] == "T39")
        task["steps"] = [step for step in task["steps"]
                         if not step.startswith(("Rerun A13 and A14", "Record this result separately"))]
        write_json(pack / "task-graph.json", graph)
        sync_task_packet(pack, "T39")
    receipts.append(run_fixture(
        "T39 cannot omit its final post-deletion A13 rerun",
        remove_t39_a13_final_rerun,
        1, [a13_owner_check],
    ))

    def remove_case_owner(pack: Path, case_id: str, task_id: str) -> None:
        data_path = pack / "acceptance-cases.json"
        data = read_json(data_path)
        case = next(row for row in data["cases"] if row["id"] == case_id)
        case["tasks"].remove(task_id)
        case["assignment_results"] = [row for row in case["assignment_results"]
                                        if row["task_id"] != task_id]
        write_json(data_path, data)
        sync_case_block(pack, case)

    receipts.append(run_fixture(
        "A15 cannot omit the integrated T23 owner",
        lambda pack: remove_case_owner(pack, "A15", "T23"),
        1, [a15_owner_check, "Assignment results have exact unique case owners and valid statuses",
            "Case and task ownership maps agree"],
    ))

    def weaken_t23_a15_activation(pack: Path) -> None:
        graph = read_json(pack / "task-graph.json")
        task = next(row for row in graph["tasks"] if row["id"] == "T23")
        task["steps"] = [
            "Exercise activation through the complete recorded operation sequence."
            if step.startswith("Exercise A15 through integrated StartSession") else step
            for step in task["steps"]
        ]
        write_json(pack / "task-graph.json", graph)
        sync_task_packet(pack, "T23")
    receipts.append(run_fixture(
        "T23 must qualify the full A15 StartSession barrier",
        weaken_t23_a15_activation,
        1, [a15_owner_check],
    ))

    def weaken_layer_contract(pack: Path, task_id: str, case_id: str) -> None:
        graph = read_json(pack / "task-graph.json")
        task = next(row for row in graph["tasks"] if row["id"] == task_id)
        task["acceptance"] = [
            f"{case_id} is covered by the upstream contract."
            if text.startswith(f"{case_id} and A") else text
            for text in task["acceptance"]
        ]
        write_json(pack / "task-graph.json", graph)
        sync_task_packet(pack, task_id)
    receipts.append(run_fixture(
        "U00 cannot collapse A15 and A16 into integrated behavior",
        lambda pack: weaken_layer_contract(pack, "U00", "A15"),
        1, [a15_owner_check, a16_owner_check],
    ))
    receipts.append(run_fixture(
        "T13 cannot collapse A15 and A16 into product behavior",
        lambda pack: weaken_layer_contract(pack, "T13", "A15"),
        1, [a15_owner_check, a16_owner_check],
    ))

    def weaken_a16_layer_source(pack: Path, task_id: str, original: str) -> None:
        graph = read_json(pack / "task-graph.json")
        task = next(row for row in graph["tasks"] if row["id"] == task_id)
        if original not in task["acceptance"]:
            raise ValueError(f"Expected A16 layer assertion for {task_id}")
        task["acceptance"] = [
            "A16 checks policy data."
            if text == original else text
            for text in task["acceptance"]
        ]
        write_json(pack / "task-graph.json", graph)
        sync_task_packet(pack, task_id)

    receipts.append(run_fixture(
        "U00 must bind A16's replacement source to its accepted revision",
        lambda pack: weaken_a16_layer_source(
            pack, "U00",
            "A16 at U00 verifies the accepted revision and replacement source without claiming a policy intersection.",
        ),
        1, [a16_owner_check],
    ))
    receipts.append(run_fixture(
        "T13 must bind A16's replacement source to its accepted revision",
        lambda pack: weaken_a16_layer_source(
            pack, "T13",
            "A16 at T13 binds the effective replacement source to the observed accepted revision without claiming an intersection.",
        ),
        1, [a16_owner_check],
    ))

    receipts.append(run_fixture(
        "A16 cannot omit the T29 product-presentation owner",
        lambda pack: remove_case_owner(pack, "A16", "T29"),
        1, [a16_owner_check, "Assignment results have exact unique case owners and valid statuses",
            "Case and task ownership maps agree"],
    ))

    def weaken_t29_a16_presentation(pack: Path) -> None:
        graph = read_json(pack / "task-graph.json")
        task = next(row for row in graph["tasks"] if row["id"] == "T29")
        task["steps"] = [
            "Display the current policy details."
            if step.startswith("Display A16's accepted policy revision") else step
            for step in task["steps"]
        ]
        write_json(pack / "task-graph.json", graph)
        sync_task_packet(pack, "T29")
    receipts.append(run_fixture(
        "T29 must display A16's accepted replacement source and revision",
        weaken_t29_a16_presentation,
        1, [a16_owner_check],
    ))

    def validator_mutates_inputs_during_execution(pack: Path) -> None:
        script = """from pathlib import Path
import sys
root = Path(sys.argv[sys.argv.index('--root') + 1])
tasks = root / 'TASKS.md'
tasks.write_text(tasks.read_text(encoding='utf-8') + '\\nUnexpected mutation.\\n', encoding='utf-8')
self_path = Path(__file__)
self_path.write_text(self_path.read_text(encoding='utf-8') + '\\n# changed during execution\\n', encoding='utf-8')
print('{}')
"""
        (pack / "validate_pack.py").write_text(script, encoding="utf-8")
    receipts.append(run_fixture(
        "fixture runner detects input and validator changes during execution",
        validator_mutates_inputs_during_execution,
        0, [],
        expect_report_match=False,
        expected_report_errors=[
            "report is missing required fields: checks, counts, failures, initial, not_checked, passed, scope",
        ],
        expected_execution_errors=[
            "pack input changed during validator execution: TASKS.md",
            "pack input changed during validator execution: validate_pack.py",
            "validator executable changed during execution",
        ],
    ))

    original_board = validate_original_board()
    original_source_after = snapshot_files(ROOT)
    runner_source_changes = snapshot_changes(original_source_before, original_source_after)
    original_board["fixture_runner_source_changes"] = runner_source_changes
    if runner_source_changes:
        original_board["execution_errors"].append("fixture runner changed the original pack")
        original_board["passed"] = False

    payload = {
        "passed": original_board["passed"] and all(receipt["passed"] for receipt in receipts),
        "scope": "Static validator fixtures only. No product or runtime tests are performed.",
        "original_board": original_board,
        "source_sha256": {
            "validate_pack.py": sha256(VALIDATOR),
            "validate_assignment_fixtures.py": sha256(Path(__file__)),
            "pack_files": snapshot_files(ROOT),
        },
        "fixtures": receipts,
    }
    print(json.dumps(payload, indent=2, ensure_ascii=False))
    return 0 if payload["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
