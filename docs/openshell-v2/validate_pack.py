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
GOAL_SHA256 = "d62aaf0477b96b014356b09d23f05bc2d22385a42277639514e6615dfa46f1a0"
ACCEPTANCE_POLICY_SHA256 = "503b2bd2ed5560a311a3641409c0f59a7806f6847575b509641b85cbab3ed59a"
C02_CONTRACT_SECTION_SHA256 = "017714a83ab1ee7827dfcf8459ef6ca5e58c11787210083856211d1ba8e9cdf2"
SUPPORTED_SCHEMA_VERSION = 1
PARALLEL_AGENT_LIMIT = 8
ACCEPTANCE_EVIDENCE_LEVELS = {"SOURCE", "UNIT", "BUILD", "LIVE"}
EXPECTED_CASE_LEVEL_IDS = {
    "SOURCE": "A01 A02 A03 A04 A13 A14 A120 A121 A123 A124",
    "UNIT": "A07 A08 A09 A10 A12 A40 A42 A47 A49 A59 A60 A61 A62 A63 A64 A70 A84 A96",
    "BUILD": "A50 A51 A52 A53 A54 A122 A125",
    "LIVE": (
        "A05 A06 A11 A15 A16 A17 A18 A19 A20 A21 A22 A23 A24 A25 A26 A27 A28 A29 A30 A31 "
        "A32 A33 A34 A35 A36 A37 A38 A39 A41 A43 A44 A45 A46 A48 A55 A56 A57 A58 A65 A66 "
        "A67 A68 A69 A71 A72 A73 A74 A75 A76 A77 A78 A79 A80 A81 A82 A83 A85 A86 A87 A88 "
        "A89 A90 A91 A92 A93 A94 A95 A97 A98 A99 A100 A101 A102 A103 A104 A105 A106 A107 "
        "A108 A109 A110 A111 A112 A113 A114 A115 A116 A117 A118 A119"
    ),
}
EXPECTED_CASE_LEVELS = {
    case_id: level
    for level, ids in EXPECTED_CASE_LEVEL_IDS.items()
    for case_id in ids.split()
}
EXPECTED_CASE_LEVEL_COUNTS = {"SOURCE": 10, "UNIT": 18, "BUILD": 7, "LIVE": 90}
REVIEW_CAPACITY_SENTENCE = "Keep capacity for an independent review subagent."
CONCURRENCY_DECLARATION = re.compile(
    r"\bUse up to (\d+|zero|one|two|three|four|five|six|seven|eight) concurrent subagents\."
)
CONCURRENCY_WORD_VALUES = {
    "zero": 0, "one": 1, "two": 2, "three": 3, "four": 4,
    "five": 5, "six": 6, "seven": 7, "eight": 8,
}
CANONICAL_TASK_ID = re.compile(r"[PUT]\d{2}")
TASK_REVIEW_RULE = (
    "A different subagent reviews the result. "
    "The integration subagent reruns affected tests after merge."
)
PROGRESS_TASK_STATUSES = {"READY", "RUNNING", "REVIEW", "PASS", "FAIL"}
CASE_STATUSES = {"NOT_RUN", "PASS", "FAIL", "BLOCKED"}
TASK_COMPLETION_RULE = (
    "All task checks pass at their declared evidence level. "
    "Required artifacts and an independent review exist."
)
SOURCE_PINS = {
    "jackin_baseline": "faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e",
    "openshell_candidate": "c4688aa8715ea75d6c39fe468361dbbc16c691fa",
    "openshell_release_comparator": "bdb956f2cd65a635f1294fccf1d138d263bf650f",
}
SOURCE_PIN_TABLE_ROWS = [
    "| Item | Reference |",
    "| --- | --- |",
    "| Jackin source baseline | `faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e` |",
    "| OpenShell candidate source | `c4688aa8715ea75d6c39fe468361dbbc16c691fa` |",
    "| OpenShell release comparator | `v0.1.5`, `bdb956f2cd65a635f1294fccf1d138d263bf650f` |",
    "| Historical experiment source | `a7689b48e2b6ef2ca5a4367ee27527887cf45ce8` |",
]
TASK_PACKET_INSTRUCTIONS = [
    "## Instructions",
    "",
    "Read [../GOAL.md](../GOAL.md), [../CONTRACTS.md](../CONTRACTS.md), and [../SUBAGENTS.md](../SUBAGENTS.md) before this assignment.",
    "Read every required input in this card.",
    "Check dependencies in [../task-graph.json](../task-graph.json) before starting work.",
    "Read the assigned cases in [../ACCEPTANCE.md](../ACCEPTANCE.md).",
    "Use an exclusive file lease from the coordinator.",
    "The task card does not authorize unrelated changes or production actions.",
]
CHECK_INVENTORY_CHECK = "Validator check inventory matches its canonical ordered sequence"
ORDINARY_CHECK_NAMES = (
    "Required pack files exist",
    "Manifest schema versions are supported integers",
    "C01 source pin table matches all four fixed references",
    "C02 production crate graph and boundary section matches fixed bytes",
    "Machine source pins match fixed references",
    "Parallel agent limit matches the subagent contract",
    "Authoritative concurrency declaration matches graph and subagent limit",
    "Task statuses use documented values",
    "Acceptance case statuses use documented values",
    "Acceptance evidence levels use documented values",
    "Acceptance case IDs map to fixed evidence levels",
    "Acceptance manifest status uses documented values",
    "Acceptance manifest status matches the case summary",
    "Task IDs are unique",
    "Case IDs are unique",
    "Acceptance cases have unique string task owners",
    "Declared task count matches",
    "Declared case count matches",
    "Expected task count",
    "Expected acceptance case count",
    "Task file and test write scopes match canonical paths",
    "Task required artifacts match canonical evidence set and report path",
    "Task completion rules match the fixed evidence contract",
    "Task review rules match the different-subagent and post-merge contract",
    "Task test_cases contain unique string IDs",
    "A13 preserves separate current-graph and final-post-deletion runs",
    "A15 separates observation, adapter, and integrated activation owners",
    "A16 preserves accepted-revision observation and product-presentation owners",
    "Release gate protocol matches fixed semantics",
    "Original specification matches its fixed checksum and both manifests",
    "GOAL.md matches its fixed checksum",
    "Task SPEC citations use valid syntax",
    "Task SPEC citation ranges are within SPEC.md",
    "Dependency lists contain unique string task IDs",
    "Dependencies reference other known tasks",
    "Release gate references name T40 and its owned final cases",
    "P04, T40, A123, and A125 mirror the fixed release gate contract",
    "T40 finalization writes route through Integration",
    "A125 verifies conditional patched-tuple approval",
    "P02 bootstrap records, P04 imports, and T12 reruns A05/A06",
    "P04 harness command registration and P05 build wiring scopes agree",
    "PACK-CHECKS limits release-gate claims to static structure",
    "P04 is listed under the Qualification subagent",
    "Subagent task assignments exactly match graph owners",
    "Review packet uses candidate input and post-merge completion stages",
    "Task progress waits for completed dependencies",
    "Dependency graph has no cycle",
    "Stored ready groups match dependencies without duplicate task IDs",
    "Assignment results have exact unique case owners and valid statuses",
    "Case status matches assignment summary",
    "NOT_STARTED and READY tasks have NOT_RUN assignments",
    "BLOCKED tasks with incomplete dependencies have NOT_RUN assignments",
    "Task test references exist",
    "PASS tasks have passing assigned case results",
    "Every case has a task owner",
    "Case and task ownership maps agree",
    "Task graph fields match TASKS index table",
    "Task graph fields match PLAN task index table",
    "PLAN task headings match the task graph",
    "Task graph fields match PLAN task cards",
    "Task graph fields and canonical preamble match individual task packets",
    "Acceptance detailed-case anchor is unique",
    "Acceptance policy before detailed cases matches fixed checksum",
    "Acceptance case headings have no missing, duplicate, or extra blocks",
    "Acceptance case fields match the acceptance-case manifest",
    "Every specification heading is mapped",
    "Traceability ranges match exact specification sections",
    "Task citations are represented in traceability",
    "Traceability table matches its manifest",
    "Traceability uses known task owners",
    "Checked procedural sentences contain at most 20 whitespace-separated words",
)
INITIAL_ONLY_CHECK_NAMES = (
    "Initial task, case, and assignment statuses claim no execution",
    "Pack records no runtime tests or delegated reviews",
)


def canonical_check_inventory(initial: bool = False) -> tuple[str, ...]:
    initial_checks = INITIAL_ONLY_CHECK_NAMES if initial else ()
    return (*ORDINARY_CHECK_NAMES, *initial_checks, CHECK_INVENTORY_CHECK)


SOURCE_BASIS_PATTERN = re.compile(r"SPEC L\d+(?:-L\d+)?(?:; L\d+(?:-L\d+)?)*")
SOURCE_CITATION_PATTERN = re.compile(r"L(\d+)(?:-L(\d+))?")
RELEASE_GATE_PROTOCOL = {
    "schema_version": 1,
    "task_id": "T40",
    "preflight": {
        "required_tasks": "Every task other than T40",
        "required_task_status": "PASS",
        "required_assignments": "PASS for every assignment owned by a prior task",
        "required_evidence": (
            "Every required task artifact, independent review, and merged-source check is present and passes"
        ),
        "required_targets": "PASS for every required target row",
        "allowed_t40_statuses": ["RUNNING", "REVIEW"],
        "excluded_final_cases": ["A123", "A124", "A125"],
        "result": "PREFLIGHT_ONLY",
        "may_approve_release": False,
    },
    "finalization": {
        "required_prior_task_status": "PASS",
        "required_assignment_status": "PASS for every case assignment, including A123, A124, and A125",
        "required_t40_cases": ["A123", "A124", "A125"],
        "required_evidence": [
            "A123 missing, failed, blocked, and unreviewed prerequisite refusal assertions",
            "Actual A124 operational documentation result",
            "Actual A125 provenance and authorization result",
            "Conditional maintainer-approval records for supported patched tuples, verified by A125",
            "All required target records and task artifacts",
            "Independent T40 review",
            "Merged-source checks",
        ],
        "conditional_patch_approval": {
            "condition": "A supported compatibility tuple contains one or more patches",
            "record_path": "evidence/openshell-v2/T40/maintainer-approvals.json",
            "one_record_per_supported_patched_tuple": True,
            "required_fields": [
                "tuple_id",
                "patch_digest",
                "maintainer_identity",
                "approval_decision",
                "approval_reference",
            ],
            "required_approval_decision": "APPROVED",
            "verified_by_case": "A125",
            "unpatched_tuples_require_record": False,
        },
        "synthetic_gate_fixtures_are_candidate_evidence": False,
        "excluded_inputs": ["Persisted T40 task status PASS", "The future finalization receipt"],
        "success_outputs": [
            "Persist T40 task status PASS",
            "Persist the final acceptance summary",
            "The finalization receipt",
            "The completion record",
            "The approval record",
        ],
        "approval_binds": ["Reviewed source commit", "Package digests", "Evidence hashes"],
        "approval_authority": "Independent review subagent",
        "finalization_writer": "Integration subagent",
        "completion_and_approval_together": True,
        "all_success_outputs_written_atomically": True,
        "failed_prerequisite_outputs": [],
    },
}
P04_RELEASE_GATE_STEP = "Implement the fixed release_gate_protocol in the evidence harness."
P04_RELEASE_GATE_ACCEPTANCE = (
    "Preflight cannot approve release. Finalization requires every fixed protocol prerequisite."
)
QUALIFICATION_SUBAGENT_ROW = "| Qualification subagent | P04, U08, and T35 through T38. |"
TASK_ID_PATTERN = re.compile(r"\b[PUT]\d{2}\b")
TASK_ID_RANGE_PATTERN = re.compile(r"\b([PUT])(\d{2}) through \1(\d{2})\b")
T40_RELEASE_GATE_STEPS = [
    "Run the release_gate_protocol preflight while T40 is RUNNING or REVIEW.",
    "Check prior task statuses and assignment results.",
    "Check reviewed artifacts, merged evidence, and required target results.",
    "Exercise A123 refusals for missing, failed, blocked, and unreviewed prerequisites.",
    "Confirm every refusal prevents approval. Treat synthetic refusal fixtures as contract tests only.",
    "Run A124 and A125 against the actual candidate. Record actual results for all three T40 cases.",
    "Obtain an independent T40 review and rerun checks against the merged source.",
    "Finalize only after all prior tasks and every case assignment pass, including A123, A124, and A125.",
    "Bind completion and approval to the reviewed source commit, package digests, and evidence hashes.",
    "Request Integration to atomically write T40 PASS, the final summary, completion, finalization receipt, and approval after all gates pass.",
]
T40_RELEASE_GATE_ACCEPTANCE = [
    "Preflight requires every task except T40 and each prior task assignment to pass.",
    "Prior task artifacts, independent reviews, merged evidence, and required target results must pass.",
    "T40 is RUNNING or REVIEW during preflight.",
    "Preflight excludes A123, A124, and A125, returns PREFLIGHT_ONLY, and never approves release.",
    "Finalization requires every task except T40 and every case assignment to pass, including A123, A124, and A125.",
    "Finalization also requires all targets and artifacts, an independent T40 review, and merged-source checks.",
    "Finalization excludes only persisted T40=PASS and its own future receipt from the input set.",
    "T40 requests Integration to write T40 PASS, final summary, completion, finalization receipt, and approval atomically. T40 writes none directly.",
    "Approval binds the reviewed source commit, package digests, and evidence hashes.",
    "Synthetic gate fixtures test refusals only and never count as candidate evidence.",
    "A failed prerequisite writes neither a completion record nor approval.",
]
RELEASE_GATE_SCOPE_SENTENCE = (
    "These release-gate checks validate static planning structure only. "
    "They do not run a release preflight, test a candidate, or issue approval."
)
A123_GATE_ACTION = (
    "Run missing, failed, blocked, and unreviewed prerequisite refusal assertions. "
    "Inspect the separate finalization contract."
)
A123_GATE_EXPECTED = (
    "Every refusal prevents approval. Finalization requires all three actual T40 case results "
    "and every fixed prerequisite."
)
A125_GATE_ACTION = (
    "Build the release candidate and inspect its provenance, conditional maintainer-approval records "
    "for supported patched tuples, and publication actions."
)
A125_GATE_EXPECTED = (
    "Artifacts match the approved source. For each supported patched tuple, "
    "evidence/openshell-v2/T40/maintainer-approvals.json records maintainer approval bound to tuple ID and "
    "patch digest. Unpatched tuples need no approval record. No unauthorized publication or production change occurs."
)
T40_RELEASE_GATE_WRITE_SCOPES = [
    "Release-candidate documentation and final compatibility matrix",
    "Version and packaging changes through the integration subagent",
    "Submit T40 task status, final acceptance summary, completion, finalization receipt, and approval "
    "as an atomic Integration subagent patch request",
]
T40_RELEASE_GATE_ACCEPTANCE.append(
    "Supported patched tuples require maintainer approval bound to tuple ID and patch digest. Unpatched tuples need no approval record."
)
REVIEW_PACKET_REQUIREMENTS = [
    "Give the review subagent the original task card, the input commit SHA, and the candidate task diff.",
    "Bind the candidate task diff to both the input commit SHA and candidate commit SHA.",
    "Resolve review findings before the integration merge.",
    "Run merged-result checks before task completion.",
]
TASK_ARTIFACTS = ("implementation.json", "review.json", "test-results.json",
                  "commands.jsonl", "source-map.json")
EARLY_LIVE_RECORD_PATH = "evidence/openshell-v2/P02/early-live-records.json"
P02_EARLY_LIVE_STEP = (
    "Record actual P02 LIVE runs for A05 and A06 before P03 starts."
)
P02_EARLY_LIVE_ACCEPTANCE = (
    "Early A05 and A06 records bind targets, commands, outputs, exits, tools, source, retained evidence hashes, and independent review."
)
P04_EARLY_LIVE_STEP = (
    "Validate and import the P02 A05 and A06 records without assigning them to T12."
)
P04_EARLY_LIVE_ACCEPTANCE = (
    "Imported P02 results satisfy only P02 assignments; T12 runs both cases again."
)
T12_FRESH_LIVE_STEP = (
    "Run separate fresh A05 and A06 LIVE tests with new run IDs after the P04 harness is available."
)
T12_FRESH_LIVE_ACCEPTANCE = (
    "T12 records independent fresh runs for A05 and A06; P02 records do not satisfy T12."
)
P04_COMMAND_REGISTRATION_SCOPE = (
    "crates/tools/jackin-xtask/src/main.rs: v2 command enum and dispatch only, through the Integration subagent"
)
P04_COMMAND_REGISTRATION_STEP = (
    "Request Integration to register the four v2 commands in the existing xtask enum and dispatch before P04 qualification."
)
P04_COMMAND_REGISTRATION_ACCEPTANCE = (
    "All four harness commands resolve through the registered xtask command enum."
)
P05_BUILD_REGISTRATION_SCOPE = (
    "crates/tools/jackin-xtask and test-support workspace and build graph registration only"
)
P05_BUILD_REGISTRATION_STEP = (
    "Register jackin-xtask and test-support in the production workspace and build graph."
)


class DuplicateJSONKeyError(ValueError):
    pass


def _unique_json_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise DuplicateJSONKeyError(f"duplicate JSON object key: {key}")
        value[key] = item
    return value


def strict_json_loads(document: str) -> Any:
    return json.loads(document, object_pairs_hook=_unique_json_object)


def read_json(path: Path) -> dict[str, Any]:
    value = strict_json_loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"Expected a JSON object: {path}")
    return value


def dependency_text(task: dict[str, Any]) -> str:
    dependencies = task.get("depends_on")
    if (not isinstance(dependencies, list)
            or any(not isinstance(dependency, str) for dependency in dependencies)):
        return "[invalid dependency list]"
    return ", ".join(dependencies) or "None"


def sentences(value: str) -> list[str]:
    return [part.strip() for part in re.split(r"(?<=[.!?])\s+", value) if part.strip()]


def render_task_card(task: dict[str, Any]) -> str:
    dependencies = dependency_text(task)
    case_references = task.get("test_cases")
    cases = ", ".join(case_id for case_id in case_references
                       if isinstance(case_id, str)) if isinstance(case_references, list) else "None"
    cases = cases or "None"
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
        f"**Task report:** `{task['report_path']}`.",
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


def render_task_packet(task: dict[str, Any]) -> str:
    return "\n".join([
        f"# Task packet: {task['id']}",
        "",
        *TASK_PACKET_INSTRUCTIONS,
        "",
        render_task_card(task),
    ])


def render_acceptance_case(case: dict[str, Any]) -> str:
    tasks = ", ".join(case["tasks"])
    assignments = "; ".join(f"{task_id}=NOT_RUN" for task_id in case["tasks"])
    return "\n".join(
        [
            f"### {case['id']} — {case['title']}",
            "",
            f"**Evidence:** {case['level']}. **Tasks:** {tasks}. **Initial status:** NOT_RUN. **Initial assignment status:** {assignments}.",
            "",
            f"**Setup:** {case['setup']}",
            "",
            f"**Action:** {case['action']}",
            "",
            f"**Required result:** {case['expected']}",
        ]
    )


def case_status_summary(statuses: list[str]) -> str:
    if "FAIL" in statuses:
        return "FAIL"
    if "BLOCKED" in statuses:
        return "BLOCKED"
    if statuses and all(status == "PASS" for status in statuses):
        return "PASS"
    return "NOT_RUN"


def owner_case_contract_errors(tasks: list[dict[str, Any]],
                               cases: list[dict[str, Any]]) -> dict[str, list[dict[str, Any]]]:
    tasks_by_id = {task.get("id"): task for task in tasks}
    cases_by_id = {case.get("id"): case for case in cases}
    failures: dict[str, list[dict[str, Any]]] = {
        "A13 preserves separate current-graph and final-post-deletion runs": [],
        "A15 separates observation, adapter, and integrated activation owners": [],
        "A16 preserves accepted-revision observation and product-presentation owners": [],
    }

    def task_contains(task_id: str, field: str, phrase: str) -> bool:
        task = tasks_by_id.get(task_id, {})
        values = task.get(field, [])
        return isinstance(values, list) and any(
            isinstance(value, str) and phrase in value for value in values
        )

    def case_text_contains(case: dict[str, Any], field: str, *phrases: str) -> bool:
        value = case.get(field)
        return isinstance(value, str) and all(phrase in value for phrase in phrases)

    a13 = cases_by_id.get("A13", {})
    a13_errors = failures["A13 preserves separate current-graph and final-post-deletion runs"]
    if a13.get("tasks") != ["P05", "T39"]:
        a13_errors.append({"case_owners": a13.get("tasks"), "expected": ["P05", "T39"]})
    if not case_text_contains(a13, "setup", "newly created current graph",
                              "Cargo metadata", "final candidate graph after legacy deletion"):
        a13_errors.append({"case": "A13", "field": "setup",
                           "problem": "must distinguish both Cargo metadata graph runs"})
    if not case_text_contains(
        a13, "action",
        "P05 captures Cargo metadata for the newly created current graph",
        "T39 captures fresh Cargo metadata for the final candidate graph",
    ):
        a13_errors.append({"case": "A13", "field": "action",
                           "problem": "P05 and T39 must each capture Cargo metadata for their graph phase"})
    if not task_contains("P05", "test_cases", "A13") or not task_contains(
        "P05", "steps", "Run A13 against the newly created current graph and compile forbidden-import fixtures."
    ) or not task_contains("P05", "acceptance",
                           "A13 records the production graph and forbidden-import result created by P05."):
        a13_errors.append({"task": "P05", "problem": "current-graph A13 run is missing"})
    if not task_contains("T39", "test_cases", "A13") or not task_contains(
        "T39", "steps", "Rerun A13 and A14 against the final graph and composition after legacy deletion."
    ) or not task_contains("T39", "steps",
                           "Record this result separately from P05's current-graph A13 run."):
        a13_errors.append({"task": "T39", "problem": "separate final post-deletion A13 rerun is missing"})

    a15 = cases_by_id.get("A15", {})
    a15_errors = failures["A15 separates observation, adapter, and integrated activation owners"]
    if a15.get("tasks") != ["U00", "T13", "T23"]:
        a15_errors.append({"case_owners": a15.get("tasks"), "expected": ["U00", "T13", "T23"]})
    if not case_text_contains(a15, "setup", "observation and adapter layers",
                              "integrated activation through StartSession"):
        a15_errors.append({"case": "A15", "field": "setup",
                           "problem": "must distinguish layer and integrated activation"})
    if not case_text_contains(a15, "expected", "U00 and T13 results do not replace T23's full activation attempt",
                              "A helper echo cannot replace gateway evidence"):
        a15_errors.append({"case": "A15", "field": "expected",
                           "problem": "must retain layer boundary and helper-claim refusal"})
    for task_id, field, phrase in (
        ("U00", "acceptance", "A15 and A16 at U00 qualify LIVE observation results, not StartSession or product presentation."),
        ("T13", "acceptance", "A15 and A16 at T13 qualify LIVE adapter results, not integrated activation or product presentation."),
        ("T23", "steps", "Exercise A15 through integrated StartSession with missing, stale, or mismatched authority observations; reject forged helper claims."),
        ("T23", "acceptance", "A15 fails closed at StartSession for missing, stale, or mismatched observations and helper-reported identity."),
    ):
        if not task_contains(task_id, "test_cases", "A15") or not task_contains(task_id, field, phrase):
            a15_errors.append({"task": task_id, "problem": "owner-specific A15 qualification is missing"})

    a16 = cases_by_id.get("A16", {})
    a16_errors = failures["A16 preserves accepted-revision observation and product-presentation owners"]
    if a16.get("tasks") != ["U00", "T13", "T29"]:
        a16_errors.append({"case_owners": a16.get("tasks"), "expected": ["U00", "T13", "T29"]})
    if not case_text_contains(a16, "setup", "verify accepted revisions and replacement source",
                              "T29 qualifies product presentation"):
        a16_errors.append({"case": "A16", "field": "setup",
                           "problem": "must distinguish observation and presentation owners"})
    if not case_text_contains(a16, "expected", "accepted revision and source",
                              "imaginary policy intersection"):
        a16_errors.append({"case": "A16", "field": "expected",
                           "problem": "must require accepted replacement source without an intersection"})
    a16_action = a16.get("action", "")
    load_policy = a16_action.find("Load the global policy.") if isinstance(a16_action, str) else -1
    query_revision = a16_action.find("Query actual accepted revisions") if isinstance(a16_action, str) else -1
    display_source = a16_action.find("display the effective policy source") if isinstance(a16_action, str) else -1
    if not (0 <= load_policy < query_revision < display_source):
        a16_errors.append({"case": "A16", "field": "action",
                           "problem": "must load the global policy before querying revisions and displaying its source"})
    for task_id, field, phrase in (
        ("U00", "acceptance", "A15 and A16 at U00 qualify LIVE observation results, not StartSession or product presentation."),
        ("U00", "acceptance", "A16 at U00 verifies the accepted revision and replacement source without claiming a policy intersection."),
        ("T13", "acceptance", "A15 and A16 at T13 qualify LIVE adapter results, not integrated activation or product presentation."),
        ("T13", "acceptance", "A16 at T13 binds the effective replacement source to the observed accepted revision without claiming an intersection."),
        ("T29", "steps", "Display A16's accepted policy revision and effective replacement source in product output."),
    ):
        if not task_contains(task_id, "test_cases", "A16") or not task_contains(task_id, field, phrase):
            a16_errors.append({"task": task_id, "problem": "owner-specific A16 qualification is missing"})
    return failures


def render_task_index(tasks: list[dict[str, Any]]) -> list[str]:
    lines = [
        "| ID | Task | Subagent | Dependencies |",
        "| --- | --- | --- | --- |",
    ]
    for task in tasks:
        dependencies = dependency_text(task)
        lines.append(
            f"| {task['id']} | {task['title']} | {task['owner']} | {dependencies} |"
        )
    return lines


def subagent_task_assignment_errors(document: str, tasks: list[dict[str, Any]]) -> list[dict[str, Any]]:
    heading = "## Subagent assignments"
    following_heading = "## Initial parallel source review"
    headings = list(re.finditer(r"(?m)^" + re.escape(heading) + r"\s*$", document))
    if len(headings) != 1:
        return [{"problem": "expected one subagent-assignment heading", "count": len(headings)}]
    start = headings[0].end()
    following = list(re.finditer(r"(?m)^" + re.escape(following_heading) + r"\s*$", document))
    following = [match for match in following if match.start() > start]
    if len(following) != 1:
        return [{"problem": "expected one following source-review heading", "count": len(following)}]

    rows: dict[str, list[str]] = {}
    errors: list[dict[str, Any]] = []
    table_lines = [line.strip() for line in document[start:following[0].start()].splitlines()
                   if line.lstrip().startswith("|")]
    if len(table_lines) < 3 or table_lines[0] != "| Subagent | Task ownership |":
        errors.append({"problem": "subagent-assignment table header is missing or malformed"})
    for line in table_lines[2:]:
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 2 or not cells[0]:
            errors.append({"problem": "malformed subagent-assignment row", "row": line})
            continue
        owner, responsibilities = cells
        if owner in rows:
            errors.append({"owner": owner, "problem": "duplicate subagent row"})
            continue

        expanded = responsibilities

        def expand_range(match: re.Match[str]) -> str:
            prefix, first, last = match.group(1), int(match.group(2)), int(match.group(3))
            if last < first:
                errors.append({"owner": owner, "problem": "descending task range", "range": match.group(0)})
                return match.group(0)
            return " ".join(f"{prefix}{number:02d}" for number in range(first, last + 1))

        expanded = TASK_ID_RANGE_PATTERN.sub(expand_range, expanded)
        assigned = TASK_ID_PATTERN.findall(expanded)
        if len(assigned) != len(set(assigned)):
            errors.append({"owner": owner, "problem": "duplicate task ID in assignment row", "tasks": assigned})
        rows[owner] = assigned

    expected: dict[str, list[str]] = {}
    for task in tasks:
        owner = task.get("owner")
        task_id = task.get("id")
        if not isinstance(owner, str) or not isinstance(task_id, str):
            errors.append({"problem": "graph owner and task ID must be strings", "task": task})
            continue
        expected.setdefault(owner, []).append(task_id)
    for values in expected.values():
        values.sort()
    for owner in sorted(set(rows) | set(expected)):
        actual = sorted(rows.get(owner, []))
        graph_owned = expected.get(owner, [])
        if actual != graph_owned:
            errors.append({"owner": owner, "table_tasks": actual, "graph_tasks": graph_owned})
    graph_ids = {task.get("id") for task in tasks}
    for owner, assigned in rows.items():
        unknown = sorted(set(assigned) - graph_ids)
        if unknown:
            errors.append({"owner": owner, "problem": "unknown task IDs", "tasks": unknown})
    return errors


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


def source_pin_table_errors(document: str) -> list[dict[str, Any]]:
    heading = "## C01. Source and compatibility pins"
    headings = list(re.finditer(r"(?m)^" + re.escape(heading) + r"\s*$", document))
    if len(headings) != 1:
        return [{"problem": "expected one C01 source-pin heading", "count": len(headings)}]
    following = list(re.finditer(r"(?m)^## C02\. Production crate graph\s*$", document))
    following = [match for match in following if match.start() > headings[0].end()]
    if len(following) != 1:
        return [{"problem": "expected one C02 heading after C01", "count": len(following)}]
    section = document[headings[0].end():following[0].start()]
    actual_rows = [line.strip() for line in section.splitlines()
                   if line.lstrip().startswith("|")]
    if actual_rows != SOURCE_PIN_TABLE_ROWS:
        return [{"expected_rows": SOURCE_PIN_TABLE_ROWS, "actual_rows": actual_rows}]
    return []


def c02_contract_section_errors(document: bytes) -> list[dict[str, Any]]:
    start_heading = rb"(?m)^## C02\. Production crate graph[ \t]*\r?$"
    end_heading = rb"(?m)^## C03\. Configuration and identifiers[ \t]*\r?$"
    starts = list(re.finditer(start_heading, document))
    ends = list(re.finditer(end_heading, document))
    if len(starts) != 1:
        return [{"problem": "expected one C02 heading", "count": len(starts)}]
    if len(ends) != 1 or ends[0].start() <= starts[0].start():
        return [{"problem": "expected one C03 boundary after C02", "count": len(ends)}]
    section = document[starts[0].start():ends[0].start()]
    actual = hashlib.sha256(section).hexdigest()
    if actual != C02_CONTRACT_SECTION_SHA256:
        return [{"expected_sha256": C02_CONTRACT_SECTION_SHA256, "actual_sha256": actual}]
    return []


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
                "PACK-CHECKS.md", "validate_pack.py", "validate_assignment_fixtures.py", "task-graph.json",
                "acceptance-cases.json", "traceability.json"]
    missing = [name for name in required if not (root / name).is_file()]
    check("Required pack files exist", not missing, missing)
    if missing:
        result = {"passed": False, "initial": args.initial,
                  "scope": "Static planning-pack checks only",
                  "not_checked": ["Product compilation", "Runtime capabilities", "Independent code review",
                                  "Full ASD-STE100 dictionary conformity"],
                  "counts": {}, "checks": checks, "failures": failures}
        print(json.dumps(result, indent=2))
        return 1

    graph = read_json(root / "task-graph.json")
    cases_file = read_json(root / "acceptance-cases.json")
    trace = read_json(root / "traceability.json")
    manifest_versions = {
        "task-graph.json": graph.get("schema_version"),
        "acceptance-cases.json": cases_file.get("schema_version"),
        "traceability.json": trace.get("schema_version"),
    }
    unsupported_versions = {name: version for name, version in manifest_versions.items()
                            if type(version) is not int or version != SUPPORTED_SCHEMA_VERSION}
    check("Manifest schema versions are supported integers",
          not unsupported_versions,
          {"supported": SUPPORTED_SCHEMA_VERSION, "actual": manifest_versions,
           "unsupported": unsupported_versions})
    contracts_bytes = (root / "CONTRACTS.md").read_bytes()
    contracts_document = contracts_bytes.decode("utf-8")
    c01_source_table_errors = source_pin_table_errors(contracts_document)
    check("C01 source pin table matches all four fixed references",
          not c01_source_table_errors, c01_source_table_errors)
    c02_contract_errors = c02_contract_section_errors(contracts_bytes)
    check("C02 production crate graph and boundary section matches fixed bytes",
          not c02_contract_errors, c02_contract_errors)
    source_pin_errors = {field: {"expected": expected, "actual": graph.get(field)}
                         for field, expected in SOURCE_PINS.items()
                         if graph.get(field) != expected}
    check("Machine source pins match fixed references", not source_pin_errors, source_pin_errors)
    check("Parallel agent limit matches the subagent contract",
          graph.get("parallel_agent_limit") == PARALLEL_AGENT_LIMIT,
          {"expected": PARALLEL_AGENT_LIMIT, "actual": graph.get("parallel_agent_limit")})
    subagents = (root / "SUBAGENTS.md").read_text(encoding="utf-8")
    concurrency_declarations = list(CONCURRENCY_DECLARATION.finditer(subagents))
    concurrency_errors = []
    if len(concurrency_declarations) != 1:
        concurrency_errors.append({"declarations": len(concurrency_declarations), "expected": 1})
    else:
        declared_token = concurrency_declarations[0].group(1)
        declared_limit = (int(declared_token) if declared_token.isdigit()
                          else CONCURRENCY_WORD_VALUES[declared_token])
        if declared_limit != PARALLEL_AGENT_LIMIT:
            concurrency_errors.append({"declaration": declared_limit,
                                       "expected": PARALLEL_AGENT_LIMIT})
        if graph.get("parallel_agent_limit") != declared_limit:
            concurrency_errors.append({"graph": graph.get("parallel_agent_limit"),
                                       "declaration": declared_limit})
    if subagents.count(REVIEW_CAPACITY_SENTENCE) != 1:
        concurrency_errors.append({"review_capacity_sentence_count":
                                   subagents.count(REVIEW_CAPACITY_SENTENCE), "expected": 1})
    check("Authoritative concurrency declaration matches graph and subagent limit",
          not concurrency_errors, concurrency_errors)
    tasks = graph["tasks"]
    cases = cases_file["cases"]
    owner_contract_errors = owner_case_contract_errors(tasks, cases)
    dependency_list_errors = []
    for task in tasks:
        dependencies = task.get("depends_on")
        if not isinstance(dependencies, list):
            dependency_list_errors.append({"task": task.get("id"),
                                           "problem": "depends_on must be a list of strings"})
        elif any(not isinstance(dependency, str) for dependency in dependencies):
            dependency_list_errors.append({"task": task.get("id"),
                                           "problem": "depends_on entries must be strings",
                                           "depends_on": dependencies})
        elif len(dependencies) != len(set(dependencies)):
            dependency_list_errors.append({"task": task.get("id"),
                                           "problem": "duplicate dependency ID",
                                           "depends_on": dependencies})
    dependency_lists_valid = not dependency_list_errors
    task_ids = [task["id"] for task in tasks]
    case_ids = [case["id"] for case in cases]
    task_set, case_set = set(task_ids), set(case_ids)
    task_statuses = {"NOT_STARTED", "READY", "RUNNING", "REVIEW", "PASS", "FAIL", "BLOCKED"}
    case_statuses = CASE_STATUSES
    bad_case_levels = [{"id": case["id"], "level": case["level"]} for case in cases
                       if case["level"] not in ACCEPTANCE_EVIDENCE_LEVELS]
    bad_task_statuses = [{"id": task["id"], "status": task["status"]} for task in tasks
                         if task["status"] not in task_statuses]
    bad_case_statuses = [{"id": case["id"], "status": case["status"]} for case in cases
                         if case["status"] not in case_statuses]
    check("Task statuses use documented values", not bad_task_statuses, bad_task_statuses)
    check("Acceptance case statuses use documented values", not bad_case_statuses, bad_case_statuses)
    check("Acceptance evidence levels use documented values", not bad_case_levels, bad_case_levels)
    actual_case_levels: dict[str, str] = {}
    duplicate_level_ids = []
    invalid_level_ids = []
    for case in cases:
        case_id = case.get("id")
        if not isinstance(case_id, str):
            invalid_level_ids.append(case_id)
        elif case_id in actual_case_levels:
            duplicate_level_ids.append(case_id)
        else:
            actual_case_levels[case_id] = case.get("level")
    level_map_errors = {
        "missing": sorted(set(EXPECTED_CASE_LEVELS) - set(actual_case_levels)),
        "extra": sorted(set(actual_case_levels) - set(EXPECTED_CASE_LEVELS)),
        "wrong_levels": [
            {"case": case_id, "expected": level, "actual": actual_case_levels.get(case_id)}
            for case_id, level in EXPECTED_CASE_LEVELS.items()
            if actual_case_levels.get(case_id) != level
        ],
        "duplicate_ids": duplicate_level_ids,
        "invalid_ids": invalid_level_ids,
        "expected_counts": EXPECTED_CASE_LEVEL_COUNTS,
        "actual_counts": {
            level: sum(case.get("level") == level for case in cases)
            for level in ACCEPTANCE_EVIDENCE_LEVELS
        },
    }
    level_map_valid = (
        not level_map_errors["missing"]
        and not level_map_errors["extra"]
        and not level_map_errors["wrong_levels"]
        and not level_map_errors["duplicate_ids"]
        and not level_map_errors["invalid_ids"]
        and level_map_errors["actual_counts"] == EXPECTED_CASE_LEVEL_COUNTS
    )
    check("Acceptance case IDs map to fixed evidence levels", level_map_valid, level_map_errors)
    manifest_status = cases_file.get("status")
    check("Acceptance manifest status uses documented values",
          isinstance(manifest_status, str) and manifest_status in case_statuses,
          manifest_status)
    expected_manifest_status = case_status_summary([case.get("status") for case in cases])
    check("Acceptance manifest status matches the case summary",
          manifest_status == expected_manifest_status,
          {"expected": expected_manifest_status, "actual": manifest_status})
    check("Task IDs are unique", len(task_set) == len(tasks))
    check("Case IDs are unique", len(case_set) == len(cases))
    case_owner_errors = []
    for case in cases:
        owners = case.get("tasks")
        if not isinstance(owners, list) or any(not isinstance(owner, str) for owner in owners):
            case_owner_errors.append({"case": case.get("id"), "problem": "tasks must be a list of strings"})
        elif len(owners) != len(set(owners)):
            case_owner_errors.append({"case": case.get("id"), "problem": "duplicate task owner", "tasks": owners})
    check("Acceptance cases have unique string task owners", not case_owner_errors, case_owner_errors)
    check("Declared task count matches", graph["task_count"] == len(tasks), len(tasks))
    check("Declared case count matches", graph["acceptance_case_count"] == len(cases), len(cases))
    check("Expected task count", len(tasks) == 46, len(tasks))
    check("Expected acceptance case count", len(cases) == 125, len(cases))
    task_path_errors = []
    canonical_task_cards: dict[str, Path] = {}
    for task in tasks:
        task_id = task.get("id")
        if not isinstance(task_id, str) or CANONICAL_TASK_ID.fullmatch(task_id) is None:
            task_path_errors.append({"task": task_id, "problem": "invalid task ID for a task path"})
            continue
        expected_task_file = f"tasks/{task_id}.md"
        expected_test_scope = f"tests/v2/tasks/{task_id}/"
        canonical_task_cards[task_id] = root / "tasks" / f"{task_id}.md"
        if task.get("task_file") != expected_task_file:
            task_path_errors.append({"task": task_id, "field": "task_file",
                                     "expected": expected_task_file,
                                     "actual": task.get("task_file")})
        if task.get("test_write_scope") != expected_test_scope:
            task_path_errors.append({"task": task_id, "field": "test_write_scope",
                                     "expected": expected_test_scope,
                                     "actual": task.get("test_write_scope")})
    check("Task file and test write scopes match canonical paths",
          not task_path_errors, task_path_errors)
    artifact_errors = []
    for task in tasks:
        task_id = task.get("id")
        task_root = f"evidence/openshell-v2/{task_id}"
        expected_report = f"{task_root}/implementation.json"
        expected_artifacts = [f"{task_root}/{name}" for name in TASK_ARTIFACTS]
        expected_artifacts.append(
            f"{task_root}/upstream-change.json" if isinstance(task_id, str)
            and task_id.startswith("U") else f"{task_root}/diff.patch"
        )
        if task_id == "P02":
            expected_artifacts.append(EARLY_LIVE_RECORD_PATH)
        if task.get("report_path") != expected_report or task.get("required_artifacts") != expected_artifacts:
            artifact_errors.append({
                "task": task_id,
                "expected_report_path": expected_report,
                "actual_report_path": task.get("report_path"),
                "expected_artifacts": expected_artifacts,
                "actual_artifacts": task.get("required_artifacts"),
            })
    check("Task required artifacts match canonical evidence set and report path",
          not artifact_errors, artifact_errors)
    bad_completion_rules = [
        {"task": task.get("id"), "completion_rule": task.get("completion_rule")}
        for task in tasks if task.get("completion_rule") != TASK_COMPLETION_RULE
    ]
    check("Task completion rules match the fixed evidence contract",
          not bad_completion_rules, bad_completion_rules)
    bad_review_rules = [
        {"task": task.get("id"), "review_rule": task.get("review_rule")}
        for task in tasks if task.get("review_rule") != TASK_REVIEW_RULE
    ]
    check("Task review rules match the different-subagent and post-merge contract",
          not bad_review_rules, bad_review_rules)
    task_case_reference_errors = []
    for task in tasks:
        references = task.get("test_cases")
        if not isinstance(references, list):
            task_case_reference_errors.append({"task": task.get("id"), "problem": "test_cases must be a list"})
        elif any(not isinstance(case_id, str) for case_id in references):
            task_case_reference_errors.append({
                "task": task.get("id"), "problem": "test_cases entries must be strings",
            })
        elif len(references) != len(set(references)):
            task_case_reference_errors.append({
                "task": task.get("id"), "problem": "duplicate test case ID", "test_cases": references,
            })
    check("Task test_cases contain unique string IDs", not task_case_reference_errors,
          task_case_reference_errors)
    for contract_name, contract_errors in owner_contract_errors.items():
        check(contract_name, not contract_errors, contract_errors)
    release_protocol = graph.get("release_gate_protocol")
    release_protocol_errors = [] if release_protocol == RELEASE_GATE_PROTOCOL else [
        {"actual": release_protocol, "expected": RELEASE_GATE_PROTOCOL}
    ]
    check("Release gate protocol matches fixed semantics", not release_protocol_errors,
          release_protocol_errors)

    actual_digest = hashlib.sha256((root / "SPEC.md").read_bytes()).hexdigest()
    spec_lines = (root / "SPEC.md").read_text(encoding="utf-8").splitlines()
    check("Original specification matches its fixed checksum and both manifests",
          actual_digest == ORIGINAL_SPEC_SHA256
          and graph["spec_sha256"] == ORIGINAL_SPEC_SHA256
          and trace["spec_sha256"] == ORIGINAL_SPEC_SHA256,
          {"actual": actual_digest, "graph": graph["spec_sha256"], "traceability": trace["spec_sha256"]})
    actual_goal_digest = hashlib.sha256((root / "GOAL.md").read_bytes()).hexdigest()
    check("GOAL.md matches its fixed checksum", actual_goal_digest == GOAL_SHA256,
          {"expected": GOAL_SHA256, "actual": actual_goal_digest})

    source_citation_syntax_errors = []
    source_citation_range_errors = []
    source_citations_by_task: dict[str, list[tuple[int, int]]] = {}
    for task in tasks:
        task_id = task["id"]
        source_basis = task.get("source_basis")
        if not isinstance(source_basis, str) or SOURCE_BASIS_PATTERN.fullmatch(source_basis) is None:
            source_citation_syntax_errors.append({"task": task_id, "source_basis": source_basis})
            continue
        citations = []
        for citation in source_basis.removeprefix("SPEC ").split("; "):
            match = SOURCE_CITATION_PATTERN.fullmatch(citation)
            if match is None:
                source_citation_syntax_errors.append({"task": task_id, "citation": citation})
                continue
            start_line = int(match.group(1))
            end_line = int(match.group(2) or match.group(1))
            if start_line < 1 or end_line < start_line or end_line > len(spec_lines):
                source_citation_range_errors.append({
                    "task": task_id, "citation": citation, "specification_lines": len(spec_lines),
                })
            else:
                citations.append((start_line, end_line))
        source_citations_by_task[task_id] = citations
    check("Task SPEC citations use valid syntax", not source_citation_syntax_errors,
          source_citation_syntax_errors)
    check("Task SPEC citation ranges are within SPEC.md", not source_citation_range_errors,
          source_citation_range_errors)

    check("Dependency lists contain unique string task IDs",
          not dependency_list_errors, dependency_list_errors)
    bad_dependencies = (
        [(task["id"], dep) for task in tasks for dep in task["depends_on"]
         if dep not in task_set or dep == task["id"]]
        if dependency_lists_valid else []
    )
    check("Dependencies reference other known tasks", not bad_dependencies, bad_dependencies)
    tasks_by_id = {task["id"]: task for task in tasks}
    cases_by_id = {case["id"]: case for case in cases}
    release_protocol_reference_errors = []
    if not isinstance(release_protocol, dict):
        release_protocol_reference_errors.append("release_gate_protocol must be an object")
    else:
        owner_id = release_protocol.get("task_id")
        if owner_id != "T40" or owner_id not in tasks_by_id:
            release_protocol_reference_errors.append({"task_id": owner_id, "expected_owner": "T40"})
        preflight = release_protocol.get("preflight")
        finalization = release_protocol.get("finalization")
        if not isinstance(preflight, dict) or not isinstance(finalization, dict):
            release_protocol_reference_errors.append("preflight and finalization must be objects")
        else:
            final_case_ids = finalization.get("required_t40_cases")
            if not isinstance(final_case_ids, list) or any(not isinstance(case_id, str)
                                                            for case_id in final_case_ids):
                release_protocol_reference_errors.append("required_t40_cases must be string IDs")
            else:
                if final_case_ids != ["A123", "A124", "A125"]:
                    release_protocol_reference_errors.append({
                        "required_t40_cases": final_case_ids,
                        "expected": ["A123", "A124", "A125"],
                    })
                if tasks_by_id.get("T40", {}).get("test_cases") != final_case_ids:
                    release_protocol_reference_errors.append("T40 must own every finalization case")
                if preflight.get("excluded_final_cases") != final_case_ids:
                    release_protocol_reference_errors.append("preflight exclusions must match T40 final cases")
                for case_id in final_case_ids:
                    if case_id not in case_set:
                        release_protocol_reference_errors.append({"unknown_final_case": case_id})
                    elif cases_by_id[case_id].get("tasks") != ["T40"]:
                        release_protocol_reference_errors.append({
                            "case": case_id, "owners": cases_by_id[case_id].get("tasks"),
                            "expected_owner": "T40",
                        })
    check("Release gate references name T40 and its owned final cases",
          not release_protocol_reference_errors, release_protocol_reference_errors)

    p04 = tasks_by_id.get("P04", {})
    t40 = tasks_by_id.get("T40", {})
    release_gate_task_mirror_errors = []
    if P04_RELEASE_GATE_STEP not in p04.get("steps", []):
        release_gate_task_mirror_errors.append("P04 must implement the fixed protocol")
    if P04_RELEASE_GATE_ACCEPTANCE not in p04.get("acceptance", []):
        release_gate_task_mirror_errors.append("P04 acceptance must keep preflight separate from approval")
    t40_steps = t40.get("steps", [])
    missing_t40_steps = [step for step in T40_RELEASE_GATE_STEPS if step not in t40_steps]
    if missing_t40_steps:
        release_gate_task_mirror_errors.append({"missing_T40_steps": missing_t40_steps})
    t40_acceptance = t40.get("acceptance", [])
    if t40_acceptance[-len(T40_RELEASE_GATE_ACCEPTANCE):] != T40_RELEASE_GATE_ACCEPTANCE:
        release_gate_task_mirror_errors.append("T40 acceptance must mirror preflight and finalization rules")
    if cases_by_id.get("A123", {}).get("tasks") != ["T40"]:
        release_gate_task_mirror_errors.append("A123 must be owned by T40")
    if (cases_by_id.get("A123", {}).get("action") != A123_GATE_ACTION
            or cases_by_id.get("A123", {}).get("expected") != A123_GATE_EXPECTED):
        release_gate_task_mirror_errors.append("A123 must specify refusal assertions and finalization contract")
    check("P04, T40, A123, and A125 mirror the fixed release gate contract",
          not release_gate_task_mirror_errors, release_gate_task_mirror_errors)
    t40_write_scope_errors = []
    if t40.get("write_scope") != T40_RELEASE_GATE_WRITE_SCOPES:
        t40_write_scope_errors.append({
            "actual": t40.get("write_scope"), "expected": T40_RELEASE_GATE_WRITE_SCOPES,
        })
    check("T40 finalization writes route through Integration",
          not t40_write_scope_errors, t40_write_scope_errors)
    a125 = cases_by_id.get("A125", {})
    a125_errors = []
    if a125.get("tasks") != ["T40"]:
        a125_errors.append({"owners": a125.get("tasks"), "expected": ["T40"]})
    if (a125.get("action") != A125_GATE_ACTION
            or a125.get("expected") != A125_GATE_EXPECTED):
        a125_errors.append({
            "action": a125.get("action"), "expected": a125.get("expected"),
            "required_action": A125_GATE_ACTION, "required_result": A125_GATE_EXPECTED,
        })
    check("A125 verifies conditional patched-tuple approval",
          not a125_errors, a125_errors)

    early_live_errors = []
    p02 = tasks_by_id.get("P02", {})
    p04 = tasks_by_id.get("P04", {})
    t12 = tasks_by_id.get("T12", {})
    for task, label, required_step, required_acceptance in [
        (p02, "P02", P02_EARLY_LIVE_STEP, P02_EARLY_LIVE_ACCEPTANCE),
        (p04, "P04", P04_EARLY_LIVE_STEP, P04_EARLY_LIVE_ACCEPTANCE),
        (t12, "T12", T12_FRESH_LIVE_STEP, T12_FRESH_LIVE_ACCEPTANCE),
    ]:
        if required_step not in task.get("steps", []):
            early_live_errors.append(f"{label} lacks the fixed LIVE evidence step")
        if required_acceptance not in task.get("acceptance", []):
            early_live_errors.append(f"{label} lacks the fixed LIVE evidence pass condition")
    if EARLY_LIVE_RECORD_PATH not in p02.get("write_scope", []):
        early_live_errors.append("P02 does not own the early LIVE record")
    for case_id in ("A05", "A06"):
        case = cases_by_id.get(case_id, {})
        if case.get("level") != "LIVE" or case.get("tasks") != ["P02", "T12"]:
            early_live_errors.append(f"{case_id} must remain LIVE and owned by P02 and T12")
    check("P02 bootstrap records, P04 imports, and T12 reruns A05/A06",
          not early_live_errors, early_live_errors)

    command_registration_errors = []
    if P04_COMMAND_REGISTRATION_SCOPE not in p04.get("write_scope", []):
        command_registration_errors.append("P04 scope must permit only the Integration command-dispatch patch")
    if P04_COMMAND_REGISTRATION_STEP not in p04.get("steps", []):
        command_registration_errors.append("P04 must request early command enum and dispatch registration")
    if P04_COMMAND_REGISTRATION_ACCEPTANCE not in p04.get("acceptance", []):
        command_registration_errors.append("P04 must require all four commands to resolve")
    if P05_BUILD_REGISTRATION_SCOPE not in tasks_by_id.get("P05", {}).get("write_scope", []):
        command_registration_errors.append("P05 must retain only workspace/build graph registration")
    if P05_BUILD_REGISTRATION_STEP not in tasks_by_id.get("P05", {}).get("steps", []):
        command_registration_errors.append("P05 must register workspace/build graph after P04")
    check("P04 harness command registration and P05 build wiring scopes agree",
          not command_registration_errors, command_registration_errors)

    pack_checks = (root / "PACK-CHECKS.md").read_text(encoding="utf-8")
    check("PACK-CHECKS limits release-gate claims to static structure",
          RELEASE_GATE_SCOPE_SENTENCE in pack_checks,
          RELEASE_GATE_SCOPE_SENTENCE if RELEASE_GATE_SCOPE_SENTENCE in pack_checks else "missing")
    subagents = (root / "SUBAGENTS.md").read_text(encoding="utf-8")
    qualification_owner_count = subagents.count(QUALIFICATION_SUBAGENT_ROW)
    check("P04 is listed under the Qualification subagent",
          qualification_owner_count == 1, qualification_owner_count)
    subagent_task_errors = subagent_task_assignment_errors(subagents, tasks)
    check("Subagent task assignments exactly match graph owners",
          not subagent_task_errors, subagent_task_errors)
    review_packet_errors = [line for line in REVIEW_PACKET_REQUIREMENTS if line not in subagents]
    check("Review packet uses candidate input and post-merge completion stages",
          not review_packet_errors, review_packet_errors)

    incomplete_dependencies = []
    if dependency_lists_valid:
        incomplete_dependencies = [
            {"task": task["id"], "status": task["status"],
             "dependencies_not_passed": sorted(
                 dep for dep in task["depends_on"]
                 if tasks_by_id.get(dep, {}).get("status") != "PASS")}
            for task in tasks if task["status"] in PROGRESS_TASK_STATUSES
            and any(tasks_by_id.get(dep, {}).get("status") != "PASS" for dep in task["depends_on"])
        ]
    check("Task progress waits for completed dependencies",
          not incomplete_dependencies, incomplete_dependencies)
    done: set[str] = set()
    levels: list[list[str]] = []
    if dependency_lists_valid:
        while len(done) < len(task_set):
            ready = [task["id"] for task in tasks if task["id"] not in done
                     and set(task["depends_on"]).issubset(done)]
            if not ready:
                break
            levels.append(ready)
            done.update(ready)
    check("Dependency graph has no cycle",
          not dependency_lists_valid or done == task_set,
          {"ready_groups": len(levels), "dependency_lists_valid": dependency_lists_valid})
    stored_groups = graph["topological_ready_groups"]
    stored_group_ids = [task_id for group in stored_groups for task_id in group]
    stored_group_counts: dict[str, int] = {}
    for task_id in stored_group_ids:
        stored_group_counts[task_id] = stored_group_counts.get(task_id, 0) + 1
    stored_group_duplicates = sorted(task_id for task_id, count in stored_group_counts.items() if count > 1)
    expected_group_lists = [sorted(group) for group in levels]
    stored_group_lists = [sorted(group) for group in stored_groups]
    check("Stored ready groups match dependencies without duplicate task IDs",
          not dependency_lists_valid
          or not stored_group_duplicates and stored_group_lists == expected_group_lists,
          {"duplicate_task_ids": stored_group_duplicates,
           "expected_group_count": len(expected_group_lists),
           "stored_group_count": len(stored_group_lists),
           "dependency_lists_valid": dependency_lists_valid})

    assignment_errors: list[dict[str, Any]] = []
    assignment_summary_errors: list[dict[str, Any]] = []
    assignment_status_by_case_and_task: dict[tuple[str, str], str] = {}
    task_cases_by_id = {
        task["id"]: [case_id for case_id in task.get("test_cases", []) if isinstance(case_id, str)]
        if isinstance(task.get("test_cases"), list) else []
        for task in tasks
    }
    expected_owners_by_case = {
        case_id: {task["id"] for task in tasks if case_id in task_cases_by_id[task["id"]]}
        for case_id in case_set
    }
    for case in cases:
        case_id = case["id"]
        expected_owners = expected_owners_by_case[case_id]
        rows = case.get("assignment_results")
        if not isinstance(rows, list):
            assignment_errors.append({"case": case_id, "problem": "assignment_results must be a list"})
            continue

        seen: set[str] = set()
        row_statuses: dict[str, str] = {}
        case_has_errors = False
        for index, row in enumerate(rows):
            location = {"case": case_id, "index": index}
            if not isinstance(row, dict) or set(row) != {"task_id", "status"}:
                assignment_errors.append({**location, "problem": "assignment row must contain only task_id and status"})
                case_has_errors = True
                continue

            task_id = row["task_id"]
            status = row["status"]
            if not isinstance(task_id, str):
                assignment_errors.append({**location, "problem": "task_id must be a string"})
                case_has_errors = True
            elif task_id not in expected_owners:
                assignment_errors.append({**location, "problem": "unexpected task_id", "task_id": task_id})
                case_has_errors = True
            elif task_id in seen:
                assignment_errors.append({**location, "problem": "duplicate task_id", "task_id": task_id})
                case_has_errors = True
            else:
                seen.add(task_id)

            if not isinstance(status, str) or status not in case_statuses:
                assignment_errors.append({**location, "problem": "undocumented assignment status", "status": status})
                case_has_errors = True
            elif isinstance(task_id, str) and task_id in expected_owners and task_id not in row_statuses:
                row_statuses[task_id] = status

        missing = sorted(expected_owners - seen)
        if missing:
            assignment_errors.append({"case": case_id, "problem": "missing task assignments", "task_ids": missing})
            case_has_errors = True
        if not case_has_errors and set(row_statuses) == expected_owners:
            statuses = [row_statuses[task_id] for task_id in sorted(expected_owners)]
            for task_id, status in row_statuses.items():
                assignment_status_by_case_and_task[(case_id, task_id)] = status
            expected_summary = case_status_summary(statuses)
            if case["status"] != expected_summary:
                assignment_summary_errors.append({"case": case_id, "expected": expected_summary,
                                                  "actual": case["status"]})

    check("Assignment results have exact unique case owners and valid statuses",
          not assignment_errors, assignment_errors)
    check("Case status matches assignment summary",
          not assignment_summary_errors, assignment_summary_errors)
    not_started_assignment_errors = [
        {"case": case_id, "task": task_id, "status": status}
        for (case_id, task_id), status in assignment_status_by_case_and_task.items()
        if tasks_by_id.get(task_id, {}).get("status") in {"NOT_STARTED", "READY"}
        and status != "NOT_RUN"
    ]
    check("NOT_STARTED and READY tasks have NOT_RUN assignments",
          not not_started_assignment_errors, not_started_assignment_errors)
    blocked_incomplete_assignment_errors = []
    for task in (tasks if dependency_lists_valid else []):
        if task.get("status") != "BLOCKED":
            continue
        incomplete_dependencies = [
            dependency for dependency in task.get("depends_on", [])
            if tasks_by_id.get(dependency, {}).get("status") != "PASS"
        ]
        if not incomplete_dependencies:
            continue
        for case_id in task_cases_by_id[task["id"]]:
            status = assignment_status_by_case_and_task.get((case_id, task["id"]))
            if status != "NOT_RUN":
                blocked_incomplete_assignment_errors.append({
                    "task": task["id"], "case": case_id, "status": status,
                    "dependencies_not_passed": sorted(incomplete_dependencies),
                })
    check("BLOCKED tasks with incomplete dependencies have NOT_RUN assignments",
          not blocked_incomplete_assignment_errors, blocked_incomplete_assignment_errors)

    unknown_cases = [(task["id"], cid) for task in tasks for cid in task_cases_by_id[task["id"]]
                     if cid not in case_set]
    check("Task test references exist", not unknown_cases, unknown_cases)
    pass_case_errors = [
        {"task": task["id"], "case": case_id,
         "status": assignment_status_by_case_and_task.get((case_id, task["id"]), "MISSING")}
        for task in tasks if task["status"] == "PASS"
        for case_id in task_cases_by_id[task["id"]]
        if assignment_status_by_case_and_task.get((case_id, task["id"])) != "PASS"
    ]
    check("PASS tasks have passing assigned case results", not pass_case_errors, pass_case_errors)
    covered = {cid for task in tasks for cid in task_cases_by_id[task["id"]]}
    check("Every case has a task owner", covered == case_set, sorted(case_set - covered))
    reverse_errors = []
    for case in cases:
        expected = {task["id"] for task in tasks
                    if case["id"] in task_cases_by_id[task["id"]]}
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

        task_id = task.get("id")
        card = canonical_task_cards.get(task_id) if isinstance(task_id, str) else None
        if card is None:
            card_errors.append(task_id)
            continue
        if not card.is_file():
            card_errors.append(task_id)
            continue
        card_text = card.read_text(encoding="utf-8")
        if card_text.strip() != render_task_packet(task).strip():
            card_errors.append(task["id"])
    check("Task graph fields match PLAN task cards", not plan_task_errors, plan_task_errors)
    check("Task graph fields and canonical preamble match individual task packets",
          not card_errors, card_errors)

    acceptance_bytes = (root / "ACCEPTANCE.md").read_bytes()
    marker_offsets = []
    offset = 0
    for line in acceptance_bytes.splitlines(keepends=True):
        if line.rstrip(b"\r\n") == b"## Detailed cases":
            marker_offsets.append(offset)
        offset += len(line)
    check("Acceptance detailed-case anchor is unique", len(marker_offsets) == 1,
          {"matches": len(marker_offsets)})
    acceptance_policy_digest = (hashlib.sha256(acceptance_bytes[:marker_offsets[0]]).hexdigest()
                                if len(marker_offsets) == 1 else None)
    check("Acceptance policy before detailed cases matches fixed checksum",
          acceptance_policy_digest == ACCEPTANCE_POLICY_SHA256,
          {"expected": ACCEPTANCE_POLICY_SHA256, "actual": acceptance_policy_digest})
    acceptance = acceptance_bytes.decode("utf-8")
    case_section_start = acceptance.find("## Detailed cases") if len(marker_offsets) == 1 else -1
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
    trace_sections_by_heading = {
        section.get("heading"): section
        for section in trace.get("sections", [])
        if isinstance(section, dict) and isinstance(section.get("heading"), str)
    }
    citation_owner_errors = []
    for task_id, citations in source_citations_by_task.items():
        for citation_start, citation_end in citations:
            for expected_section in expected_trace_sections:
                if (citation_start <= expected_section["end_line"]
                        and citation_end >= expected_section["start_line"]):
                    section = trace_sections_by_heading.get(expected_section["heading"], {})
                    owners = section.get("tasks", []) if isinstance(section, dict) else []
                    if not isinstance(owners, list) or task_id not in owners:
                        citation_owner_errors.append({
                            "task": task_id,
                            "citation": f"L{citation_start}-L{citation_end}",
                            "section": expected_section["heading"],
                        })
    check("Task citations are represented in traceability", not citation_owner_errors,
          citation_owner_errors)
    trace_doc = (root / "TRACEABILITY.md").read_text(encoding="utf-8")
    actual_trace_table = traceability_table(trace_doc)
    expected_trace_table = render_traceability_table(trace["sections"])
    check("Traceability table matches its manifest", actual_trace_table == expected_trace_table,
          {"expected_rows": len(expected_trace_table),
           "actual_rows": len(actual_trace_table) if actual_trace_table is not None else None})
    bad_trace = [section.get("heading") for section in trace["sections"]
                 if not isinstance(section.get("tasks"), list)
                 or not section["tasks"]
                 or any(not isinstance(task_id, str) or task_id not in task_set
                        for task_id in section["tasks"])]
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
        if cases_file.get("status") != "NOT_RUN":
            bad_status.append("acceptance manifest status")
        bad_assignments = []
        for case in cases:
            rows = case.get("assignment_results")
            if not isinstance(rows, list):
                bad_assignments.append({"case": case["id"], "problem": "assignment_results must be a list"})
                continue
            for row in rows:
                if not isinstance(row, dict):
                    bad_assignments.append({"case": case["id"], "problem": "assignment row must be an object"})
                elif row.get("status") != "NOT_RUN":
                    bad_assignments.append({"case": case["id"], "task": row.get("task_id"),
                                            "status": row.get("status")})
        check("Initial task, case, and assignment statuses claim no execution",
              not bad_status and not bad_assignments,
              {"task_or_case_statuses": bad_status, "assignment_statuses": bad_assignments})
        check("Pack records no runtime tests or delegated reviews",
              graph["subagents_used_for_this_pack"] is False
              and graph["runtime_tests_executed_for_this_pack"] is False)

    actual_check_names = [item["check"] for item in checks]
    expected_check_names = canonical_check_inventory(args.initial)[:-1]
    check_inventory_errors = [] if actual_check_names == list(expected_check_names) else [{
        "expected": list(expected_check_names), "actual": actual_check_names,
    }]
    check(CHECK_INVENTORY_CHECK, not check_inventory_errors, check_inventory_errors)

    result = {"passed": not failures, "initial": args.initial,
              "scope": "Static planning-pack checks only",
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
        if "--json" in sys.argv:
            result = {
                "passed": False,
            "initial": "--initial" in sys.argv,
                "scope": "Static planning-pack checks only",
                "not_checked": ["Product compilation", "Runtime capabilities", "Independent code review",
                                "Full ASD-STE100 dictionary conformity"],
                "counts": {},
                "checks": [{"check": "Planning pack input is valid", "passed": False,
                            "detail": str(error)}],
                "failures": ["Planning pack input is valid"],
            }
            print(json.dumps(result, indent=2))
        else:
            print(f"Pack validation failed: {error}", file=sys.stderr)
        raise SystemExit(2)
