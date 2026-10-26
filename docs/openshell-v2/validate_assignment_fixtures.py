#!/usr/bin/env python3
"""Record causal fixtures for per-task acceptance result validation."""
from __future__ import annotations

import hashlib
import contextlib
import io
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

FIXTURE_TIMEOUT_SECONDS = 30
ORIGINAL_BOARD_TIMEOUT_SECONDS = 30
PROBE_TIMEOUT_SECONDS = 15


class CertifiedInputError(ValueError):
    """Identify a pack path that cannot be read or copied as a certified input."""

    def __init__(self, violations: list[dict[str, Any]]) -> None:
        self.violations = violations
        super().__init__("The certified input path contains a symbolic link or an escaping path.")


def path_component_violations(path: Path) -> list[dict[str, Any]]:
    absolute = Path(os.path.abspath(os.fspath(path)))
    current = Path(absolute.anchor)
    for part in absolute.parts[1:]:
        current = current / part
        try:
            metadata = current.lstat()
        except FileNotFoundError:
            break
        except OSError as error:
            return [{"path": str(current), "reason": "path component cannot be inspected",
                     "detail": str(error)}]
        if stat.S_ISLNK(metadata.st_mode):
            return [{"path": str(current), "reason": "symbolic link in certified path",
                     "target": os.readlink(current)}]
    return []


def certified_input_violations(root: Path, entry_path: Path | None = None) -> list[dict[str, Any]]:
    root_path = Path(os.path.abspath(os.fspath(root)))
    violations = path_component_violations(root_path)
    if entry_path is not None:
        entry_violations = path_component_violations(entry_path)
        for violation in entry_violations:
            if violation not in violations:
                violations.append(violation)
    if violations:
        return violations

    try:
        root_metadata = root_path.lstat()
    except OSError as error:
        return [{"path": str(root_path), "reason": "certified root cannot be inspected",
                 "detail": str(error)}]
    if not stat.S_ISDIR(root_metadata.st_mode):
        return [{"path": str(root_path), "reason": "certified root is not a directory"}]

    root_resolved = root_path.resolve(strict=True)
    if entry_path is not None:
        entry_resolved = Path(os.path.abspath(os.fspath(entry_path))).resolve(strict=True)
        if not entry_resolved.is_relative_to(root_resolved):
            violations.append({"path": str(entry_path),
                               "reason": "entry point resolves outside certified root"})

    def walk_error(error: OSError) -> None:
        violations.append({"path": error.filename or str(root_path),
                           "reason": "certified input directory cannot be inspected",
                           "detail": str(error)})

    for directory, directories, files in os.walk(root_resolved, topdown=True,
                                                  followlinks=False, onerror=walk_error):
        current_directory = Path(directory)
        for name in list(directories):
            child = current_directory / name
            if child.is_symlink():
                violations.append({"path": child.relative_to(root_resolved).as_posix(),
                                   "reason": "symbolic link in certified inputs",
                                   "target": os.readlink(child)})
                directories.remove(name)
                continue
            if name == "__pycache__":
                directories.remove(name)
                continue
            resolved = child.resolve(strict=True)
            if not resolved.is_relative_to(root_resolved):
                violations.append({"path": child.relative_to(root_resolved).as_posix(),
                                   "reason": "certified input resolves outside root",
                                   "resolved_path": str(resolved)})
                directories.remove(name)
        for name in files:
            child = current_directory / name
            metadata = child.lstat()
            if stat.S_ISLNK(metadata.st_mode):
                violations.append({"path": child.relative_to(root_resolved).as_posix(),
                                   "reason": "symbolic link in certified inputs",
                                   "target": os.readlink(child)})
                continue
            if child.suffix == ".pyc":
                continue
            if not stat.S_ISREG(metadata.st_mode):
                violations.append({"path": child.relative_to(root_resolved).as_posix(),
                                   "reason": "certified input is not a regular file"})
                continue
            resolved = child.resolve(strict=True)
            if not resolved.is_relative_to(root_resolved):
                violations.append({"path": child.relative_to(root_resolved).as_posix(),
                                   "reason": "certified input resolves outside root",
                                   "resolved_path": str(resolved)})
    return sorted(violations, key=lambda item: (str(item.get("path")), str(item.get("reason"))))


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot_files(root: Path) -> dict[str, str]:
    if certified_input_violations(root):
        return {}
    root = Path(os.path.abspath(os.fspath(root))).resolve(strict=True)
    snapshot: dict[str, str] = {}
    for directory, directories, files in os.walk(root, topdown=True, followlinks=False):
        current_directory = Path(directory)
        directories[:] = [name for name in directories
                          if name != "__pycache__" and not (current_directory / name).is_symlink()]
        for name in files:
            path = current_directory / name
            if path.suffix == ".pyc" or path.is_symlink():
                continue
            snapshot[path.relative_to(root).as_posix()] = sha256(path)
    return dict(sorted(snapshot.items()))


def input_state(digest: str | None) -> dict[str, str | None]:
    if digest is None:
        return {"state": "missing", "sha256": None}
    return {"state": "present", "sha256": digest}


def snapshot_states(snapshot: dict[str, str]) -> dict[str, dict[str, str | None]]:
    return {path: input_state(digest) for path, digest in snapshot.items()}


def snapshot_changes(before: dict[str, str], after: dict[str, str]) -> dict[str, Any]:
    return {
        path: {"before": input_state(before.get(path)), "after": input_state(after.get(path))}
        for path in sorted(set(before) | set(after))
        if before.get(path) != after.get(path)
    }


def path_state(path: Path) -> dict[str, str | None]:
    if path_component_violations(path):
        return {"state": "unsafe", "sha256": None}
    return input_state(sha256(path) if path.is_file() and not path.is_symlink() else None)


def copy_certified_pack(source: Path, destination: Path) -> None:
    violations = certified_input_violations(source)
    if violations:
        raise CertifiedInputError(violations)
    destination_path_violations = path_component_violations(destination)
    if destination_path_violations:
        raise CertifiedInputError(destination_path_violations)
    shutil.copytree(
        source,
        destination,
        symlinks=True,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )
    copied_violations = certified_input_violations(destination)
    if copied_violations:
        raise CertifiedInputError(copied_violations)


def temporary_directory(prefix: str) -> tempfile.TemporaryDirectory[str]:
    temporary_base = Path(tempfile.gettempdir()).resolve(strict=True)
    base_violations = path_component_violations(temporary_base)
    if base_violations:
        raise CertifiedInputError(base_violations)
    return tempfile.TemporaryDirectory(prefix=prefix, dir=temporary_base)


def output_text(value: str | bytes | None) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return value


def run_with_timeout(command: list[str], timeout_seconds: float,
                     cwd: Path | None = None) -> dict[str, Any]:
    process = subprocess.Popen(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
        return {
            "exit_code": process.returncode,
            "stdout": output_text(stdout),
            "stderr": output_text(stderr),
            "timed_out": False,
            "timeout_seconds": timeout_seconds,
            "timeout_partial_stdout": "",
            "timeout_partial_stderr": "",
            "termination_sent": False,
            "child_reaped": process.returncode is not None,
        }
    except subprocess.TimeoutExpired as error:
        partial_stdout = output_text(error.output)
        partial_stderr = output_text(error.stderr)
        termination_sent = False
        try:
            process.kill()
            termination_sent = True
        except ProcessLookupError:
            pass
        stdout, stderr = process.communicate()
        return {
            "exit_code": process.returncode,
            "stdout": output_text(stdout),
            "stderr": output_text(stderr),
            "timed_out": True,
            "timeout_seconds": timeout_seconds,
            "timeout_partial_stdout": partial_stdout,
            "timeout_partial_stderr": partial_stderr,
            "termination_sent": termination_sent,
            "child_reaped": process.returncode is not None,
        }


ENTRY_FILE = Path(os.path.abspath(__file__))
ROOT_CANDIDATE = ENTRY_FILE.parent
BOOTSTRAP_INPUT_POLICY_VIOLATIONS = certified_input_violations(ROOT_CANDIDATE, ENTRY_FILE)
ROOT = ROOT_CANDIDATE.resolve(strict=True) if not BOOTSTRAP_INPUT_POLICY_VIOLATIONS else ROOT_CANDIDATE
VALIDATOR = ROOT / "validate_pack.py"
sys.dont_write_bytecode = True
SOURCE_SNAPSHOT_BEFORE_VALIDATOR_IMPORT = (
    snapshot_files(ROOT) if not BOOTSTRAP_INPUT_POLICY_VIOLATIONS else {}
)
VALIDATOR_IMPORT_STDOUT = io.StringIO()
VALIDATOR_IMPORT_STDERR = io.StringIO()
VALIDATOR_IMPORT_EXCEPTION: dict[str, str] | None = None
if not BOOTSTRAP_INPUT_POLICY_VIOLATIONS:
    sys.path.insert(0, str(ROOT))
    try:
        with contextlib.redirect_stdout(VALIDATOR_IMPORT_STDOUT), contextlib.redirect_stderr(
            VALIDATOR_IMPORT_STDERR,
        ):
            from validate_pack import (  # noqa: E402
                DuplicateJSONKeyError,
                render_task_card,
                render_task_index,
                render_task_packet,
                strict_json_loads,
            )
    except Exception as error:
        VALIDATOR_IMPORT_EXCEPTION = {"type": type(error).__name__, "message": str(error)}
else:
    VALIDATOR_IMPORT_EXCEPTION = {
        "type": "CertifiedInputError",
        "message": "The fixture runner refused to import from an unsafe source path.",
    }

SOURCE_SNAPSHOT_AFTER_VALIDATOR_IMPORT = (
    snapshot_files(ROOT) if not BOOTSTRAP_INPUT_POLICY_VIOLATIONS else {}
)
VALIDATOR_IMPORT_CHANGES = snapshot_changes(
    SOURCE_SNAPSHOT_BEFORE_VALIDATOR_IMPORT,
    SOURCE_SNAPSHOT_AFTER_VALIDATOR_IMPORT,
)

# Kept independent from validate_pack.py so fixture review detects a missing check.
FIXTURE_ORDINARY_CHECK_NAMES = (
    'Required pack files exist',
    'Manifest schema versions are supported integers',
    'Execution markers are Boolean in every mode.',
    'LIVE PASS and FAIL results require the runtime-execution marker.',
    'C01 source pin table matches all four fixed references',
    'C02 production crate graph and boundary section matches fixed bytes',
    'Machine source pins match fixed references',
    'Parallel agent limit matches the subagent contract',
    'Authoritative concurrency declaration matches graph and subagent limit',
    'Task statuses use documented values',
    'Acceptance case statuses use documented values',
    'Acceptance evidence levels use documented values',
    'Acceptance case IDs map to fixed evidence levels',
    'Acceptance manifest status uses documented values',
    'Acceptance manifest status matches the case summary',
    'Task IDs are unique',
    'Case IDs are unique',
    'Acceptance cases have unique string task owners',
    'Declared task count matches',
    'Declared case count matches',
    'Expected task count',
    'Expected acceptance case count',
    'Task file and test write scopes match canonical paths',
    'Task required artifacts match canonical evidence set and report path',
    'Task completion rules match the fixed evidence contract',
    'Task review rules match the different-subagent and post-merge contract',
    'Task test_cases contain unique string IDs',
    'A13 preserves separate current-graph and final-post-deletion runs',
    'A15 separates observation, adapter, and integrated activation owners',
    'A16 preserves accepted-revision observation and product-presentation owners',
    'A17 preserves observation layers and integrated activation ownership',
    'A86 preserves backend host-action and integrated presentation owners',
    'Release gate protocol matches fixed semantics',
    'Original specification matches its fixed checksum and both manifests',
    'GOAL.md matches its fixed checksum',
    'Task SPEC citations use valid syntax',
    'Task SPEC citation ranges are within SPEC.md',
    'Dependency lists contain unique string task IDs',
    'Dependencies reference other known tasks',
    'Release gate references name T40 and its owned final cases',
    'P04, T40, A123, and A125 mirror the fixed release gate contract',
    'T40 finalization writes route through Integration',
    'A125 verifies conditional patched-tuple approval',
    'P02 bootstrap records, P04 imports, and T12 reruns A05/A06',
    'P04 harness command registration and P05 build wiring scopes agree',
    'Target matrix qualification requests route through Integration',
    'PACK-CHECKS limits release-gate claims to static structure',
    'P04 is listed under the Qualification subagent',
    'Subagent task assignments exactly match graph owners',
    'Review packet uses candidate input and post-merge completion stages',
    'Task progress waits for completed dependencies',
    'Dependency graph has no cycle',
    'Stored ready groups match dependencies without duplicate task IDs',
    'Assignment results have exact unique case owners and valid statuses',
    'Case status matches assignment summary',
    'NOT_STARTED and READY tasks have NOT_RUN assignments',
    'BLOCKED tasks with incomplete dependencies have NOT_RUN assignments',
    'REVIEW tasks have recorded results for every owned assignment',
    'Task test references exist',
    'PASS tasks have passing assigned case results',
    'Every case has a task owner',
    'Case and task ownership maps agree',
    'Task graph fields match TASKS index table',
    'Task graph fields match PLAN task index table',
    'PLAN authored prefix matches its unique task-section boundary',
    'PLAN task headings match the task graph',
    'Task graph fields match PLAN task cards',
    'Task graph fields and canonical preamble match individual task packets',
    'Acceptance detailed-case anchor is unique',
    'Acceptance policy before detailed cases matches fixed checksum',
    'Acceptance case headings have no missing, duplicate, or extra blocks',
    'Acceptance case fields match the acceptance-case manifest',
    'Every specification heading is mapped',
    'Traceability ranges match exact specification sections',
    'Task citations are represented in traceability',
    'Traceability table matches its manifest',
    'Traceability uses known task owners',
    'Checked procedural sentences contain at most 20 whitespace-separated words',
    'Task authored fields match fixed projection',
    'Case authored fields match fixed projection',
    'Graph declarations match fixed projection',
    'Traceability authored sections match fixed projection',
    'CONTRACTS.md matches its fixed checksum',
    'SUBAGENTS.md matches its fixed checksum',
    'PACK-CHECKS matches canonical template with mutable count and result slots',
)
FIXTURE_INITIAL_ONLY_CHECK_NAMES = (
    'Initial task, case, and assignment statuses claim no execution',
    'Pack records no runtime tests or delegated reviews',
)
FIXTURE_CHECK_INVENTORY_CHECK = 'Validator check inventory matches its canonical ordered sequence'


FIXED_FIXTURE_COUNT = 250
FIXED_FIXTURE_NAMES = (
    "isolated subprocess detects idempotent README mutation during validator import",
    "shared PASS summary requires every owner",
    "task PASS with its own assignment results",
    "mixed PASS and NOT_RUN summarizes as NOT_RUN",
    "mixed PASS and BLOCKED summarizes as BLOCKED",
    "FAIL takes precedence over BLOCKED",
    "NOT_STARTED owner cannot have PASS assignment",
    "NOT_STARTED owner cannot have FAIL assignment",
    "NOT_STARTED owner cannot have BLOCKED assignment",
    "READY owner cannot have PASS assignment",
    "READY owner cannot have FAIL assignment",
    "READY owner cannot have BLOCKED assignment",
    "READY owner can retain NOT_RUN assignment",
    "ordinary validation rejects a missing runtime-execution marker",
    "ordinary validation rejects a non-Boolean delegation marker",
    "initial validation rejects non-Boolean execution markers",
    "readiness groups reject an object with a structured input error",
    "readiness groups reject a non-list group with a structured input error",
    "readiness groups reject a non-string task identifier with a structured input error",
    "serial SOURCE work passes without delegation or runtime claims",
    "serial REVIEW work records its own passing assignment",
    "LIVE PASS assignments require a true runtime-execution marker",
    "LIVE PASS assignments are accepted with a true runtime-execution marker",
    "LIVE FAIL assignments require a true runtime-execution marker",
    "LIVE FAIL assignments are accepted with a true runtime-execution marker",
    "LIVE BLOCKED results do not claim runtime execution",
    "a BUILD assignment does not claim LIVE runtime execution",
    "temporary normalization resets only valid true execution markers",
    "temporary normalization does not recreate a missing execution marker",
    "temporary normalization does not repair an invalid execution marker",
    "progressed source with true execution markers passes ordinary validation",
    "BLOCKED owner can retain its passing assignment while T32 waits",
    "upstream PASS cannot close downstream assignment",
    "legacy summary cannot PASS with incomplete assignment",
    "legacy manifest without assignment_results is rejected",
    "malformed assignment row is rejected",
    "undocumented assignment status is rejected",
    "duplicate task assignment is rejected",
    "missing task assignment is rejected",
    "extra task assignment is rejected",
    "duplicate case owner fails when its Markdown block is synchronized",
    "copied pack without validator fails under the real external validator",
    "acceptance prefix mutation violates its fixed checksum",
    "acceptance prefix deletion violates its fixed checksum",
    "initial mode rejects assignment progress",
    "initial mode rejects malformed assignment rows cleanly",
    "temporary normalization changes only P00 and A01 status fields",
    "consistent progressed P00 and A01 board passes ordinary validation",
    "invalid temporary source board with missing A01 assignment is not repaired",
    "T33 waits for T27 checkpoint completion",
    "BLOCKED task and assignment remain blocked without failing validation",
    "FAIL task must wait for completed dependencies",
    "FAIL task remains valid after completed dependencies with NOT_RUN and PASS cases",
    "BLOCKED task may wait on an incomplete dependency with NOT_RUN cases",
    "BLOCKED T32 with incomplete dependencies cannot retain PASS assignments",
    "BLOCKED T32 with incomplete dependencies cannot retain FAIL assignments",
    "BLOCKED T32 with incomplete dependencies cannot retain BLOCKED assignments",
    "BLOCKED T32 with incomplete dependencies keeps NOT_RUN assignments",
    "BLOCKED task with completed dependencies retains prior case results",
    "REVIEW task requires its own result when another owner has PASS",
    "REVIEW task requires every owned assignment to have a result",
    "REVIEW task accepts its own PASS assignment results",
    "REVIEW task accepts its own FAIL assignment results",
    "REVIEW task accepts its own BLOCKED assignment results",
    "REVIEW task without acceptance cases remains valid",
    "task without acceptance cases keeps completion rule",
    "duplicate task case reference is rejected with synchronized task cards",
    "non-string task case reference is rejected before ownership set logic",
    "acceptance manifest root status cannot claim PASS for incomplete cases",
    "acceptance manifest root status rejects undocumented values",
    "manifest summary gives FAIL precedence over BLOCKED",
    "task completion rule cannot diverge from the shared evidence contract",
    "task review rule rejects self-review after synchronized cards",
    "task review rule requires a post-merge integration rerun",
    "canonical review receipt cannot be removed with synchronized cards",
    "report path cannot move from its canonical task receipt directory",
    "upstream task keeps its task-specific change receipt",
    "P02 keeps the required early LIVE records with synchronized cards",
    "P02 cannot omit actual early LIVE execution",
    "T12 cannot reuse the earlier P02 LIVE results",
    "P04 must validate and import early records without closing T12",
    "P04 registration is limited to enum and dispatch through Integration",
    "P05 retains only workspace and build graph registration",
    "P05 must register xtask and test support in the workspace graph",
    "P04 requests the initial target registration from Integration",
    "T35 through T38 retain ordered qualified-target patch requests",
    "P04 cannot receive direct write scope for the central target matrix",
    "T35 cannot write the central target matrix directly",
    "Required target rows remain and unavailable rows stay BLOCKED",
    "release preflight cannot weaken the prior-task prerequisite",
    "release finalization cannot omit A125",
    "release finalization requires independent T40 review",
    "release finalization requires merged-source checks",
    "runtime requalification treats T40 version and package changes as triggers",
    "unjustified runtime impact requires the full applicable LIVE matrix",
    "runtime reruns preserve original case owners and let T40 coordinate",
    "runtime reruns retain early failed blocked and repeat evidence",
    "fresh LIVE results bind final inputs artifacts target run and review",
    "missing stale failed or blocked LIVE reruns prevent finalization",
    "finalization explicitly requires fresh bound LIVE results",
    "preflight does not require future T40 runtime-change outputs",
    "T39 task packet records post-deletion runtime impact and LIVE reruns",
    "T40 card requires fresh affected LIVE results through original operators",
    "T40 version and package changes cannot bypass fresh LIVE reruns",
    "A123 refuses stale or missing post-deletion LIVE evidence",
    "release preflight cannot issue approval",
    "P04 task card retains the fixed protocol implementation requirement",
    "T40 card retains fail-closed finalization conditions",
    "T40 requires actual results for A123 through A125",
    "A123 keeps prerequisite refusal assertions and finalization contract",
    "pack report cannot claim runtime finalizer verification",
    "review packet requires candidate review before merge",
    "P04 remains owned by the Qualification subagent",
    "cited task is required in every intersecting section even when mapped elsewhere",
    "malformed task SPEC citation syntax is rejected",
    "task SPEC citation beyond the document is rejected",
    "malformed task manifest returns structured input error with exit two",
    "task-graph.json rejects a null tasks record before owner evaluation",
    "acceptance-cases.json rejects a null cases record before owner evaluation",
    "traceability.json rejects a null sections record before owner evaluation",
    "ordinary baseline report contains the complete ordered check inventory",
    "initial baseline report contains ordinary and initial-only check inventories",
    "manifest owns the exact recursive Markdown task-packet set",
    "graph root classification rejects a release-approval value",
    "fixture oracle detects removal of task-count implementation and inventory entry",
    "canonical report rejects nonempty counts that differ from tested inputs",
    "canonical report rejects nonempty all-zero counts",
    "missing validator short form rejects fabricated README detail",
    "missing tasks short form rejects fabricated cases detail",
    "empty validator stdout is rejected",
    "invalid JSON validator stdout is rejected",
    "non-object validator JSON is rejected",
    "validator report with missing fields is rejected",
    "validator report with wrong field types is rejected",
    "validator report with inconsistent counts is rejected",
    "contradictory passed and failures fields are rejected",
    "check list inconsistent with failures is rejected even at exit zero",
    "malformed check records are rejected",
    "duplicate check names are rejected",
    "validator report with mismatched scope is rejected",
    "report exit code must agree with passed even at exit zero",
    "structured input-error report with contradictory exit one is rejected",
    "truncated passing report is rejected",
    "truncated failing report is rejected even when expected failure appears",
    "report with a missing middle check is rejected",
    "report with reordered checks is rejected",
    "report with an extra check is rejected",
    "initial report missing an initial-only tail check is rejected",
    "report initial flag must match the validator invocation",
    "unexpected extra failure name does not match exact expectation",
    "Integration role row lists every graph-owned task",
    "role assignment table matches every graph owner, not only Integration",
    "T40 cannot bypass Integration for shared finalization writes",
    "release finalization keeps shared writes with Integration",
    "release finalization requires conditional approval for supported patched tuples",
    "unpatched supported tuples do not require patch-approval records",
    "A125 verifies conditional maintainer approval for each patched tuple",
    "C01 source table rejects a missing historical reference",
    "C01 source table rejects a duplicate reference",
    "C01 source table rejects an extra reference",
    "C01 source table rejects an altered release version",
    "C01 source table rejects an altered release comparator commit",
    "C01 source table rejects an altered historical commit",
    "C02 digest rejects a deleted production crate row",
    "C02 digest rejects an altered crate responsibility",
    "C02 digest rejects a weakened dependency boundary",
    "C02 digest rejects a duplicate section anchor",
    "C02 digest rejects a missing section anchor",
    "C02 digest rejects a missing end boundary",
    "C02 digest rejects a duplicate end boundary",
    "individual task packet rejects a missing instruction preamble",
    "missing task packet input identifies P00",
    "individual task packet rejects a mismatched packet ID",
    "individual task packet rejects a weakened file lease instruction",
    "individual task packet rejects a weakened dependency instruction",
    "individual task packet rejects weakened change authorization",
    "individual task packet rejects an extra conflicting instruction",
    "exact manifest packet set rejects nested archived Markdown",
    "exact manifest packet set rejects a missing canonical Markdown packet",
    "task file path rejects relocated locations",
    "task file path rejects traversing locations",
    "task file path rejects absolute locations",
    "task file path rejects colliding locations",
    "task test-write scope rejects relocated locations",
    "task test-write scope rejects traversing locations",
    "task test-write scope rejects absolute locations",
    "task test-write scope rejects colliding locations",
    "empty root dependency list remains valid",
    "dependency list rejects duplicate IDs before DAG operations",
    "dependency list rejects non-string entries before DAG operations",
    "dependency list rejects a non-list value before DAG operations",
    "unknown dependency retains reference and readiness diagnostics",
    "self dependency retains reference and cycle diagnostics",
    "dependency cycle retains cycle and readiness diagnostics",
    "valid evidence-level downgrade does not reclassify A11",
    "valid evidence-level downgrade does not reclassify A50",
    "GOAL.md rejects truncation",
    "GOAL.md is a required input",
    "GOAL.md rejects a conflicting execution-control clause",
    "authoritative concurrency declaration rejects a changed limit",
    "authoritative concurrency declaration cannot be missing",
    "authoritative concurrency declaration cannot be duplicated",
    "task graph rejects a duplicate top-level JSON key",
    "task graph rejects a duplicate nested JSON key",
    "acceptance rejects a duplicate top-level JSON key",
    "acceptance rejects a duplicate nested JSON key",
    "traceability rejects a duplicate top-level JSON key",
    "traceability rejects a duplicate nested JSON key",
    "task-graph.json rejects nonstandard JSON constant NaN",
    "acceptance-cases.json rejects nonstandard JSON constant Infinity",
    "traceability.json rejects nonstandard JSON constant -Infinity",
    "fixture report parser rejects duplicate top-level JSON keys",
    "fixture report parser rejects duplicate nested JSON keys",
    "fixture report parser rejects nonstandard JSON constant NaN",
    "fixture report parser rejects nonstandard JSON constant Infinity",
    "fixture report parser rejects nonstandard JSON constant -Infinity",
    "P05 cannot defer its A13 run to the final graph",
    "T39 cannot omit its final post-deletion A13 rerun",
    "A13 cannot omit P05 current-graph Cargo metadata",
    "A13 cannot omit T39 final-graph Cargo metadata",
    "A15 cannot omit the integrated T23 owner",
    "T23 must qualify the full A15 StartSession barrier",
    "U00 cannot collapse A15 and A16 into integrated behavior",
    "T13 cannot collapse A15 and A16 into product behavior",
    "U00 must bind A16's replacement source to its accepted revision",
    "T13 must bind A16's replacement source to its accepted revision",
    "A16 cannot omit the T29 product-presentation owner",
    "T29 must display A16's accepted replacement source and revision",
    "A17 cannot omit integrated T23 ownership",
    "T23 retains integrated StartSession and re-admission for A17",
    "A16 loads the global policy before querying and displaying it",
    "Synchronized A22 prose cannot weaken owner-specific assertions",
    "C06 credential-authority fail-open cannot alter the complete contracts",
    "T23 cannot omit its T16 dependency after all graph mirrors are recomputed",
    "T23 cannot add an extra dependency after all graph mirrors are recomputed",
    "Worker task cannot claim shared root Cargo files",
    "Extra contradictory task scope cannot override the authored lease",
    "T23 cannot rebase its citation to SPEC L1 with synchronized traceability",
    "SUBAGENTS shared-file authority cannot be weakened",
    "PACK-CHECKS rejects conflicting prose outside its result slots",
    "PACK-CHECKS mutable count and result slots remain editable",
    "PACK-CHECKS check results reject release-approval prose",
    "PACK-CHECKS observations reject release-approval prose",
    "fixture runner detects input and validator changes during execution",
    "PLAN prefix policy mutation violates its fixed digest",
    "PLAN task-index boundary cannot be missing",
    "PLAN task-index boundary cannot be duplicated",
    "Synchronized PLAN concurrency-policy weakening violates the fixed prefix",
    "A86 cannot omit T29 integrated presentation ownership or weaken its assertions",
    "T40 cannot claim direct writes to shared target and compatibility matrices",
)


def fixture_inventory_report(observed_names: list[Any]) -> dict[str, Any]:
    expected_names = list(FIXED_FIXTURE_NAMES)
    actual_names = [name for name in observed_names if isinstance(name, str)]
    invalid_observed_names = [
        {"index": index, "value": name}
        for index, name in enumerate(observed_names)
        if not isinstance(name, str)
    ]
    expected_duplicates = sorted({
        name for name in expected_names if expected_names.count(name) > 1
    })
    actual_duplicates = sorted({
        name for name in actual_names if actual_names.count(name) > 1
    })
    expected_set = set(expected_names)
    actual_set = set(actual_names)
    errors = []
    if len(expected_names) != FIXED_FIXTURE_COUNT:
        errors.append("The fixed fixture-name inventory has the wrong number of entries.")
    if expected_duplicates:
        errors.append("The fixed fixture-name inventory contains duplicate names.")
    if len(observed_names) != FIXED_FIXTURE_COUNT:
        errors.append("The executed fixture count does not match the fixed count.")
    if invalid_observed_names:
        errors.append("The executed fixture inventory contains non-string names.")
    if actual_duplicates:
        errors.append("The executed fixture inventory contains duplicate names.")
    missing_names = sorted(expected_set - actual_set)
    unexpected_names = sorted(actual_set - expected_set)
    if missing_names:
        errors.append("The executed fixture inventory is missing fixed scenarios.")
    if unexpected_names:
        errors.append("The executed fixture inventory contains unexpected scenarios.")
    return {
        "passed": not errors,
        "expected_count": FIXED_FIXTURE_COUNT,
        "fixed_inventory_count": len(expected_names),
        "observed_count": len(observed_names),
        "expected_duplicate_names": expected_duplicates,
        "observed_duplicate_names": actual_duplicates,
        "invalid_observed_names": invalid_observed_names,
        "missing_names": missing_names,
        "unexpected_names": unexpected_names,
        "errors": errors,
    }



def fixture_check_inventory(initial: bool = False) -> tuple[str, ...]:
    initial_checks = FIXTURE_INITIAL_ONLY_CHECK_NAMES if initial else ()
    return (*FIXTURE_ORDINARY_CHECK_NAMES, *initial_checks, FIXTURE_CHECK_INVENTORY_CHECK)


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
    input_policy_violations = certified_input_violations(pack)
    if input_policy_violations:
        return changes, [
            f"certified input paths are unsafe before normalization: {input_policy_violations}"
        ]

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
            for field in ("subagents_used_for_this_pack", "runtime_tests_executed_for_this_pack"):
                value = data.get(field)
                if type(value) is bool and value is True:
                    changes.append({"path": relative_path, "field": field,
                                    "before": True, "after": False})
                    data[field] = False
                    changed = True
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
        if any(not isinstance(case, dict) for case in cases):
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


def set_execution_markers(pack: Path, **values: Any) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    graph.update(values)
    write_json(graph_path, graph)


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


def sync_task_indexes(pack: Path) -> None:
    graph = read_json(pack / "task-graph.json")
    rendered = render_task_index(graph["tasks"])
    header = "| ID | Task | Subagent | Dependencies |"
    for relative_path in ("TASKS.md", "PLAN.md"):
        path = pack / relative_path
        lines = path.read_text(encoding="utf-8").splitlines()
        matches = [index for index, line in enumerate(lines) if line == header]
        if len(matches) != 1:
            raise ValueError(f"Expected one task index table in {relative_path}")
        start = matches[0]
        end = start + 1
        while end < len(lines) and lines[end].startswith("|"):
            end += 1
        lines[start:end] = rendered
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def recompute_ready_groups(pack: Path) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    tasks = graph["tasks"]
    done: set[str] = set()
    levels = []
    while len(done) < len(tasks):
        ready = [task["id"] for task in tasks
                 if task["id"] not in done and set(task["depends_on"]).issubset(done)]
        if not ready:
            raise ValueError("Cannot derive readiness groups for a cyclic fixture graph")
        levels.append(ready)
        done.update(ready)
    graph["topological_ready_groups"] = levels
    write_json(graph_path, graph)


def all_pass(pack: Path) -> None:
    graph_path = pack / "task-graph.json"
    graph = read_json(graph_path)
    for task in graph["tasks"]:
        task["status"] = "PASS"
    graph["runtime_tests_executed_for_this_pack"] = True
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
    except ValueError as error:
        return None, [str(error)]
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
        if check_names != list(fixture_check_inventory(initial)):
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
    with temporary_directory(prefix="jackin-assignment-") as temp_dir:
        temporary_root = Path(temp_dir)
        pack = temporary_root / "pack"
        source_policy_violations = certified_input_violations(ROOT, ENTRY_FILE)
        if source_policy_violations:
            return {
                "scenario": name,
                "passed": False,
                "copy_performed": False,
                "source_input_policy_violations": source_policy_violations,
                "validator_executed": False,
                "exit_code": None,
            }
        try:
            copy_certified_pack(ROOT, pack)
        except CertifiedInputError as error:
            return {
                "scenario": name,
                "passed": False,
                "copy_performed": pack.exists(),
                "source_input_policy_violations": error.violations,
                "validator_executed": False,
                "exit_code": None,
            }
        source_inputs = snapshot_files(pack)
        if pre_normalization_mutation is not None:
            pre_normalization_mutation(pack)
        pre_normalization_policy_violations = certified_input_violations(pack)
        if pre_normalization_policy_violations:
            return {
                "scenario": name,
                "passed": False,
                "copy_performed": True,
                "pre_normalization_input_policy_violations": pre_normalization_policy_violations,
                "validator_executed": False,
                "exit_code": None,
            }
        pre_normalization_inputs = snapshot_files(pack)
        normalization_diff, normalization_errors = normalize_status_fields(pack)
        normalized_input_policy_violations = certified_input_violations(pack)
        if normalized_input_policy_violations:
            return {
                "scenario": name,
                "passed": False,
                "copy_performed": True,
                "normalized_input_policy_violations": normalized_input_policy_violations,
                "validator_executed": False,
                "exit_code": None,
            }
        normalized_inputs = snapshot_files(pack)
        mutate(pack)
        fixture_input_policy_violations = certified_input_violations(pack)
        if fixture_input_policy_violations:
            return {
                "scenario": name,
                "passed": False,
                "copy_performed": True,
                "fixture_input_policy_violations": fixture_input_policy_violations,
                "validator_executed": False,
                "exit_code": None,
            }
        inputs_after = snapshot_files(pack)
        expected_counts = derive_expected_counts(pack)
        validator_copy = pack / "validate_pack.py"
        if fake_stdout is not None:
            executable = fake_validator(temporary_root, fake_stdout, fake_stderr, expected_exit)
        else:
            executable = validator_executable or validator_copy
        command = [sys.executable, str(executable), "--root", str(pack), "--json"]
        if initial:
            command.append("--initial")
        execution_inputs_before = snapshot_files(pack)
        execution_input_policy_before = certified_input_violations(pack)
        executable_policy_violations = path_component_violations(executable)
        if execution_input_policy_before or executable_policy_violations:
            return {
                "scenario": name,
                "passed": False,
                "copy_performed": True,
                "execution_input_policy_before": execution_input_policy_before,
                "validator_executable_policy_violations": executable_policy_violations,
                "validator_executed": False,
                "exit_code": None,
            }
        executable_before = path_state(executable)
        validator_copy_before = path_state(validator_copy)
        process = run_with_timeout(command, FIXTURE_TIMEOUT_SECONDS)
        execution_inputs_after = snapshot_files(pack)
        execution_input_policy_after = certified_input_violations(pack)
        executable_after = path_state(executable)
        validator_copy_after = path_state(validator_copy)
        if process["timed_out"]:
            report = None
            report_errors = [
                f"validator subprocess timed out after {process['timeout_seconds']} seconds"
            ]
        elif fixture_input_policy_violations or execution_input_policy_before or executable_policy_violations:
            report = None
            report_errors = ["fixture inputs or validator executable violate the certified-input policy"]
        else:
            report, report_errors = inspect_report(
                process["stdout"], process["exit_code"], initial=initial,
            )
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
            not process["timed_out"]
            and not fixture_input_policy_violations
            and not execution_input_policy_before
            and not execution_input_policy_after
            and not executable_policy_violations
            and process["exit_code"] == expected_exit
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
            "exit_code": process["exit_code"],
            "expected_exit_code": expected_exit,
            "timed_out": process["timed_out"],
            "timeout_seconds": process["timeout_seconds"],
            "timeout_partial_stdout": process["timeout_partial_stdout"],
            "timeout_partial_stderr": process["timeout_partial_stderr"],
            "termination_sent": process["termination_sent"],
            "child_reaped": process["child_reaped"],
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
            "source_input_policy_violations": source_policy_violations,
            "fixture_input_policy_violations": fixture_input_policy_violations,
            "execution_input_policy_before": execution_input_policy_before,
            "execution_input_policy_after": execution_input_policy_after,
            "validator_executable_policy_violations": executable_policy_violations,
            "passed": passed,
            "stdout": process["stdout"],
            "stderr": process["stderr"],
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
    input_policy_before = certified_input_violations(ROOT, ENTRY_FILE)
    executable_policy_before = path_component_violations(VALIDATOR)
    if input_policy_before or executable_policy_before:
        return {
            "passed": False,
            "argv": command,
            "exit_code": None,
            "timed_out": False,
            "timeout_seconds": ORIGINAL_BOARD_TIMEOUT_SECONDS,
            "timeout_partial_stdout": "",
            "timeout_partial_stderr": "",
            "termination_sent": False,
            "child_reaped": False,
            "input_policy_violations_before": input_policy_before,
            "input_policy_violations_after": input_policy_before,
            "validator_executable_policy_violations": executable_policy_before,
            "report_errors": ["original-board subprocess was refused before unsafe input access"],
            "execution_errors": ["original-board inputs violate the certified-input policy"],
            "stdout": "",
            "stderr": "",
        }
    inputs_before = snapshot_files(ROOT)
    executable_before = path_state(VALIDATOR)
    process = run_with_timeout(command, ORIGINAL_BOARD_TIMEOUT_SECONDS)
    input_policy_after = certified_input_violations(ROOT, ENTRY_FILE)
    inputs_after = snapshot_files(ROOT)
    executable_after = path_state(VALIDATOR)
    if process["timed_out"]:
        report = None
        report_errors = [
            f"original-board subprocess timed out after {process['timeout_seconds']} seconds"
        ]
    elif input_policy_before or input_policy_after or path_component_violations(VALIDATOR):
        report = None
        report_errors = ["original-board inputs violate the certified-input policy"]
    else:
        report, report_errors = inspect_report(
            process["stdout"], process["exit_code"], initial=False,
        )
    expected_counts = derive_expected_counts(ROOT) if report is not None else None
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
    if input_policy_before:
        execution_errors.append("original board inputs were unsafe before ordinary validation")
    if input_policy_after:
        execution_errors.append("original board inputs became unsafe during ordinary validation")
    if executable_before != executable_after:
        execution_errors.append("original validator executable changed during ordinary validation")
    if process["timed_out"]:
        execution_errors.append(
            f"original-board subprocess timed out after {process['timeout_seconds']} seconds"
        )
    return {
        "passed": (
            not process["timed_out"]
            and not input_policy_before
            and not input_policy_after
            and process["exit_code"] == 0
            and not report_errors
            and isinstance(report, dict)
            and report.get("passed") is True
            and not execution_errors
        ),
        "argv": command,
        "exit_code": process["exit_code"],
        "timed_out": process["timed_out"],
        "timeout_seconds": process["timeout_seconds"],
        "timeout_partial_stdout": process["timeout_partial_stdout"],
        "timeout_partial_stderr": process["timeout_partial_stderr"],
        "termination_sent": process["termination_sent"],
        "child_reaped": process["child_reaped"],
        "input_policy_violations_before": input_policy_before,
        "input_policy_violations_after": input_policy_after,
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
        "stdout": process["stdout"],
        "stderr": process["stderr"],
    }


def validator_import_receipt() -> dict[str, Any]:
    captured_stdout = VALIDATOR_IMPORT_STDOUT.getvalue()
    captured_stderr = VALIDATOR_IMPORT_STDERR.getvalue()
    return {
        "passed": (
            not BOOTSTRAP_INPUT_POLICY_VIOLATIONS
            and VALIDATOR_IMPORT_EXCEPTION is None
            and not captured_stdout
            and not captured_stderr
            and not VALIDATOR_IMPORT_CHANGES
        ),
        "captured_stdout": captured_stdout,
        "captured_stderr": captured_stderr,
        "exception": VALIDATOR_IMPORT_EXCEPTION,
        "source_path_policy_violations": BOOTSTRAP_INPUT_POLICY_VIOLATIONS,
        "source_sha256_before_import": snapshot_states(SOURCE_SNAPSHOT_BEFORE_VALIDATOR_IMPORT),
        "source_sha256_after_import": snapshot_states(SOURCE_SNAPSHOT_AFTER_VALIDATOR_IMPORT),
        "source_changes_during_import": VALIDATOR_IMPORT_CHANGES,
    }


def import_time_readme_mutation_probe() -> dict[str, Any]:
    with temporary_directory(prefix="jackin-import-mutation-") as temp_dir:
        pack = Path(temp_dir) / "pack"
        copy_certified_pack(ROOT, pack)
        validator = pack / "validate_pack.py"
        runner = pack / "validate_assignment_fixtures.py"
        readme = pack / "README.md"
        initial_inputs = snapshot_files(pack)
        validator_before = sha256(validator)
        runner_before = sha256(runner)
        readme_before = sha256(readme)
        with validator.open("a", encoding="utf-8") as source:
            source.write(
                "\nfrom pathlib import Path as _ImportProbePath\n"
                "(_ImportProbePath(__file__).resolve().parent / 'README.md').write_text("
                "'idempotent import-time mutation\\n', encoding='utf-8')\n"
            )
        validator_mutated = sha256(validator)
        command = [
            sys.executable,
            "-c",
            "import importlib.util,json,sys; "
            "spec=importlib.util.spec_from_file_location('fixture_probe',sys.argv[1]); "
            "module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); "
            "print(json.dumps({'before':module.SOURCE_SNAPSHOT_BEFORE_VALIDATOR_IMPORT.get('README.md'),"
            "'after':module.SOURCE_SNAPSHOT_AFTER_VALIDATOR_IMPORT.get('README.md'),"
            "'changes':module.VALIDATOR_IMPORT_CHANGES}))",
            str(runner),
        ]
        process = run_with_timeout(command, PROBE_TIMEOUT_SECONDS, cwd=pack)
        try:
            observed = json.loads(process["stdout"]) if not process["timed_out"] else None
        except json.JSONDecodeError:
            observed = None
        after_inputs = snapshot_files(pack)
        expected_readme_after = sha256(readme)
        detected = observed.get("changes", {}).get("README.md") if isinstance(observed, dict) else None
        passed = (
            not process["timed_out"]
            and process["exit_code"] == 0
            and isinstance(observed, dict)
            and observed.get("before") == readme_before
            and observed.get("after") == expected_readme_after
            and readme_before != expected_readme_after
            and isinstance(detected, dict)
            and detected.get("before") == input_state(readme_before)
            and detected.get("after") == input_state(expected_readme_after)
        )
        return {
            "scenario": "isolated subprocess detects idempotent README mutation during validator import",
            "passed": passed,
            "argv": command,
            "exit_code": process["exit_code"],
            "timed_out": process["timed_out"],
            "timeout_seconds": process["timeout_seconds"],
            "timeout_partial_stdout": process["timeout_partial_stdout"],
            "timeout_partial_stderr": process["timeout_partial_stderr"],
            "termination_sent": process["termination_sent"],
            "child_reaped": process["child_reaped"],
            "stdout": process["stdout"],
            "stderr": process["stderr"],
            "input_path_policy_violations_before": certified_input_violations(pack),
            "fixture_source_sha256": runner_before,
            "validator_executable_before_mutation_sha256": validator_before,
            "validator_executable_after_mutation_sha256": validator_mutated,
            "tested_inputs_before": snapshot_states(initial_inputs),
            "tested_inputs_after": snapshot_states(after_inputs),
            "input_changes": snapshot_changes(initial_inputs, after_inputs),
            "readme_before_sha256": readme_before,
            "readme_after_sha256": expected_readme_after,
            "detected_import_changes": observed.get("changes") if isinstance(observed, dict) else None,
        }


def fixture_inventory_control_probe(mode: str) -> dict[str, Any]:
    scenario_names = list(FIXED_FIXTURE_NAMES)
    target_name = FIXED_FIXTURE_NAMES[0]
    if mode == "missing":
        scenario_names.remove(target_name)
    elif mode == "duplicate":
        scenario_names.append(target_name)
    elif mode == "unexpected":
        scenario_names.append("An undeclared fixture scenario was executed.")
    else:
        raise ValueError(f"Unsupported fixture inventory probe mode: {mode}")

    report = fixture_inventory_report(scenario_names)
    expected_failure = {
        "missing": target_name in report["missing_names"],
        "duplicate": target_name in report["observed_duplicate_names"],
        "unexpected": "An undeclared fixture scenario was executed." in report["unexpected_names"],
    }[mode]
    return {
        "scenario": (
            "The fixture inventory rejects an unexpected scenario entry."
            if mode == "unexpected"
            else f"The fixture inventory rejects a {mode} scenario entry."
        ),
        "passed": expected_failure and not report["passed"],
        "mode": mode,
        "report": report,
    }


def certified_input_security_probe() -> dict[str, Any]:
    with temporary_directory(prefix="jackin-input-custody-") as temp_dir:
        temporary_root = Path(temp_dir)
        source_policy = certified_input_violations(ROOT, ENTRY_FILE)
        original_hashes = snapshot_files(ROOT)
        self_contained_pack = temporary_root / "self-contained-pack"
        copy_certified_pack(ROOT, self_contained_pack)
        copied_hashes = snapshot_files(self_contained_pack)
        self_contained_policy = certified_input_violations(self_contained_pack)

        external_target = temporary_root / "external-target.txt"
        external_target.write_text("This controlled fixture target must remain unchanged.\n",
                                   encoding="utf-8")
        external_target_sha256_before = sha256(external_target)
        external_link = self_contained_pack / "external-target-link.md"
        os.symlink(external_target, external_link)
        external_link_violations = certified_input_violations(self_contained_pack)
        external_copy = temporary_root / "copy-that-must-not-exist"
        try:
            copy_certified_pack(self_contained_pack, external_copy)
            external_copy_refused = False
            external_copy_error: list[dict[str, Any]] = []
        except CertifiedInputError as error:
            external_copy_refused = True
            external_copy_error = error.violations
        external_target_sha256_after = sha256(external_target)

        physical_parent = temporary_root / "physical-parent"
        physical_parent.mkdir()
        ancestor_pack = physical_parent / "pack"
        copy_certified_pack(ROOT, ancestor_pack)
        ancestor_alias = temporary_root / "ancestor-link"
        os.symlink(physical_parent, ancestor_alias, target_is_directory=True)
        aliased_pack = ancestor_alias / "pack"
        ancestor_violations = certified_input_violations(aliased_pack)
        ancestor_copy = temporary_root / "ancestor-copy-that-must-not-exist"
        try:
            copy_certified_pack(aliased_pack, ancestor_copy)
            ancestor_copy_refused = False
            ancestor_copy_error: list[dict[str, Any]] = []
        except CertifiedInputError as error:
            ancestor_copy_refused = True
            ancestor_copy_error = error.violations

        external_link_detected = any(
            item.get("path") == "external-target-link.md"
            and item.get("reason") == "symbolic link in certified inputs"
            and item.get("target") == str(external_target)
            for item in external_link_violations
        )
        ancestor_link_detected = any(
            item.get("reason") == "symbolic link in certified path"
            and item.get("path") == str(ancestor_alias)
            for item in ancestor_violations
        )
        passed = (
            not source_policy
            and original_hashes == copied_hashes
            and not self_contained_policy
            and external_link_detected
            and external_copy_refused
            and not external_copy.exists()
            and external_copy_error == external_link_violations
            and external_target_sha256_before == external_target_sha256_after
            and ancestor_link_detected
            and ancestor_copy_refused
            and not ancestor_copy.exists()
            and ancestor_copy_error == ancestor_violations
        )
        return {
            "scenario": "The fixture runner refuses external and ancestor symlinks before copying.",
            "passed": passed,
            "source_input_policy_violations": source_policy,
            "self_contained_source_sha256": original_hashes,
            "self_contained_copy_sha256": copied_hashes,
            "self_contained_copy_policy_violations": self_contained_policy,
            "external_link_violations": external_link_violations,
            "external_copy_refused": external_copy_refused,
            "external_copy_error": external_copy_error,
            "external_copy_created": external_copy.exists(),
            "external_target_sha256_before": external_target_sha256_before,
            "external_target_sha256_after": external_target_sha256_after,
            "ancestor_link_violations": ancestor_violations,
            "ancestor_copy_refused": ancestor_copy_refused,
            "ancestor_copy_error": ancestor_copy_error,
            "ancestor_copy_created": ancestor_copy.exists(),
        }


def validator_import_probe(mode: str) -> dict[str, Any]:
    with temporary_directory(prefix="jackin-import-capture-") as temp_dir:
        temporary_root = Path(temp_dir)
        pack = temporary_root / "pack"
        copy_certified_pack(ROOT, pack)
        validator = pack / "validate_pack.py"
        runner = pack / "validate_assignment_fixtures.py"
        validator_before = path_state(validator)
        runner_before = path_state(runner)
        inputs_before = snapshot_files(pack)
        if mode == "stdout":
            with validator.open("a", encoding="utf-8") as source:
                source.write("\nprint('Unexpected validator import output.')\n")
            command = [sys.executable, str(runner)]
        elif mode == "stderr":
            with validator.open("a", encoding="utf-8") as source:
                source.write("\nimport sys\nsys.stderr.write('Unexpected validator import error output.\\n')\n")
            command = [sys.executable, str(runner)]
        elif mode == "exception":
            with validator.open("a", encoding="utf-8") as source:
                source.write("\nraise RuntimeError('The validator import failed in this fixture.')\n")
            command = [sys.executable, str(runner)]
        elif mode == "quiet":
            import_probe = (
                "import importlib.util,json,sys; "
                "spec=importlib.util.spec_from_file_location('quiet_probe',sys.argv[1]); "
                "module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); "
                "print(json.dumps({'captured_stdout':module.VALIDATOR_IMPORT_STDOUT.getvalue(),"
                "'captured_stderr':module.VALIDATOR_IMPORT_STDERR.getvalue(),"
                "'exception':module.VALIDATOR_IMPORT_EXCEPTION,"
                "'source_changes':module.VALIDATOR_IMPORT_CHANGES}))"
            )
            command = [sys.executable, "-c", import_probe, str(runner)]
        else:
            raise ValueError(f"Unsupported validator import probe mode: {mode}")

        validator_after_mutation = path_state(validator)
        process = run_with_timeout(command, PROBE_TIMEOUT_SECONDS, cwd=pack)
        inputs_after = snapshot_files(pack)
        validator_after = path_state(validator)
        runner_after = path_state(runner)
        try:
            observed = json.loads(process["stdout"]) if not process["timed_out"] else None
        except json.JSONDecodeError:
            observed = None

        if mode == "quiet":
            passed = (
                not process["timed_out"]
                and process["exit_code"] == 0
                and isinstance(observed, dict)
                and observed == {
                    "captured_stdout": "",
                    "captured_stderr": "",
                    "exception": None,
                    "source_changes": {},
                }
                and not process["stderr"]
            )
        else:
            import_receipt = observed.get("validator_import") if isinstance(observed, dict) else None
            expected_stdout = "Unexpected validator import output.\n" if mode == "stdout" else ""
            expected_stderr = (
                "Unexpected validator import error output.\n" if mode == "stderr" else ""
            )
            expected_exception = (
                {"type": "RuntimeError",
                 "message": "The validator import failed in this fixture."}
                if mode == "exception" else None
            )
            passed = (
                not process["timed_out"]
                and process["exit_code"] == 1
                and isinstance(observed, dict)
                and observed.get("passed") is False
                and isinstance(import_receipt, dict)
                and import_receipt.get("captured_stdout") == expected_stdout
                and import_receipt.get("captured_stderr") == expected_stderr
                and import_receipt.get("exception") == expected_exception
                and not process["stderr"]
            )
        return {
            "scenario": f"The runner captures validator import {mode} without leaking output.",
            "passed": passed,
            "argv": command,
            "exit_code": process["exit_code"],
            "timed_out": process["timed_out"],
            "timeout_seconds": process["timeout_seconds"],
            "timeout_partial_stdout": process["timeout_partial_stdout"],
            "timeout_partial_stderr": process["timeout_partial_stderr"],
            "termination_sent": process["termination_sent"],
            "child_reaped": process["child_reaped"],
            "stdout": process["stdout"],
            "stderr": process["stderr"],
            "observed_json": observed,
            "validator_sha256_before": validator_before,
            "validator_sha256_after_mutation": validator_after_mutation,
            "validator_sha256_after_process": validator_after,
            "runner_sha256_before": runner_before,
            "runner_sha256_after": runner_after,
            "input_sha256_before": snapshot_states(inputs_before),
            "input_sha256_after": snapshot_states(inputs_after),
            "input_changes": snapshot_changes(inputs_before, inputs_after),
        }


def bounded_timeout_probe() -> dict[str, Any]:
    with temporary_directory(prefix="jackin-timeout-probe-") as temp_dir:
        temporary_root = Path(temp_dir)
        sleeper = temporary_root / "bounded-sleeper.py"
        sleeper.write_text(
            "import sys\n"
            "import time\n"
            "print('Partial standard output.', flush=True)\n"
            "print('Partial standard error.', file=sys.stderr, flush=True)\n"
            "time.sleep(30)\n",
            encoding="utf-8",
        )
        input_before = snapshot_files(temporary_root)
        executable_before = path_state(sleeper)
        process = run_with_timeout([sys.executable, str(sleeper)], 0.25, cwd=temporary_root)
        input_after = snapshot_files(temporary_root)
        executable_after = path_state(sleeper)
        passed = (
            process["timed_out"]
            and process["exit_code"] is not None
            and process["termination_sent"]
            and process["child_reaped"]
            and "Partial standard output." in process["timeout_partial_stdout"]
            and "Partial standard error." in process["timeout_partial_stderr"]
            and input_before == input_after
            and executable_before == executable_after
        )
        return {
            "scenario": "The bounded sleeper retains partial output and reaps its timed-out child.",
            "passed": passed,
            "argv": [sys.executable, str(sleeper)],
            "timeout_seconds": process["timeout_seconds"],
            "timed_out": process["timed_out"],
            "exit_code": process["exit_code"],
            "timeout_partial_stdout": process["timeout_partial_stdout"],
            "timeout_partial_stderr": process["timeout_partial_stderr"],
            "stdout": process["stdout"],
            "stderr": process["stderr"],
            "termination_sent": process["termination_sent"],
            "child_reaped": process["child_reaped"],
            "input_sha256_before": snapshot_states(input_before),
            "input_sha256_after": snapshot_states(input_after),
            "input_changes": snapshot_changes(input_before, input_after),
            "executable_before": executable_before,
            "executable_after": executable_after,
        }


def main() -> int:
    import_receipt = validator_import_receipt()
    if not import_receipt["passed"]:
        policy_errors = BOOTSTRAP_INPUT_POLICY_VIOLATIONS
        failure_payload = {
            "passed": False,
            "scope": "Static validator fixtures only. No product or runtime tests are performed.",
            "validator_import": import_receipt,
            "source_input_policy_violations": policy_errors,
            "fixture_inventory": {
                "passed": False,
                "expected_count": FIXED_FIXTURE_COUNT,
                "observed_count": 0,
                "errors": ["The runner refused to execute fixtures after validator import failure."],
            },
            "runner_checks": [],
            "fixtures": [],
            "original_board": {
                "passed": False,
                "validator_not_started": True,
                "reason": "The runner refused to continue after an unsafe or noisy validator import.",
            },
        }
        print(json.dumps(failure_payload, indent=2, ensure_ascii=False))
        return 1

    receipts: list[dict[str, Any]] = []
    original_source_before = SOURCE_SNAPSHOT_BEFORE_VALIDATOR_IMPORT
    receipts.append(import_time_readme_mutation_probe())

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
        def mutate(pack: Path) -> None:
            if status in {"PASS", "FAIL"}:
                set_execution_markers(pack, runtime_tests_executed_for_this_pack=True)
            update_cases(pack, set_shared_summary(status, "NOT_RUN", summaries[status]))
        return mutate

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

    def remove_runtime_execution_marker(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        graph.pop("runtime_tests_executed_for_this_pack")
        write_json(graph_path, graph)

    receipts.append(run_fixture(
        "ordinary validation rejects a missing runtime-execution marker",
        remove_runtime_execution_marker,
        1, ["Execution markers are Boolean in every mode."],
    ))

    def use_non_boolean_delegation_marker(pack: Path) -> None:
        set_execution_markers(pack, subagents_used_for_this_pack=1)
    receipts.append(run_fixture(
        "ordinary validation rejects a non-Boolean delegation marker",
        use_non_boolean_delegation_marker,
        1, ["Execution markers are Boolean in every mode."],
    ))

    def use_non_boolean_marker_in_initial_mode(pack: Path) -> None:
        set_execution_markers(pack, subagents_used_for_this_pack="false")
    receipts.append(run_fixture(
        "initial validation rejects non-Boolean execution markers",
        use_non_boolean_marker_in_initial_mode,
        1, ["Execution markers are Boolean in every mode.",
            "Pack records no runtime tests or delegated reviews"], initial=True,
    ))

    def malformed_ready_groups(pack: Path, groups: Any) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        graph["topological_ready_groups"] = groups
        write_json(graph_path, graph)

    for label, groups in (
        ("an object", {"first": ["P00"]}),
        ("a non-list group", ["P00"]),
        ("a non-string task identifier", [["P00", 7]]),
    ):
        receipts.append(run_fixture(
            f"readiness groups reject {label} with a structured input error",
            lambda pack, value=groups: malformed_ready_groups(pack, value),
            2, ["Planning pack input is valid"],
            expected_report_details={
                "Planning pack input is valid":
                    "task-graph.json 'topological_ready_groups' must be a list of lists of strings",
            },
        ))

    def serial_source_progress(pack: Path) -> None:
        set_task_status(pack, "P00", "PASS")
        set_execution_markers(
            pack, subagents_used_for_this_pack=False, runtime_tests_executed_for_this_pack=False,
        )
        update_cases(pack, set_single_summary("P00", "PASS"))
    receipts.append(run_fixture(
        "serial SOURCE work passes without delegation or runtime claims",
        serial_source_progress,
        0, [],
    ))

    def serial_review_progress(pack: Path) -> None:
        set_task_status(pack, "P00", "REVIEW")
        set_execution_markers(
            pack, subagents_used_for_this_pack=False, runtime_tests_executed_for_this_pack=False,
        )
        update_cases(pack, set_single_summary("P00", "PASS"))
    receipts.append(run_fixture(
        "serial REVIEW work records its own passing assignment",
        serial_review_progress,
        0, [],
    ))

    def all_live_cases_pass(pack: Path, runtime_marker: bool) -> None:
        all_pass(pack)
        set_execution_markers(pack, runtime_tests_executed_for_this_pack=runtime_marker)

    receipts.append(run_fixture(
        "LIVE PASS assignments require a true runtime-execution marker",
        lambda pack: all_live_cases_pass(pack, False),
        1, ["LIVE PASS and FAIL results require the runtime-execution marker."],
    ))
    receipts.append(run_fixture(
        "LIVE PASS assignments are accepted with a true runtime-execution marker",
        lambda pack: all_live_cases_pass(pack, True),
        0, [],
    ))

    def p02_live_failure(pack: Path, runtime_marker: bool) -> None:
        set_task_states(pack, {"P00": "PASS", "P02": "FAIL"})
        set_execution_markers(
            pack, subagents_used_for_this_pack=False,
            runtime_tests_executed_for_this_pack=runtime_marker,
        )

        def update(cases: dict[str, dict[str, Any]]) -> None:
            set_assignment(cases["A01"], "P00", "PASS")
            cases["A01"]["status"] = "PASS"
            set_assignment(cases["A05"], "P02", "FAIL")
            cases["A05"]["status"] = "FAIL"
        update_cases(pack, update)

    receipts.append(run_fixture(
        "LIVE FAIL assignments require a true runtime-execution marker",
        lambda pack: p02_live_failure(pack, False),
        1, ["LIVE PASS and FAIL results require the runtime-execution marker."],
    ))
    receipts.append(run_fixture(
        "LIVE FAIL assignments are accepted with a true runtime-execution marker",
        lambda pack: p02_live_failure(pack, True),
        0, [],
    ))

    def p02_blocked_after_dependency_completion(pack: Path) -> None:
        set_task_states(pack, {"P00": "PASS", "P02": "BLOCKED"})
        set_execution_markers(
            pack, subagents_used_for_this_pack=False, runtime_tests_executed_for_this_pack=False,
        )
        update_cases(pack, set_single_summary("P00", "PASS"))
    receipts.append(run_fixture(
        "LIVE BLOCKED results do not claim runtime execution",
        p02_blocked_after_dependency_completion,
        0, [],
    ))

    def build_assignment_does_not_claim_live_execution(pack: Path) -> None:
        def update(cases: dict[str, dict[str, Any]]) -> None:
            set_assignment(cases["A125"], "T40", "PASS")
            cases["A125"]["status"] = "PASS"
        update_cases(pack, update)
    receipts.append(run_fixture(
        "a BUILD assignment does not claim LIVE runtime execution",
        build_assignment_does_not_claim_live_execution,
        1, ["NOT_STARTED and READY tasks have NOT_RUN assignments"],
        expected_report_details={
            "LIVE PASS and FAIL results require the runtime-execution marker.": [],
        },
    ))

    def true_markers_before_initial_normalization(pack: Path) -> None:
        set_execution_markers(
            pack, subagents_used_for_this_pack=True, runtime_tests_executed_for_this_pack=True,
        )
    receipts.append(run_fixture(
        "temporary normalization resets only valid true execution markers",
        lambda _pack: None,
        0, [], initial=True,
        pre_normalization_mutation=true_markers_before_initial_normalization,
        expected_normalization_diff=[
            {"path": "task-graph.json", "field": "subagents_used_for_this_pack",
             "before": True, "after": False},
            {"path": "task-graph.json", "field": "runtime_tests_executed_for_this_pack",
             "before": True, "after": False},
        ],
    ))

    receipts.append(run_fixture(
        "temporary normalization does not recreate a missing execution marker",
        lambda _pack: None,
        1, ["Execution markers are Boolean in every mode."],
        pre_normalization_mutation=remove_runtime_execution_marker,
        expected_normalization_diff=[],
    ))

    def non_boolean_marker_before_normalization(pack: Path) -> None:
        set_execution_markers(pack, subagents_used_for_this_pack=0)
    receipts.append(run_fixture(
        "temporary normalization does not repair an invalid execution marker",
        lambda _pack: None,
        1, ["Execution markers are Boolean in every mode."],
        pre_normalization_mutation=non_boolean_marker_before_normalization,
        expected_normalization_diff=[],
    ))

    def valid_progress_markers(pack: Path) -> None:
        set_task_status(pack, "P00", "PASS")
        set_execution_markers(
            pack, subagents_used_for_this_pack=True, runtime_tests_executed_for_this_pack=True,
        )
        update_cases(pack, set_single_summary("P00", "PASS"))
    receipts.append(run_fixture(
        "progressed source with true execution markers passes ordinary validation",
        valid_progress_markers,
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
        1, [
            "A86 preserves backend host-action and integrated presentation owners",
            "Assignment results have exact unique case owners and valid statuses",
        ],
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
        1, [
            "Acceptance cases have unique string task owners",
            "Case authored fields match fixed projection",
        ],
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
        set_execution_markers(pack, runtime_tests_executed_for_this_pack=True)
        update_cases(pack, set_shared_summary("PASS", "NOT_RUN", "NOT_RUN"))
    receipts.append(run_fixture(
        "initial mode rejects assignment progress", assignment_progress,
        1, ["NOT_STARTED and READY tasks have NOT_RUN assignments",
            "Initial task, case, and assignment statuses claim no execution",
            "Pack records no runtime tests or delegated reviews"], initial=True,
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
            if status in {"PASS", "FAIL"}:
                set_execution_markers(pack, runtime_tests_executed_for_this_pack=True)
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

    review_assignment_check = "REVIEW tasks have recorded results for every owned assignment"

    def review_task_with_results(task_id: str, status: str,
                                 not_run_case: str | None = None) -> Callable[[Path], None]:
        def mutate(pack: Path) -> None:
            all_pass(pack)
            set_task_states(pack, {task_id: "REVIEW"})
            owned_cases = task_map(pack)[task_id]["test_cases"]

            def update(cases: dict[str, dict[str, Any]]) -> None:
                for case_id in owned_cases:
                    case = cases[case_id]
                    assigned_status = "NOT_RUN" if case_id == not_run_case else status
                    set_assignment(case, task_id, assigned_status)
                    case["status"] = status_summary([row["status"] for row in case["assignment_results"]])

            update_cases(pack, update)
        return mutate

    receipts.append(run_fixture(
        "REVIEW task requires its own result when another owner has PASS",
        review_task_with_results("T32", "PASS", not_run_case="A22"),
        1, [review_assignment_check],
    ))
    receipts.append(run_fixture(
        "REVIEW task requires every owned assignment to have a result",
        review_task_with_results("T40", "PASS", not_run_case="A123"),
        1, [review_assignment_check],
    ))
    for status in ("PASS", "FAIL", "BLOCKED"):
        receipts.append(run_fixture(
            f"REVIEW task accepts its own {status} assignment results",
            review_task_with_results("T40", status),
            0, [],
        ))

    def no_case_task_in_review(pack: Path) -> None:
        all_pass(pack)
        set_task_states(pack, {"U06": "REVIEW"})
        if task_map(pack)["U06"]["test_cases"]:
            raise ValueError("Expected U06 to keep its manual no-case review rule")
    receipts.append(run_fixture(
        "REVIEW task without acceptance cases remains valid",
        no_case_task_in_review,
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
        1, [
            "Task test_cases contain unique string IDs",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "Task test_cases contain unique string IDs",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "Task completion rules match the fixed evidence contract",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "Task review rules match the different-subagent and post-merge contract",
            "Task authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "task review rule requires a post-merge integration rerun",
        lambda pack: set_review_rule(pack, "A different subagent reviews the result."),
        1, [
            "Task review rules match the different-subagent and post-merge contract",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "Task required artifacts match canonical evidence set and report path",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "Task required artifacts match canonical evidence set and report path",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "Task required artifacts match canonical evidence set and report path",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "Task required artifacts match canonical evidence set and report path",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "P02 bootstrap records, P04 imports, and T12 reruns A05/A06",
            "Task authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "T12 cannot reuse the earlier P02 LIVE results",
        lambda pack: weaken_early_live_task(
            pack, "T12", "steps",
            "Run separate fresh A05 and A06 LIVE tests with new run IDs after the P04 harness is available.",
        ),
        1, [
            "P02 bootstrap records, P04 imports, and T12 reruns A05/A06",
            "Task authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "P04 must validate and import early records without closing T12",
        lambda pack: weaken_early_live_task(
            pack, "P04", "steps",
            "Validate and import the P02 A05 and A06 records without assigning them to T12.",
        ),
        1, [
            "P02 bootstrap records, P04 imports, and T12 reruns A05/A06",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "P04 harness command registration and P05 build wiring scopes agree",
            "Task authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "P05 retains only workspace and build graph registration",
        lambda pack: weaken_registration_scope(
            pack, "P05",
            "crates/tools/jackin-xtask and test-support workspace and build graph registration only",
            "crates/tools/jackin-xtask and test-support registration",
        ),
        1, [
            "P04 harness command registration and P05 build wiring scopes agree",
            "Task authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "P05 must register xtask and test support in the workspace graph",
        lambda pack: weaken_early_live_task(
            pack, "P05", "steps",
            "Register jackin-xtask and test-support in the production workspace and build graph.",
        ),
        1, [
            "P04 harness command registration and P05 build wiring scopes agree",
            "Task authored fields match fixed projection",
        ],
    ))

    target_matrix_check = "Target matrix qualification requests route through Integration"

    def remove_p04_initial_matrix_request(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "P04")
        task["steps"].remove(
            "Submit the initial required-row registration to Integration as an ordered patch request before qualification."
        )
        task["acceptance"].remove(
            "Integration alone applies the initial target registration before qualification updates."
        )
        write_json(graph_path, graph)
        sync_task_packet(pack, "P04")
    receipts.append(run_fixture(
        "P04 requests the initial target registration from Integration",
        remove_p04_initial_matrix_request,
        1, [
            "Target matrix qualification requests route through Integration",
            "Task authored fields match fixed projection",
        ],
    ))

    def remove_t37_matrix_request(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T37")
        task["steps"].remove(
            "Submit this task's qualified tuple, case applicability, and fixture references to Integration as an ordered targets.toml patch request before completion."
        )
        write_json(graph_path, graph)
        sync_task_packet(pack, "T37")
    receipts.append(run_fixture(
        "T35 through T38 retain ordered qualified-target patch requests",
        remove_t37_matrix_request,
        1, [
            "Target matrix qualification requests route through Integration",
            "Task authored fields match fixed projection",
        ],
    ))

    def give_p04_direct_matrix_scope(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "P04")
        task["write_scope"].append("tests/v2/targets.toml")
        write_json(graph_path, graph)
        sync_task_packet(pack, "P04")
    receipts.append(run_fixture(
        "P04 cannot receive direct write scope for the central target matrix",
        give_p04_direct_matrix_scope,
        1, [
            "Target matrix qualification requests route through Integration",
            "Task authored fields match fixed projection",
        ],
    ))

    def give_t35_direct_matrix_scope(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T35")
        task["write_scope"].append("tests/v2/targets.toml")
        write_json(graph_path, graph)
        sync_task_packet(pack, "T35")
    receipts.append(run_fixture(
        "T35 cannot write the central target matrix directly",
        give_t35_direct_matrix_scope,
        1, [
            "Target matrix qualification requests route through Integration",
            "Task authored fields match fixed projection",
        ],
    ))

    def remove_blocked_row_rule(pack: Path) -> None:
        path = pack / "ACCEPTANCE.md"
        document = path.read_text(encoding="utf-8")
        rule = "Preserve unavailable required rows as `BLOCKED`.\n"
        if document.count(rule) != 1:
            raise ValueError("Expected one unavailable-required-row rule")
        path.write_text(document.replace(rule, "Remove unavailable required rows from the matrix.\n", 1),
                        encoding="utf-8")
    receipts.append(run_fixture(
        "Required target rows remain and unavailable rows stay BLOCKED",
        remove_blocked_row_rule,
        1, ['Target matrix qualification requests route through Integration',
 'Acceptance policy before detailed cases matches fixed checksum'],
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
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def omit_final_case(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["required_t40_cases"].remove("A125")
    receipts.append(run_fixture(
        "release finalization cannot omit A125",
        lambda pack: mutate_release_protocol(pack, omit_final_case),
        1, [
            "Release gate protocol matches fixed semantics",
            "Release gate references name T40 and its owned final cases",
            "Graph declarations match fixed projection",
        ],
    ))

    def omit_final_review(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["required_evidence"].remove("Independent T40 review")
    receipts.append(run_fixture(
        "release finalization requires independent T40 review",
        lambda pack: mutate_release_protocol(pack, omit_final_review),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def omit_merged_checks(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["required_evidence"].remove("Merged-source checks")
    receipts.append(run_fixture(
        "release finalization requires merged-source checks",
        lambda pack: mutate_release_protocol(pack, omit_merged_checks),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def weaken_post_t39_boundary(protocol: dict[str, Any]) -> None:
        protocol["runtime_requalification"]["subsequent_runtime_changes"] = (
            "Every later runtime-affecting change except T40 version and package changes"
        )
    receipts.append(run_fixture(
        "runtime requalification treats T40 version and package changes as triggers",
        lambda pack: mutate_release_protocol(pack, weaken_post_t39_boundary),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def remove_unknown_impact_fallback(protocol: dict[str, Any]) -> None:
        protocol["runtime_requalification"]["unknown_impact_fallback"] = "No rerun is required"
    receipts.append(run_fixture(
        "unjustified runtime impact requires the full applicable LIVE matrix",
        lambda pack: mutate_release_protocol(pack, remove_unknown_impact_fallback),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def reassign_runtime_cases_to_t40(protocol: dict[str, Any]) -> None:
        protocol["runtime_requalification"]["execution"] = "T40 owns and executes every LIVE case"
    receipts.append(run_fixture(
        "runtime reruns preserve original case owners and let T40 coordinate",
        lambda pack: mutate_release_protocol(pack, reassign_runtime_cases_to_t40),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def permit_overwriting_runtime_history(protocol: dict[str, Any]) -> None:
        protocol["runtime_requalification"]["evidence_history"] = "Replace prior run records"
    receipts.append(run_fixture(
        "runtime reruns retain early failed blocked and repeat evidence",
        lambda pack: mutate_release_protocol(pack, permit_overwriting_runtime_history),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def remove_runtime_evidence_bindings(protocol: dict[str, Any]) -> None:
        protocol["runtime_requalification"]["fresh_result_bindings"].remove("Unique run ID")
    receipts.append(run_fixture(
        "fresh LIVE results bind final inputs artifacts target run and review",
        lambda pack: mutate_release_protocol(pack, remove_runtime_evidence_bindings),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def allow_stale_runtime_results(protocol: dict[str, Any]) -> None:
        protocol["runtime_requalification"]["required_result"] = "Any earlier LIVE result is sufficient"
        protocol["runtime_requalification"]["refusal_conditions"].remove("Stale")
    receipts.append(run_fixture(
        "missing stale failed or blocked LIVE reruns prevent finalization",
        lambda pack: mutate_release_protocol(pack, allow_stale_runtime_results),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def omit_final_runtime_requalification(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["required_evidence"].remove(
            "Fresh bound LIVE PASS results after the latest runtime-affecting change for every affected case"
        )
    receipts.append(run_fixture(
        "finalization explicitly requires fresh bound LIVE results",
        lambda pack: mutate_release_protocol(pack, omit_final_runtime_requalification),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def require_future_t40_outputs_at_preflight(protocol: dict[str, Any]) -> None:
        protocol["preflight"]["future_t40_runtime_outputs_are_inputs"] = True
    receipts.append(run_fixture(
        "preflight does not require future T40 runtime-change outputs",
        lambda pack: mutate_release_protocol(pack, require_future_t40_outputs_at_preflight),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def weaken_t39_runtime_procedure(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T39")
        task["steps"].remove("Record runtime impact and fresh LIVE reruns after final legacy deletions.")
        write_json(graph_path, graph)
        sync_task_packet(pack, "T39")
    receipts.append(run_fixture(
        "T39 task packet records post-deletion runtime impact and LIVE reruns",
        weaken_t39_runtime_procedure,
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "Task authored fields match fixed projection",
        ],
    ))

    def weaken_t40_runtime_procedure(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T40")
        task["steps"].remove("Rerun every affected LIVE case through its original owner or operator.")
        write_json(graph_path, graph)
        sync_task_packet(pack, "T40")
    receipts.append(run_fixture(
        "T40 card requires fresh affected LIVE results through original operators",
        weaken_t40_runtime_procedure,
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "Task authored fields match fixed projection",
        ],
    ))

    def omit_t40_version_requalification(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T40")
        task["steps"].remove(
            "Treat T40 version and package changes as triggers; record impact and coordinate fresh LIVE reruns."
        )
        write_json(graph_path, graph)
        sync_task_packet(pack, "T40")
    receipts.append(run_fixture(
        "T40 version and package changes cannot bypass fresh LIVE reruns",
        omit_t40_version_requalification,
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "Task authored fields match fixed projection",
        ],
    ))

    def weaken_a123_live_freshness(pack: Path) -> None:
        path = pack / "acceptance-cases.json"
        data = read_json(path)
        case = next(row for row in data["cases"] if row["id"] == "A123")
        case["expected"] = "Every refusal prevents approval. Finalization uses all fixed prerequisites."
        write_json(path, data)
        sync_case_block(pack, case)
    receipts.append(run_fixture(
        "A123 refuses stale or missing post-deletion LIVE evidence",
        weaken_a123_live_freshness,
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "Case authored fields match fixed projection",
        ],
    ))

    def allow_preflight_approval(protocol: dict[str, Any]) -> None:
        protocol["preflight"]["may_approve_release"] = True
    receipts.append(run_fixture(
        "release preflight cannot issue approval",
        lambda pack: mutate_release_protocol(pack, allow_preflight_approval),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
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
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "Case authored fields match fixed projection",
        ],
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
        1, [
            "PACK-CHECKS limits release-gate claims to static structure",
            "PACK-CHECKS matches canonical template with mutable count and result slots",
        ],
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
        1, [
            "Review packet uses candidate input and post-merge completion stages",
            "SUBAGENTS.md matches its fixed checksum",
        ],
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
        1, [
            "P04 is listed under the Qualification subagent",
            "Subagent task assignments exactly match graph owners",
            "SUBAGENTS.md matches its fixed checksum",
        ],
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
        1, [
            "Task citations are represented in traceability",
            "Traceability authored sections match fixed projection",
        ],
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
        1, [
            "Task SPEC citations use valid syntax",
            "Task authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "task SPEC citation beyond the document is rejected",
        lambda pack: malformed_source_citation(pack, "SPEC L550"),
        1, [
            "Task SPEC citation ranges are within SPEC.md",
            "Task authored fields match fixed projection",
        ],
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

    def append_nonobject_record(pack: Path, relative_path: str, field: str) -> None:
        path = pack / relative_path
        data = read_json(path)
        records = data[field]
        if not isinstance(records, list):
            raise ValueError(f"Expected {field} to be an array")
        records.insert(0, None)
        write_json(path, data)

    for relative_path, field in (
        ("task-graph.json", "tasks"),
        ("acceptance-cases.json", "cases"),
        ("traceability.json", "sections"),
    ):
        receipts.append(run_fixture(
            f"{relative_path} rejects a null {field} record before owner evaluation",
            lambda pack, path=relative_path, key=field: append_nonobject_record(pack, path, key),
            2, ["Planning pack input is valid"],
            expected_report_details={
                "Planning pack input is valid": f"{relative_path} '{field}' record at index 0 must be an object",
            },
        ))

    def validator_report(passed: bool, failures: list[str],
                         checks: list[dict[str, Any]] | None = None,
                         initial: bool = False) -> str:
        if checks is None:
            checks = [
                {"check": name, "passed": True, "detail": None}
                for name in fixture_check_inventory(initial)
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
                       initial: bool = False, expect_report_match: bool = False,
                       expected_report_details: dict[str, Any] | None = None) -> None:
        receipts.append(run_fixture(
            name,
            lambda _pack: None,
            expected_exit,
            expected_failures or [],
            initial=initial,
            fake_stdout=stdout,
            expect_report_match=expect_report_match,
            expected_report_errors=report_errors,
            expected_report_details=expected_report_details,
        ))

    def short_form_report(check_name: str, detail: Any) -> str:
        return json.dumps({
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
            "checks": [{"check": check_name, "passed": False, "detail": detail}],
            "failures": [check_name],
        })

    receipts.append(run_fixture(
        "ordinary baseline report contains the complete ordered check inventory",
        lambda _pack: None, 0, [],
    ))
    receipts.append(run_fixture(
        "initial baseline report contains ordinary and initial-only check inventories",
        lambda _pack: None, 0, [], initial=True,
    ))
    receipts.append(run_fixture(
        "manifest owns the exact recursive Markdown task-packet set",
        lambda _pack: None, 0, [],
    ))

    def change_graph_classification(pack: Path) -> None:
        path = pack / "task-graph.json"
        graph = read_json(path)
        graph["status"] = "RELEASE_APPROVED"
        write_json(path, graph)
    receipts.append(run_fixture(
        "graph root classification rejects a release-approval value",
        change_graph_classification,
        1, ["Graph declarations match fixed projection"],
    ))

    def remove_expected_task_count_check(pack: Path) -> None:
        path = pack / "validate_pack.py"
        source = path.read_text(encoding="utf-8")
        replacements = (
            ('    "Expected task count",\n', ""),
            ('    check("Expected task count", len(tasks) == 46, len(tasks))\n', ""),
        )
        for old, new in replacements:
            if source.count(old) != 1:
                raise ValueError(f"Expected one task-count validator entry: {old.strip()}")
            source = source.replace(old, new, 1)
        path.write_text(source, encoding="utf-8")
    receipts.append(run_fixture(
        "fixture oracle detects removal of task-count implementation and inventory entry",
        remove_expected_task_count_check,
        0, [], expect_report_match=False,
        expected_report_errors=["checks do not match the canonical ordered inventory"],
    ))

    wrong_nonempty_counts = json.loads(validator_report(True, []))
    wrong_nonempty_counts["counts"]["tasks"] += 1
    parser_fixture(
        "canonical report rejects nonempty counts that differ from tested inputs",
        json.dumps(wrong_nonempty_counts), ["counts do not match tested fixture inputs"],
    )
    all_zero_counts = json.loads(validator_report(True, []))
    all_zero_counts["counts"] = {name: 0 for name in all_zero_counts["counts"]}
    parser_fixture(
        "canonical report rejects nonempty all-zero counts",
        json.dumps(all_zero_counts), ["counts do not match tested fixture inputs"],
    )

    receipts.append(run_fixture(
        "missing validator short form rejects fabricated README detail",
        remove_validator_copy,
        1, ["Required pack files exist"],
        fake_stdout=short_form_report("Required pack files exist", ["README.md"]),
        expect_report_match=False,
        expected_report_errors=[],
        expected_report_details={"Required pack files exist": ["validate_pack.py"]},
    ))
    receipts.append(run_fixture(
        "missing tasks short form rejects fabricated cases detail",
        malformed_manifest,
        2, ["Planning pack input is valid"],
        fake_stdout=short_form_report("Planning pack input is valid", "'cases'"),
        expect_report_match=False,
        expected_report_errors=[],
        expected_report_details={"Planning pack input is valid": "'tasks'"},
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
        1, [
            "Subagent task assignments exactly match graph owners",
            "SUBAGENTS.md matches its fixed checksum",
        ],
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
        1, [
            "Subagent task assignments exactly match graph owners",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "P04, T40, A123, and A125 mirror the fixed release gate contract",
            "T40 finalization writes route through Integration",
            "Task authored fields match fixed projection",
        ],
    ))

    def assign_finalization_writer_to_t40(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["finalization_writer"] = "Independent review subagent"
    receipts.append(run_fixture(
        "release finalization keeps shared writes with Integration",
        lambda pack: mutate_release_protocol(pack, assign_finalization_writer_to_t40),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def omit_conditional_patch_approval(protocol: dict[str, Any]) -> None:
        protocol["finalization"].pop("conditional_patch_approval")
    receipts.append(run_fixture(
        "release finalization requires conditional approval for supported patched tuples",
        lambda pack: mutate_release_protocol(pack, omit_conditional_patch_approval),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
    ))

    def require_approval_for_unpatched_tuples(protocol: dict[str, Any]) -> None:
        protocol["finalization"]["conditional_patch_approval"]["unpatched_tuples_require_record"] = True
    receipts.append(run_fixture(
        "unpatched supported tuples do not require patch-approval records",
        lambda pack: mutate_release_protocol(pack, require_approval_for_unpatched_tuples),
        1, [
            "Release gate protocol matches fixed semantics",
            "Graph declarations match fixed projection",
        ],
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
        1, [
            "A125 verifies conditional patched-tuple approval",
            "Case authored fields match fixed projection",
        ],
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
        1, [
            "C01 source pin table matches all four fixed references",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C01 source pin table matches all four fixed references",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C01 source pin table matches all four fixed references",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C01 source pin table matches all four fixed references",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C01 source pin table matches all four fixed references",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C01 source pin table matches all four fixed references",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C02 production crate graph and boundary section matches fixed bytes",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C02 production crate graph and boundary section matches fixed bytes",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C02 production crate graph and boundary section matches fixed bytes",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C01 source pin table matches all four fixed references",
            "C02 production crate graph and boundary section matches fixed bytes",
            "CONTRACTS.md matches its fixed checksum",
        ],
    ))

    def remove_c02_heading(pack: Path) -> None:
        mutate_c02_bytes(
            pack, b"## C02. Production crate graph\n", b"", "missing C02 heading",
        )
    receipts.append(run_fixture(
        "C02 digest rejects a missing section anchor",
        remove_c02_heading,
        1, [
            "C01 source pin table matches all four fixed references",
            "C02 production crate graph and boundary section matches fixed bytes",
            "CONTRACTS.md matches its fixed checksum",
        ],
    ))

    def remove_c03_boundary(pack: Path) -> None:
        mutate_c02_bytes(
            pack, b"## C03. Configuration and identifiers\n", b"", "missing C03 boundary",
        )
    receipts.append(run_fixture(
        "C02 digest rejects a missing end boundary",
        remove_c03_boundary,
        1, [
            "C02 production crate graph and boundary section matches fixed bytes",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "C02 production crate graph and boundary section matches fixed bytes",
            "CONTRACTS.md matches its fixed checksum",
        ],
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
        1, [
            "Task file and test write scopes match canonical paths",
            "Task graph fields and canonical preamble match individual task packets",
        ],
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

    path_contract_check = [
        "Task file and test write scopes match canonical paths",
        "Task authored fields match fixed projection",
    ]

    def add_nested_task_packet(pack: Path) -> None:
        archived = pack / "tasks" / "archive" / "T99.md"
        archived.parent.mkdir(parents=True)
        archived.write_text("# Archived packet\n", encoding="utf-8")
    receipts.append(run_fixture(
        "exact manifest packet set rejects nested archived Markdown",
        add_nested_task_packet,
        1, ["Task file and test write scopes match canonical paths"],
    ))

    def hide_canonical_task_packet(pack: Path) -> None:
        packet = pack / "tasks" / "P00.md"
        packet.rename(packet.with_name("P00.md.disabled"))
    receipts.append(run_fixture(
        "exact manifest packet set rejects a missing canonical Markdown packet",
        hide_canonical_task_packet,
        1, [
            "Task file and test write scopes match canonical paths",
            "Task graph fields and canonical preamble match individual task packets",
        ],
    ))

    for scenario, task_file in (
        ("relocated", "tasks/archive/P00.md"),
        ("traversing", "tasks/../PLAN.md"),
        ("absolute", "/tmp/P00.md"),
        ("colliding", "tasks/P01.md"),
    ):
        receipts.append(run_fixture(
            f"task file path rejects {scenario} locations",
            lambda pack, value=task_file: set_task_field(pack, "P00", "task_file", value),
            1, path_contract_check,
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
            1, path_contract_check,
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
        "Task authored fields match fixed projection",
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
        1, [
            "Acceptance case IDs map to fixed evidence levels",
            "Case authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "valid evidence-level downgrade does not reclassify A50",
        lambda pack: change_case_level(pack, "A50", "UNIT"),
        1, [
            "Acceptance case IDs map to fixed evidence levels",
            "Case authored fields match fixed projection",
        ],
    ))

    goal_contract_check = "GOAL.md matches its fixed checksum"
    def truncate_goal(pack: Path) -> None:
        path = pack / "GOAL.md"
        path.write_bytes(path.read_bytes()[:-1])
    receipts.append(run_fixture(
        "GOAL.md rejects truncation",
        truncate_goal,
        1, ['GOAL.md matches its fixed checksum'],
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
        1, ['GOAL.md matches its fixed checksum'],
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
        1, [
            "Authoritative concurrency declaration matches graph and subagent limit",
            "SUBAGENTS.md matches its fixed checksum",
        ],
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
        1, [
            "Authoritative concurrency declaration matches graph and subagent limit",
            "SUBAGENTS.md matches its fixed checksum",
        ],
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
        1, [
            "Authoritative concurrency declaration matches graph and subagent limit",
            "SUBAGENTS.md matches its fixed checksum",
        ],
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

    def add_nonstandard_json_constant(pack: Path, relative_path: str, constant: str) -> None:
        path = pack / relative_path
        document = path.read_text(encoding="utf-8")
        closing = document.rfind("}")
        if closing < 0:
            raise ValueError(f"Expected a root JSON object in {relative_path}")
        prefix = document[:closing].rstrip()
        separator = "" if prefix.endswith("{") else ","
        path.write_text(
            prefix + separator + f'\n  "fixture_constant": {constant}\n}}\n',
            encoding="utf-8",
        )

    for relative_path, constant in (
        ("task-graph.json", "NaN"),
        ("acceptance-cases.json", "Infinity"),
        ("traceability.json", "-Infinity"),
    ):
        receipts.append(run_fixture(
            f"{relative_path} rejects nonstandard JSON constant {constant}",
            lambda pack, path=relative_path, value=constant:
                add_nonstandard_json_constant(pack, path, value),
            2, ["Planning pack input is valid"],
            expected_report_details={
                "Planning pack input is valid": f"nonstandard JSON constant: {constant}",
            },
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

    def report_with_nonstandard_constant(constant: str) -> str:
        document = validator_report(True, [])
        source_counts = derive_expected_counts(ROOT)
        if source_counts is None:
            raise ValueError("Expected input-derived counts for the canonical report")
        old = f'"checked_sentences": {source_counts["checked_sentences"]}'
        if document.count(old) != 1:
            raise ValueError("Expected one input-derived checked-sentence count in the canonical report")
        return document.replace(old, f'"checked_sentences": {constant}', 1)

    for constant in ("NaN", "Infinity", "-Infinity"):
        parser_fixture(
            f"fixture report parser rejects nonstandard JSON constant {constant}",
            report_with_nonstandard_constant(constant),
            [f"nonstandard JSON constant: {constant}"],
        )

    a13_owner_check = "A13 preserves separate current-graph and final-post-deletion runs"
    a15_owner_check = "A15 separates observation, adapter, and integrated activation owners"
    a16_owner_check = "A16 preserves accepted-revision observation and product-presentation owners"
    a17_owner_check = "A17 preserves observation layers and integrated activation ownership"

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
        1, [
            "A13 preserves separate current-graph and final-post-deletion runs",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "A13 preserves separate current-graph and final-post-deletion runs",
            "Task authored fields match fixed projection",
        ],
    ))

    def remove_p05_a13_cargo_metadata(pack: Path) -> None:
        path = pack / "acceptance-cases.json"
        data = read_json(path)
        case = next(row for row in data["cases"] if row["id"] == "A13")
        old = "P05 captures Cargo metadata for the newly created current graph, "
        if case["action"].count(old) != 1:
            raise ValueError("Expected one P05 current-graph Cargo metadata instruction")
        case["action"] = case["action"].replace(old, "P05 inspects the newly created current graph, ", 1)
        write_json(path, data)
        sync_case_block(pack, case)
    receipts.append(run_fixture(
        "A13 cannot omit P05 current-graph Cargo metadata",
        remove_p05_a13_cargo_metadata,
        1, [
            "A13 preserves separate current-graph and final-post-deletion runs",
            "Case authored fields match fixed projection",
        ],
    ))

    def remove_t39_a13_cargo_metadata(pack: Path) -> None:
        path = pack / "acceptance-cases.json"
        data = read_json(path)
        case = next(row for row in data["cases"] if row["id"] == "A13")
        old = "T39 captures fresh Cargo metadata for the final candidate graph and "
        if case["action"].count(old) != 1:
            raise ValueError("Expected one T39 final-graph Cargo metadata instruction")
        case["action"] = case["action"].replace(old, "T39 checks the final candidate graph and ", 1)
        write_json(path, data)
        sync_case_block(pack, case)
    receipts.append(run_fixture(
        "A13 cannot omit T39 final-graph Cargo metadata",
        remove_t39_a13_cargo_metadata,
        1, [
            "A13 preserves separate current-graph and final-post-deletion runs",
            "Case authored fields match fixed projection",
        ],
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
        1, [
            "A15 separates observation, adapter, and integrated activation owners",
            "Assignment results have exact unique case owners and valid statuses",
            "Case and task ownership maps agree",
            "Case authored fields match fixed projection",
        ],
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
        1, [
            "A15 separates observation, adapter, and integrated activation owners",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "A15 separates observation, adapter, and integrated activation owners",
            "A16 preserves accepted-revision observation and product-presentation owners",
            "Task authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "T13 cannot collapse A15 and A16 into product behavior",
        lambda pack: weaken_layer_contract(pack, "T13", "A15"),
        1, [
            "A15 separates observation, adapter, and integrated activation owners",
            "A16 preserves accepted-revision observation and product-presentation owners",
            "Task authored fields match fixed projection",
        ],
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
        1, [
            "A16 preserves accepted-revision observation and product-presentation owners",
            "Task authored fields match fixed projection",
        ],
    ))
    receipts.append(run_fixture(
        "T13 must bind A16's replacement source to its accepted revision",
        lambda pack: weaken_a16_layer_source(
            pack, "T13",
            "A16 at T13 binds the effective replacement source to the observed accepted revision without claiming an intersection.",
        ),
        1, [
            "A16 preserves accepted-revision observation and product-presentation owners",
            "Task authored fields match fixed projection",
        ],
    ))

    receipts.append(run_fixture(
        "A16 cannot omit the T29 product-presentation owner",
        lambda pack: remove_case_owner(pack, "A16", "T29"),
        1, [
            "A16 preserves accepted-revision observation and product-presentation owners",
            "Assignment results have exact unique case owners and valid statuses",
            "Case and task ownership maps agree",
            "Case authored fields match fixed projection",
        ],
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
        1, [
            "A16 preserves accepted-revision observation and product-presentation owners",
            "Task authored fields match fixed projection",
        ],
    ))

    def remove_a17_t23_owner(pack: Path) -> None:
        remove_case_owner(pack, "A17", "T23")
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T23")
        task["test_cases"].remove("A17")
        write_json(graph_path, graph)
        sync_task_packet(pack, "T23")
    receipts.append(run_fixture(
        "A17 cannot omit integrated T23 ownership",
        remove_a17_t23_owner,
        1, [
            "A17 preserves observation layers and integrated activation ownership",
            "Task authored fields match fixed projection",
            "Case authored fields match fixed projection",
        ],
    ))

    def weaken_t23_a17_integrated_activation(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T23")
        task["steps"].remove(
            "Exercise A17 through integrated StartSession and re-admission after provider attach, revocation, and binding changes."
        )
        task["acceptance"].remove(
            "A17 qualifies StartSession and re-admission against installed provider revisions."
        )
        write_json(graph_path, graph)
        sync_task_packet(pack, "T23")
    receipts.append(run_fixture(
        "T23 retains integrated StartSession and re-admission for A17",
        weaken_t23_a17_integrated_activation,
        1, [
            "A17 preserves observation layers and integrated activation ownership",
            "Task authored fields match fixed projection",
        ],
    ))

    def remove_a16_policy_load(pack: Path) -> None:
        path = pack / "acceptance-cases.json"
        data = read_json(path)
        case = next(row for row in data["cases"] if row["id"] == "A16")
        old = "Load the global policy. "
        if case["action"].count(old) != 1:
            raise ValueError("Expected one A16 global-policy load instruction")
        case["action"] = case["action"].replace(old, "", 1)
        write_json(path, data)
        sync_case_block(pack, case)
    receipts.append(run_fixture(
        "A16 loads the global policy before querying and displaying it",
        remove_a16_policy_load,
        1, [
            "A16 preserves accepted-revision observation and product-presentation owners",
            "Case authored fields match fixed projection",
        ],
    ))

    def weaken_a22_assertion(pack: Path) -> None:
        cases_path = pack / "acceptance-cases.json"
        data = read_json(cases_path)
        case = next(row for row in data["cases"] if row["id"] == "A22")
        case["expected"] = "The task reports its result without a separate result for each owner."
        write_json(cases_path, data)
        sync_case_block(pack, case)
    receipts.append(run_fixture(
        "Synchronized A22 prose cannot weaken owner-specific assertions",
        weaken_a22_assertion,
        1, ["Case authored fields match fixed projection"],
    ))

    def weaken_contract_c06(pack: Path) -> None:
        path = pack / "CONTRACTS.md"
        document = path.read_text(encoding="utf-8")
        old = "Missing one account value or mixing\nthe variants blocks activation."
        if document.count(old) != 1:
            raise ValueError("Expected one C06 credential-authority requirement")
        path.write_text(document.replace(old, "Missing one account value can use local configuration.", 1),
                        encoding="utf-8")
    receipts.append(run_fixture(
        "C06 credential-authority fail-open cannot alter the complete contracts",
        weaken_contract_c06,
        1, ["CONTRACTS.md matches its fixed checksum"],
    ))

    def change_t23_dependency(pack: Path, dependency: str, operation: str) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T23")
        if operation == "remove":
            task["depends_on"].remove(dependency)
        elif operation == "add":
            task["depends_on"].append(dependency)
        else:
            raise ValueError(f"Unexpected dependency operation: {operation}")
        write_json(graph_path, graph)
        recompute_ready_groups(pack)
        sync_task_packet(pack, "T23")
        sync_task_indexes(pack)
    receipts.append(run_fixture(
        "T23 cannot omit its T16 dependency after all graph mirrors are recomputed",
        lambda pack: change_t23_dependency(pack, "T16", "remove"),
        1, ["Task authored fields match fixed projection"],
    ))
    receipts.append(run_fixture(
        "T23 cannot add an extra dependency after all graph mirrors are recomputed",
        lambda pack: change_t23_dependency(pack, "P05", "add"),
        1, ["Task authored fields match fixed projection"],
    ))

    def add_worker_root_cargo_claim(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T32")
        task["write_scope"].append("Root Cargo.toml and Cargo.lock")
        write_json(graph_path, graph)
        sync_task_packet(pack, "T32")
    receipts.append(run_fixture(
        "Worker task cannot claim shared root Cargo files",
        add_worker_root_cargo_claim,
        1, ["Task authored fields match fixed projection"],
    ))

    def append_contradictory_task_scope(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T14")
        task["write_scope"].append("All repository files, including files leased to other tasks")
        write_json(graph_path, graph)
        sync_task_packet(pack, "T14")
    receipts.append(run_fixture(
        "Extra contradictory task scope cannot override the authored lease",
        append_contradictory_task_scope,
        1, ["Task authored fields match fixed projection"],
    ))

    def rebase_t23_citation_and_trace(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        task = next(row for row in graph["tasks"] if row["id"] == "T23")
        task["source_basis"] = "SPEC L1"
        write_json(graph_path, graph)
        sync_task_packet(pack, "T23")

        trace_path = pack / "traceability.json"
        trace = read_json(trace_path)
        for section in trace["sections"]:
            section["tasks"] = [task_id for task_id in section["tasks"] if task_id != "T23"]
        trace["sections"][0]["tasks"].append("T23")
        write_json(trace_path, trace)
        sync_trace_table(pack)
    receipts.append(run_fixture(
        "T23 cannot rebase its citation to SPEC L1 with synchronized traceability",
        rebase_t23_citation_and_trace,
        1, ["Task authored fields match fixed projection",
            "Traceability authored sections match fixed projection"],
    ))

    def weaken_subagent_shared_file_authority(pack: Path) -> None:
        path = pack / "SUBAGENTS.md"
        document = path.read_text(encoding="utf-8")
        old = "The integration subagent alone changes these shared files:"
        if document.count(old) != 1:
            raise ValueError("Expected one shared-file authority statement")
        path.write_text(document.replace(old, "Task subagents may change these shared files:", 1),
                        encoding="utf-8")
    receipts.append(run_fixture(
        "SUBAGENTS shared-file authority cannot be weakened",
        weaken_subagent_shared_file_authority,
        1, ["SUBAGENTS.md matches its fixed checksum"],
    ))

    def append_conflicting_pack_report_prose(pack: Path) -> None:
        path = pack / "PACK-CHECKS.md"
        with path.open("a", encoding="utf-8") as document:
            document.write("\nTask owners may approve their own release.\n")
    receipts.append(run_fixture(
        "PACK-CHECKS rejects conflicting prose outside its result slots",
        append_conflicting_pack_report_prose,
        1, ["PACK-CHECKS matches canonical template with mutable count and result slots"],
    ))

    def change_pack_check_count_and_result_slots(pack: Path) -> None:
        path = pack / "PACK-CHECKS.md"
        document = path.read_text(encoding="utf-8")
        tested_counts = derive_expected_counts(pack)
        if tested_counts is None:
            raise ValueError("Expected input-derived counts for the copied pack")
        sentence_count = tested_counts["checked_sentences"]
        replacements = (
            (f"| Checked procedural sentences | {sentence_count} |",
             "| Checked procedural sentences | 0 |"),
            ("Result: Static pack validation and per-task assignment fixtures pass.",
             "Result: Static pack validation and per-task assignment fixtures fail."),
            ("| GOAL matches its fixed checksum | PASS |", "| GOAL matches its fixed checksum | FAIL |"),
            ("| Task and case statuses use documented values | PASS |",
             "| Task and case statuses use documented values | BLOCKED |"),
            ("| Acceptance manifest status uses documented values and matches the case summary | PASS |",
             "| Acceptance manifest status uses documented values and matches the case summary | NOT_RUN |"),
            ("| All assignments pass and the shared case summary is `PASS` | Accept, exit 0 | PASS, exit 0 |",
             "| All assignments pass and the shared case summary is `PASS` | Accept, exit 0 | FAIL, exit 1 / exit 2 |"),
            ("| Task `PASS` with its own case assignments `PASS` | Accept, exit 0 | PASS, exit 0 |",
             "| Task `PASS` with its own case assignments `PASS` | Accept, exit 0 | BLOCKED, exit 0 |"),
            ("| U02 `PASS`, T32 `NOT_RUN`, summary `NOT_RUN`, with completed prerequisites | Accept, exit 0 | PASS, exit 0 |",
             "| U02 `PASS`, T32 `NOT_RUN`, summary `NOT_RUN`, with completed prerequisites | Accept, exit 0 | NOT_RUN |"),
        )
        for old, new in replacements:
            if document.count(old) != 1:
                raise ValueError(f"Expected one mutable report slot: {old}")
            document = document.replace(old, new, 1)
        path.write_text(document, encoding="utf-8")
    receipts.append(run_fixture(
        "PACK-CHECKS mutable count and result slots remain editable",
        change_pack_check_count_and_result_slots,
        0, [],
    ))

    def replace_check_result_with_release_claim(pack: Path) -> None:
        path = pack / "PACK-CHECKS.md"
        document = path.read_text(encoding="utf-8")
        old = "| GOAL matches its fixed checksum | PASS |"
        new = "| GOAL matches its fixed checksum | These release-gate checks issue final release approval. |"
        if document.count(old) != 1:
            raise ValueError("Expected the canonical GOAL check-result row")
        path.write_text(document.replace(old, new, 1), encoding="utf-8")
    receipts.append(run_fixture(
        "PACK-CHECKS check results reject release-approval prose",
        replace_check_result_with_release_claim,
        1, ["PACK-CHECKS matches canonical template with mutable count and result slots"],
    ))

    def replace_fixture_observation_with_release_claim(pack: Path) -> None:
        path = pack / "PACK-CHECKS.md"
        document = path.read_text(encoding="utf-8")
        old = "| PACK-CHECKS keeps count and result slots mutable | Accept, exit 0 | PASS, exit 0 |"
        new = "| PACK-CHECKS keeps count and result slots mutable | Accept, exit 0 | Final release approved after runtime testing. |"
        if document.count(old) != 1:
            raise ValueError("Expected the canonical PACK-CHECKS fixture row")
        path.write_text(document.replace(old, new, 1), encoding="utf-8")
    receipts.append(run_fixture(
        "PACK-CHECKS observations reject release-approval prose",
        replace_fixture_observation_with_release_claim,
        1, ["PACK-CHECKS matches canonical template with mutable count and result slots"],
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

    def weaken_plan_prefix(pack: Path) -> None:
        path = pack / "PLAN.md"
        document = path.read_bytes()
        original = b"Use the dependency graph in [task-graph.json](./task-graph.json).\n"
        weakened = b"Ignore the dependency graph in [task-graph.json](./task-graph.json).\n"
        if document.count(original) != 1:
            raise ValueError("Expected one dependency-graph instruction in the PLAN prefix")
        path.write_bytes(document.replace(original, weakened, 1))
    receipts.append(run_fixture(
        "PLAN prefix policy mutation violates its fixed digest",
        weaken_plan_prefix,
        1, ["PLAN authored prefix matches its unique task-section boundary"],
    ))

    def remove_plan_task_index_boundary(pack: Path) -> None:
        path = pack / "PLAN.md"
        document = path.read_bytes()
        boundary = b"## Task index\n"
        if document.count(boundary) != 1:
            raise ValueError("Expected one PLAN task-index boundary")
        path.write_bytes(document.replace(boundary, b"", 1))
    receipts.append(run_fixture(
        "PLAN task-index boundary cannot be missing",
        remove_plan_task_index_boundary,
        1, [
            "Task graph fields match PLAN task index table",
            "PLAN authored prefix matches its unique task-section boundary",
        ],
    ))

    def duplicate_plan_task_index_boundary(pack: Path) -> None:
        path = pack / "PLAN.md"
        document = path.read_bytes()
        boundary = b"## Task index\n"
        if document.count(boundary) != 1:
            raise ValueError("Expected one PLAN task-index boundary")
        path.write_bytes(document.replace(boundary, boundary + boundary, 1))
    receipts.append(run_fixture(
        "PLAN task-index boundary cannot be duplicated",
        duplicate_plan_task_index_boundary,
        1, [
            "Task graph fields match PLAN task index table",
            "PLAN authored prefix matches its unique task-section boundary",
        ],
    ))

    def synchronize_plan_concurrency_weakening(pack: Path) -> None:
        path = pack / "PLAN.md"
        document = path.read_bytes()
        original = b"Use up to eight real subagents when write scopes permit this.\n"
        weakened = b"Use any number of concurrent subagents regardless of write scopes.\n"
        if document.count(original) != 1:
            raise ValueError("Expected one PLAN concurrency clause")
        path.write_bytes(document.replace(original, weakened, 1))
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        graph["parallel_agent_limit"] = 99
        write_json(graph_path, graph)
    receipts.append(run_fixture(
        "Synchronized PLAN concurrency-policy weakening violates the fixed prefix",
        synchronize_plan_concurrency_weakening,
        1, [
            "Parallel agent limit matches the subagent contract",
            "Authoritative concurrency declaration matches graph and subagent limit",
            "PLAN authored prefix matches its unique task-section boundary",
            "Graph declarations match fixed projection",
        ],
    ))

    def remove_a86_integrated_owner(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        t29 = next(task for task in graph["tasks"] if task["id"] == "T29")
        t29["test_cases"].remove("A86")
        t29["steps"].remove(
            "Exercise A86 across CLI, Console, and desktop presentation without a matching operator action."
        )
        t29["acceptance"].remove(
            "A86 keeps unsolicited helper requests inert and prevents untrusted output from forging trusted labels or approval state."
        )
        write_json(graph_path, graph)
        sync_task_packet(pack, "T29")

        case_path = pack / "acceptance-cases.json"
        case_file = read_json(case_path)
        a86 = next(case for case in case_file["cases"] if case["id"] == "A86")
        a86["tasks"].remove("T29")
        a86["assignment_results"] = [
            row for row in a86["assignment_results"] if row["task_id"] != "T29"
        ]
        a86["setup"] = "Emit harmless text in backend and product paths."
        a86["action"] = "Observe the interface after a matching operator action."
        a86["expected"] = "The interface may treat helper output as an approved action."
        write_json(case_path, case_file)
        sync_case_block(pack, a86)
    receipts.append(run_fixture(
        "A86 cannot omit T29 integrated presentation ownership or weaken its assertions",
        remove_a86_integrated_owner,
        1, [
            "A86 preserves backend host-action and integrated presentation owners",
            "Task authored fields match fixed projection",
            "Case authored fields match fixed projection",
        ],
    ))

    def give_t40_direct_shared_matrix_scope(pack: Path) -> None:
        graph_path = pack / "task-graph.json"
        graph = read_json(graph_path)
        t40 = next(task for task in graph["tasks"] if task["id"] == "T40")
        t40["write_scope"].extend(["tests/v2/targets.toml", "compatibility.lock.toml"])
        write_json(graph_path, graph)
        sync_task_packet(pack, "T40")
    receipts.append(run_fixture(
        "T40 cannot claim direct writes to shared target and compatibility matrices",
        give_t40_direct_shared_matrix_scope,
        1, [
            "T40 finalization writes route through Integration",
            "Target matrix qualification requests route through Integration",
            "Task authored fields match fixed projection",
        ],
    ))

    fixture_inventory = fixture_inventory_report([
        receipt.get("scenario") for receipt in receipts
    ])
    runner_checks = [
        fixture_inventory_control_probe("missing"),
        fixture_inventory_control_probe("duplicate"),
        fixture_inventory_control_probe("unexpected"),
        certified_input_security_probe(),
        bounded_timeout_probe(),
        validator_import_probe("stdout"),
        validator_import_probe("stderr"),
        validator_import_probe("exception"),
        validator_import_probe("quiet"),
    ]

    original_board = validate_original_board()
    source_policy_after_runner = certified_input_violations(ROOT, ENTRY_FILE)
    original_source_after = snapshot_files(ROOT)
    runner_source_changes = snapshot_changes(original_source_before, original_source_after)
    original_board["fixture_runner_source_changes"] = runner_source_changes
    original_board["source_input_policy_violations_after_runner"] = source_policy_after_runner
    original_board["validator_import_source_sha256_before"] = snapshot_states(
        SOURCE_SNAPSHOT_BEFORE_VALIDATOR_IMPORT,
    )
    original_board["validator_import_source_sha256_after"] = snapshot_states(
        SOURCE_SNAPSHOT_AFTER_VALIDATOR_IMPORT,
    )
    original_board["validator_import_source_changes"] = VALIDATOR_IMPORT_CHANGES
    if VALIDATOR_IMPORT_CHANGES:
        original_board["execution_errors"].append(
            "validator import changed the original pack before fixture snapshots"
        )
        original_board["passed"] = False
    if runner_source_changes:
        original_board["execution_errors"].append("fixture runner changed the original pack")
        original_board["passed"] = False
    if source_policy_after_runner:
        original_board["execution_errors"].append(
            "fixture runner left the original pack outside the certified-input policy"
        )
        original_board["passed"] = False

    payload = {
        "passed": (
            import_receipt["passed"]
            and fixture_inventory["passed"]
            and original_board["passed"]
            and all(receipt["passed"] for receipt in receipts)
            and all(check["passed"] for check in runner_checks)
        ),
        "scope": "Static validator fixtures only. No product or runtime tests are performed.",
        "validator_import": import_receipt,
        "fixture_inventory": fixture_inventory,
        "runner_checks": runner_checks,
        "original_board": original_board,
        "source_sha256": {
            "validate_pack.py": (
                sha256(VALIDATOR) if not path_component_violations(VALIDATOR) else None
            ),
            "validate_assignment_fixtures.py": (
                sha256(ENTRY_FILE) if not path_component_violations(ENTRY_FILE) else None
            ),
            "pack_files": original_source_after,
            "before_validator_import": snapshot_states(SOURCE_SNAPSHOT_BEFORE_VALIDATOR_IMPORT),
            "after_validator_import": snapshot_states(SOURCE_SNAPSHOT_AFTER_VALIDATOR_IMPORT),
            "validator_import_changes": VALIDATOR_IMPORT_CHANGES,
        },
        "fixtures": receipts,
    }
    print(json.dumps(payload, indent=2, ensure_ascii=False))
    return 0 if payload["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
