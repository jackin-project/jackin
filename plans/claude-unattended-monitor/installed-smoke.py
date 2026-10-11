#!/usr/bin/env python3
"""Exercise installed local-only usage CLI commands in private state.

Run only after installing matching `jackin` and `jackin-usage-broker` sibling
binaries. The script clears the child environment, keeps credential tripwires
and an HTTP proxy counter active, and never performs auth/provider work.
"""

from __future__ import annotations

import argparse
import errno
import http.server
import json
import os
import pathlib
import pty
import select
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any


DEFAULT_BIN_DIR = pathlib.Path(
    "/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin"
)
EXPECTED_HELP = (
    "doctor",
    "service",
    "monitor",
    "binding",
    "policy",
    "status",
    "refresh",
    "watch",
    "wait",
    "statusline",
    "spend",
    "auth",
)


class SmokeFailure(RuntimeError):
    pass


class ProxyCounter(http.server.BaseHTTPRequestHandler):
    requests: list[tuple[str, str]] = []
    requests_lock = threading.Lock()

    def _record(self) -> None:
        with self.requests_lock:
            self.requests.append((self.command, self.path))
        self.send_response(502)
        self.end_headers()

    do_CONNECT = _record
    do_DELETE = _record
    do_GET = _record
    do_PATCH = _record
    do_POST = _record
    do_PUT = _record

    def log_message(self, _format: str, *_args: Any) -> None:
        pass


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--bin-dir",
        type=pathlib.Path,
        default=DEFAULT_BIN_DIR,
        help=f"directory containing both installed binaries (default: {DEFAULT_BIN_DIR})",
    )
    args = parser.parse_args()

    jackin = args.bin_dir / "jackin"
    broker = args.bin_dir / "jackin-usage-broker"
    for binary in (jackin, broker):
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise SmokeFailure(f"missing executable sibling binary: {binary}")

    # A short root keeps the direct socket path inside macOS sun_path. A
    # separate broker fixture covers the long-path alias fallback.
    root = pathlib.Path(tempfile.mkdtemp(prefix="jkin-installed-smoke-", dir="/tmp"))
    data_dir = root / "data"
    home_dir = root / "home"
    config_dir = root / "config"
    tripwire_dir = root / "tripwire"
    for directory in (data_dir, home_dir, config_dir, tripwire_dir):
        directory.mkdir()

    tripwire_log = root / "credential-trips.log"
    command_log = root / "commands.log"
    for command in ("op", "claude", "security"):
        executable = tripwire_dir / command
        executable.write_text(
            "#!/bin/sh\n"
            f"printf '%s\\n' '{command}' >> '{tripwire_log}'\n"
            "exit 91\n",
            encoding="utf-8",
        )
        executable.chmod(0o755)

    ProxyCounter.requests = []
    proxy = http.server.ThreadingHTTPServer(("127.0.0.1", 0), ProxyCounter)
    proxy_thread = threading.Thread(target=proxy.serve_forever, daemon=True)
    proxy_thread.start()
    proxy_url = f"http://127.0.0.1:{proxy.server_port}"
    python3_path = shutil.which("python3") or sys.executable
    python3_dir = pathlib.Path(python3_path).absolute().parent
    child_env = {
        "HOME": str(home_dir),
        "USER": os.environ.get("USER", "offline"),
        "LOGNAME": os.environ.get("LOGNAME", "offline"),
        "PATH": f"{tripwire_dir}:{python3_dir}:/usr/bin:/bin:/usr/sbin:/sbin",
        "TMPDIR": "/tmp",
        "TMP": "/tmp",
        "TEMP": "/tmp",
        "JACKIN_HOME_DIR": str(home_dir),
        "JACKIN_CONFIG_DIR": str(config_dir),
        "HTTP_PROXY": proxy_url,
        "http_proxy": proxy_url,
        "HTTPS_PROXY": proxy_url,
        "https_proxy": proxy_url,
        "ALL_PROXY": proxy_url,
        "all_proxy": proxy_url,
        "NO_PROXY": "",
        "no_proxy": "",
        "NO_COLOR": "1",
        "TERM": "dumb",
    }
    # Deliberately omit JACKIN_USAGE_BROKER_BIN: the installed CLI must find
    # the broker as its sibling executable.
    assert "JACKIN_USAGE_BROKER_BIN" not in child_env

    monitor_id: str | None = None
    bound_observer_id: str | None = None
    quota_monitor_id: str | None = None
    service_start_attempted = False
    failure: BaseException | None = None
    binary_versions: list[str] = []

    def invoke(
        label: str,
        arguments: list[str],
        expected_exit: int,
        input_text: str = "",
    ) -> subprocess.CompletedProcess[str]:
        command = [str(jackin), *arguments]
        completed = subprocess.run(
            command,
            env=child_env,
            input=input_text,
            text=True,
            capture_output=True,
            timeout=20,
            check=False,
        )
        with command_log.open("a", encoding="utf-8") as log:
            log.write(f"$ {command!r}\n")
            log.write(f"exit={completed.returncode}\n")
            log.write(f"stdout:\n{completed.stdout}")
            log.write(f"stderr:\n{completed.stderr}\n")
        print(f"[{label}] exit={completed.returncode}")
        if completed.stdout:
            print(completed.stdout.rstrip())
        if completed.stderr:
            print(completed.stderr.rstrip(), file=sys.stderr)
        if completed.returncode != expected_exit:
            raise SmokeFailure(
                f"{label}: expected exit {expected_exit}, got {completed.returncode}; "
                f"fixture retained at {root}"
            )
        return completed

    def invoke_tty(
        label: str,
        arguments: list[str],
        expected_exit: int,
    ) -> subprocess.CompletedProcess[str]:
        command = [str(jackin), *arguments]
        master_fd, slave_fd = pty.openpty()
        stdio_fds = (slave_fd, os.dup(slave_fd), os.dup(slave_fd))
        if not all(os.isatty(fd) for fd in stdio_fds):
            os.close(master_fd)
            for fd in stdio_fds:
                os.close(fd)
            raise SmokeFailure("operator fixture PTY did not provide three TTY descriptors")

        process: subprocess.Popen[bytes] | None = None
        output = bytearray()
        try:
            # The same private PTY slave backs child stdin, stdout and stderr;
            # all three operator checks therefore see a terminal.
            process = subprocess.Popen(
                command,
                env=child_env,
                stdin=stdio_fds[0],
                stdout=stdio_fds[1],
                stderr=stdio_fds[2],
                close_fds=True,
            )
            for fd in stdio_fds:
                os.close(fd)
            deadline = time.monotonic() + 20
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    process.kill()
                    process.wait()
                    raise SmokeFailure(
                        f"{label}: PTY command exceeded 20 seconds; fixture retained at {root}"
                    )
                readable, _, _ = select.select([master_fd], [], [], min(remaining, 0.1))
                if readable:
                    try:
                        chunk = os.read(master_fd, 8192)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    output.extend(chunk)
                elif process.poll() is not None:
                    break
            return_code = process.wait(timeout=max(0.1, deadline - time.monotonic()))
        except BaseException:
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
            raise
        finally:
            os.close(master_fd)
            for fd in stdio_fds:
                try:
                    os.close(fd)
                except OSError:
                    pass

        stdout = output.decode("utf-8", errors="replace").replace("\r\n", "\n").replace("\r", "\n")
        completed = subprocess.CompletedProcess(command, return_code, stdout, "")
        with command_log.open("a", encoding="utf-8") as log:
            log.write(f"$ {command!r} [stdin/stdout/stderr=PTY]\n")
            log.write(f"exit={completed.returncode}\n")
            log.write(f"stdout/stderr:\n{completed.stdout}\n")
        print(f"[{label}] exit={completed.returncode} (all stdio attached to PTY)")
        if completed.stdout:
            print(completed.stdout.rstrip())
        if completed.returncode != expected_exit:
            raise SmokeFailure(
                f"{label}: expected exit {expected_exit}, got {completed.returncode}; "
                f"fixture retained at {root}"
            )
        return completed

    def invoke_wrapper(label: str, wrapper_command: str, input_text: str) -> subprocess.CompletedProcess[str]:
        command = ["/bin/sh", "-c", wrapper_command]
        completed = subprocess.run(
            command,
            env=child_env,
            input=input_text,
            text=True,
            capture_output=True,
            timeout=20,
            check=False,
        )
        with command_log.open("a", encoding="utf-8") as log:
            log.write(f"$ {command!r}\n")
            log.write(f"exit={completed.returncode}\n")
            log.write(f"stdout:\n{completed.stdout}")
            log.write(f"stderr:\n{completed.stderr}\n")
        print(f"[{label}] exit={completed.returncode}")
        if completed.stdout:
            print(completed.stdout.rstrip())
        if completed.stderr:
            print(completed.stderr.rstrip(), file=sys.stderr)
        if completed.returncode != 0:
            raise SmokeFailure(
                f"{label}: expected exit 0, got {completed.returncode}; fixture retained at {root}"
            )
        return completed

    def usage(*arguments: str, fmt: str = "json") -> list[str]:
        return [
            "usage",
            "--format",
            fmt,
            "--data-dir",
            str(data_dir),
            *arguments,
        ]

    def verify_installed_broker_process() -> None:
        # The service-status reply intentionally reports monitor state, not
        # process identity. Read the broker owner's private JSON lease to get
        # its PID, then verify the live process argv against the exact sibling
        # executable and private fixture arguments.
        lease_path = data_dir / "usage-broker" / "run" / "leader.pid"
        try:
            lease = json.loads(lease_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise SmokeFailure(f"cannot read broker owner lease {lease_path}: {error}") from error
        if not isinstance(lease, dict):
            raise SmokeFailure("broker owner lease JSON was not an object")
        process_id = lease.get("process_id")
        if not isinstance(process_id, int) or isinstance(process_id, bool) or process_id <= 1:
            raise SmokeFailure("broker owner lease omitted a valid process_id")
        if lease.get("build_id") != binary_versions[0]:
            raise SmokeFailure("broker owner lease build_id did not match the installed pair")

        ps = next(
            (
                candidate
                for candidate in (pathlib.Path("/bin/ps"), pathlib.Path("/usr/bin/ps"))
                if candidate.is_file()
            ),
            None,
        )
        if ps is None:
            raise SmokeFailure("could not find /bin/ps or /usr/bin/ps for process verification")
        process = subprocess.run(
            [str(ps), "-ww", "-p", str(process_id), "-o", "command="],
            env=child_env,
            text=True,
            capture_output=True,
            timeout=5,
            check=False,
        )
        if process.returncode != 0:
            raise SmokeFailure(
                f"ps could not inspect broker PID {process_id}: {process.stderr.strip()}"
            )
        try:
            argv = shlex.split(process.stdout.strip())
        except ValueError as error:
            raise SmokeFailure(
                f"could not parse broker PID {process_id} command: {error}"
            ) from error
        if not argv:
            raise SmokeFailure(f"ps returned no command for broker PID {process_id}")

        try:
            actual_executable = pathlib.Path(argv[0]).resolve(strict=True)
            expected_executable = broker.resolve(strict=True)
        except (OSError, RuntimeError) as error:
            raise SmokeFailure(f"could not resolve broker process executable: {error}") from error
        if actual_executable != expected_executable:
            raise SmokeFailure(
                f"broker PID {process_id} used {actual_executable}, expected installed sibling "
                f"{expected_executable}"
            )
        expected_arguments = [
            "--data-dir",
            str(data_dir),
            "--build-id",
            str(lease["build_id"]),
            "--local-only",
        ]
        if argv[1:] != expected_arguments:
            raise SmokeFailure(
                f"broker PID {process_id} arguments did not match the private fixture: {argv[1:]}"
            )

        with command_log.open("a", encoding="utf-8") as log:
            log.write("broker_process_selection=verified\n")
            log.write(f"broker_process_id={process_id}\n")
            log.write(f"broker_lease={lease_path}\n")
            log.write(f"broker_expected_executable={expected_executable}\n")
            log.write(f"broker_ps_executable={ps}\n")
            log.write(f"broker_ps_argv={argv!r}\n")
        print(f"broker_process_id={process_id}")
        print(f"broker_process_executable={actual_executable}")
        print("broker_process_selection=verified exact installed sibling invocation")

    def print_future_operator_commands() -> None:
        print("Future installed-pair command examples (run binding and policy in a TTY):")
        print("  # Use the binding ID/revision and policy revision returned by prior commands.")
        examples = [
            usage(
                "binding",
                "confirm",
                "--provider",
                "claude",
                "--account",
                "work-account",
                "--operator-label",
                "work account",
                "--confirm",
            ),
            usage(
                "policy",
                "approve",
                "--binding",
                "BINDING_ID",
                "--binding-revision",
                "1",
                "--goal",
                "coding-task",
                "--policy",
                "strict-sgd",
                "--budget-sgd",
                "50",
                "--operator-label",
                "work account",
                "--confirm",
            ),
            usage(
                "monitor",
                "start",
                "--provider",
                "claude",
                "--binding",
                "BINDING_ID",
                "--binding-revision",
                "1",
                "--goal",
                "coding-task",
                "--policy-revision",
                "1",
                "--idempotency-key",
                "coding-task-run-1",
            ),
            usage(
                "monitor",
                "observe",
                "--provider",
                "claude",
                "--session",
                "CLAUDE_SESSION_ID",
                "--idempotency-key",
                "session-observation-1",
            ),
            usage(
                "monitor",
                "observe",
                "--provider",
                "claude",
                "--binding",
                "BINDING_ID",
                "--binding-revision",
                "1",
                "--idempotency-key",
                "account-observation-1",
            ),
            usage("statusline", "ingest", "--session-only"),
            usage(
                "statusline",
                "compose",
                "--session-only",
                "--settings",
                "/Users/donbeave/.claude/settings.json",
            ),
            usage(
                "statusline",
                "ingest",
                "--binding",
                "BINDING_ID",
                "--binding-revision",
                "1",
            ),
            usage(
                "statusline",
                "compose",
                "--binding",
                "BINDING_ID",
                "--binding-revision",
                "1",
                "--settings",
                "/Users/donbeave/.claude/settings.json",
            ),
        ]
        for example in examples:
            print(f"  {shlex.join([str(jackin), *example])}")

    try:
        print(f"fixture_root={root}")
        print("JACKIN_USAGE_BROKER_BIN=unset")
        for label, binary in (("jackin-version", jackin), ("broker-version", broker)):
            result = subprocess.run(
                [str(binary), "--version"],
                env=child_env,
                input="",
                text=True,
                capture_output=True,
                timeout=10,
                check=False,
            )
            if result.returncode != 0 or not result.stdout.strip():
                raise SmokeFailure(f"{label} failed: {result.stdout}{result.stderr}")
            print(f"[{label}] {result.stdout.strip()}")
            with command_log.open("a", encoding="utf-8") as log:
                log.write(f"$ {binary} --version\n")
                log.write(f"exit={result.returncode}\n")
                log.write(f"stdout:\n{result.stdout}")
                log.write(f"stderr:\n{result.stderr}\n")
            binary_versions.append(result.stdout.strip().split()[-1])
        if len(binary_versions) != 2 or binary_versions[0] != binary_versions[1]:
            raise SmokeFailure(f"installed binary versions differ: {binary_versions}")

        print_future_operator_commands()

        help_result = invoke("usage-help", ["usage", "--help"], 0)
        for word in EXPECTED_HELP:
            if word not in help_result.stdout:
                raise SmokeFailure(f"usage --help omitted `{word}`")

        for label, arguments, required_words in [
            (
                "monitor-start-help",
                usage("monitor", "start", "--help"),
                ("--binding", "--binding-revision", "--goal", "--policy-revision", "--idempotency-key"),
            ),
            (
                "monitor-observe-help",
                usage("monitor", "observe", "--help"),
                ("--session", "--binding", "--binding-revision", "--idempotency-key"),
            ),
            (
                "statusline-ingest-help",
                usage("statusline", "ingest", "--help"),
                ("--session-only", "--binding", "--binding-revision"),
            ),
            (
                "statusline-compose-help",
                usage("statusline", "compose", "--help"),
                ("--settings", "--session-only", "--binding", "--binding-revision"),
            ),
        ]:
            result = invoke(label, arguments, 0)
            for word in required_words:
                if word not in result.stdout:
                    raise SmokeFailure(f"{label} omitted `{word}`")
            if label in {"monitor-start-help", "monitor-observe-help", "statusline-ingest-help"}:
                for removed_word in ("--account", "--budget-sgd"):
                    if removed_word in result.stdout:
                        raise SmokeFailure(f"{label} retained removed option `{removed_word}`")

        auth = invoke(
            "headless-auth-prepare",
            usage("auth", "prepare", "--provider", "claude"),
            2,
        )
        auth_reply = json.loads(auth.stdout)
        if auth_reply.get("error", {}).get("code") != "interaction_required":
            raise SmokeFailure("headless auth preparation did not return interaction_required")
        if auth.stderr:
            raise SmokeFailure("headless auth preparation wrote interactive stderr")
        if (data_dir / "usage-broker" / "run").exists():
            raise SmokeFailure("headless auth preparation started the broker")

        for label, arguments in [
            (
                "headless-binding-confirm",
                usage(
                    "binding",
                    "confirm",
                    "--provider",
                    "claude",
                    "--account",
                    "installed-proof-account",
                    "--operator-label",
                    "offline fixture",
                    "--confirm",
                ),
            ),
            (
                "headless-policy-approve",
                usage(
                    "policy",
                    "approve",
                    "--binding",
                    "fixture-binding",
                    "--binding-revision",
                    "1",
                    "--goal",
                    "installed-proof-goal",
                    "--policy",
                    "strict-sgd",
                    "--budget-sgd",
                    "50",
                    "--operator-label",
                    "offline fixture",
                    "--confirm",
                ),
            ),
        ]:
            rejected = invoke(label, arguments, 2)
            if json.loads(rejected.stdout).get("error", {}).get("code") != "interaction_required":
                raise SmokeFailure(f"{label} did not return interaction_required")
            if (data_dir / "usage-broker" / "run").exists():
                raise SmokeFailure(f"{label} contacted or started a broker without TTYs")

        service_start_attempted = True
        service_start = invoke("service-start", usage("service", "start"), 0)
        service_status = json.loads(service_start.stdout)
        if service_status.get("status", {}).get("running") is not True:
            raise SmokeFailure("service start did not report running=true")
        verify_installed_broker_process()

        doctor = invoke(
            "doctor",
            usage("doctor", "--provider", "claude", "--unattended"),
            0,
        )
        doctor_reply = json.loads(doctor.stdout)
        if not doctor_reply.get("report", {}).get("broker_available"):
            raise SmokeFailure("doctor did not report broker_available=true")
        for repeat in range(2):
            repeated = invoke(
                f"doctor-repeat-{repeat + 1}",
                usage("doctor", "--provider", "claude", "--unattended"),
                0,
            )
            if not json.loads(repeated.stdout).get("report", {}).get("broker_available"):
                raise SmokeFailure("repeated doctor did not report broker_available=true")

        observer_args = usage(
            "monitor",
            "observe",
            "--provider",
            "claude",
            "--session",
            "installed-proof-session",
            "--expected-model",
            "claude-sonnet-4-5",
            "--idempotency-key",
            "installed-proof-observer-1",
        )
        start = invoke("monitor-observe", observer_args, 0)
        start_reply = json.loads(start.stdout)
        monitor_id = start_reply["status"]["monitor_id"]
        observer_status = start_reply["status"]
        if observer_status.get("purpose") != "observe_only":
            raise SmokeFailure("monitor observe created a non-observation monitor")
        if observer_status.get("scope", {}).get("scope") != "session":
            raise SmokeFailure("monitor observe did not keep its unbound session scope")
        if observer_status.get("account_id") is not None or observer_status.get("goal_id") is not None:
            raise SmokeFailure("unbound observer acquired an account or goal")
        if observer_status.get("budget") is not None:
            raise SmokeFailure("unbound observer acquired an SGD budget")
        if observer_status.get("expected_model") != "claude-sonnet-4-5":
            raise SmokeFailure("observer status omitted its configured expected model")
        if observer_status.get("model_guard_validity") != "unknown":
            raise SmokeFailure("model guard should remain unknown before scoped evidence arrives")
        for window_name in ("five_hour", "seven_day"):
            if observer_status.get(window_name, {}).get("reset_validity") != "unknown":
                raise SmokeFailure(
                    f"{window_name} reset validity should be unknown before quota evidence arrives"
                )
        if observer_status.get("readiness", {}).get("dispatch") != "not_authorized":
            raise SmokeFailure("observation-only monitor acquired dispatch authority")
        if observer_status.get("runnable") is not False:
            raise SmokeFailure("observation-only monitor became runnable")

        settings_file = config_dir / "claude-settings.json"
        original_settings = {
            "theme": "dark",
            "statusLine": {"type": "command", "command": "printf existing"},
        }
        original_settings_text = json.dumps(original_settings)
        settings_file.write_text(original_settings_text, encoding="utf-8")
        composed = invoke(
            "statusline-compose-session-only",
            usage(
                "statusline",
                "compose",
                "--session-only",
                "--settings",
                str(settings_file),
            ),
            0,
        )
        proposed_settings = json.loads(composed.stdout)
        if proposed_settings.get("theme") != "dark":
            raise SmokeFailure("statusline compose did not preserve unrelated settings")
        if "--session-only" not in proposed_settings.get("statusLine", {}).get("command", ""):
            raise SmokeFailure("statusline compose omitted the unbound session scope")
        if settings_file.read_text(encoding="utf-8") != original_settings_text:
            raise SmokeFailure("statusline compose wrote the proposed settings file")

        repeated_start = invoke("monitor-observe-idempotent-repeat", observer_args, 0)
        if json.loads(repeated_start.stdout)["status"]["monitor_id"] != monitor_id:
            raise SmokeFailure("repeating the observer idempotency key created a second monitor")

        reset_epoch = int(time.time())
        statusline = json.dumps(
            {
                "session_id": "installed-proof-session",
                "transcript_path": "/tmp/installed-proof-session.jsonl",
                "cwd": str(home_dir),
                "model": {"id": "claude-sonnet-4-5", "display_name": "Sonnet 4.5"},
                "version": "2.1.80",
                "rate_limits": {
                    "five_hour": {"used_percentage": 12.34, "resets_at": reset_epoch + 3600},
                    "seven_day": {"used_percentage": 8.5, "resets_at": reset_epoch + 7200},
                },
            }
        )
        ingest = invoke(
            "statusline-ingest-session-only",
            usage("statusline", "ingest", "--session-only"),
            0,
            statusline,
        )
        ingest_reply = json.loads(ingest.stdout)
        if ingest_reply.get("scope", {}).get("scope") != "session":
            raise SmokeFailure("session-only statusline ingress did not stay unbound")
        if ingest_reply.get("account_id") is not None:
            raise SmokeFailure("session-only statusline ingress asserted an account")

        status = invoke("observer-status", usage("status", "--monitor", monitor_id), 2)
        status_reply = json.loads(status.stdout)
        observer_status = status_reply["status"]
        if observer_status.get("monitor_id") != monitor_id:
            raise SmokeFailure("status returned a different monitor ID")
        if observer_status.get("runnable") is not False:
            raise SmokeFailure("observer status reported runnable work")
        if observer_status.get("readiness", {}).get("dispatch") != "not_authorized":
            raise SmokeFailure("observer status reported dispatch authority")
        if (
            observer_status.get("expected_model") != "claude-sonnet-4-5"
            or observer_status.get("model") != "claude-sonnet-4-5"
            or observer_status.get("model_guard_validity") != "match"
        ):
            raise SmokeFailure("fresh session evidence did not match the configured model guard")
        for window_name in ("five_hour", "seven_day"):
            window = observer_status.get(window_name, {})
            if window.get("reset_validity") != "future":
                raise SmokeFailure(f"fresh {window_name} reset was not independently future")
        if observer_status.get("five_hour", {}).get("used_percentage_basis_points") != 1234:
            raise SmokeFailure("session-only ingress was not visible to its observer")
        for repeat in range(2):
            repeated = invoke(
                f"observer-status-repeat-{repeat + 1}",
                usage("status", "--monitor", monitor_id),
                2,
            )
            if json.loads(repeated.stdout)["status"]["monitor_id"] != monitor_id:
                raise SmokeFailure("repeated status returned a different monitor ID")

        watch = invoke(
            "watch",
            usage(
                "watch",
                "--monitor",
                monitor_id,
                "--timeout-secs",
                "1",
                fmt="jsonl",
            ),
            0,
        )
        events = [json.loads(line) for line in watch.stdout.splitlines() if line.strip()]
        if not events or events[0]["status"]["monitor_id"] != monitor_id:
            raise SmokeFailure("watch did not emit the current monitor status as JSONL")

        wait = invoke(
            "wait",
            usage(
                "wait",
                "--monitor",
                monitor_id,
                "--until",
                "runnable",
                "--timeout-secs",
                "1",
            ),
            2,
        )
        wait_reply = json.loads(wait.stdout)
        issue_codes = {issue["code"] for issue in wait_reply["status"]["issues"]}
        if "wait_timeout" not in issue_codes:
            raise SmokeFailure("bounded wait did not return a wait_timeout issue")

        fixture_account = "installed-proof-quota-only-account"
        fixture_goal = "installed-proof-quota-only-goal"
        operator_label = "installed smoke fixture"
        binding_reply = invoke_tty(
            "fixture-binding-confirm-tty",
            usage(
                "binding",
                "confirm",
                "--provider",
                "claude",
                "--account",
                fixture_account,
                "--operator-label",
                operator_label,
                "--confirm",
            ),
            0,
        )
        binding_response = json.loads(binding_reply.stdout)
        if binding_response.get("result") != "account_bound":
            raise SmokeFailure("TTY binding confirmation did not return account_bound")
        binding = binding_response.get("binding")
        if not isinstance(binding, dict):
            raise SmokeFailure("TTY binding confirmation omitted its binding record")
        binding_id = binding.get("binding_id")
        binding_revision = binding.get("revision")
        if not isinstance(binding_id, str) or not isinstance(binding_revision, int):
            raise SmokeFailure("TTY binding confirmation returned an invalid ID or revision")
        if (
            binding.get("provider") != "claude"
            or binding.get("account_id") != fixture_account
            or binding.get("operator_confirmed") is not True
        ):
            raise SmokeFailure("fixture binding escaped its private account scope")

        policy_reply = invoke_tty(
            "fixture-quota-only-policy-approve-tty",
            usage(
                "policy",
                "approve",
                "--binding",
                binding_id,
                "--binding-revision",
                str(binding_revision),
                "--goal",
                fixture_goal,
                "--policy",
                "quota-only",
                "--operator-label",
                operator_label,
                "--confirm",
                "--acknowledge-no-sgd-cap",
            ),
            0,
        )
        policy_response = json.loads(policy_reply.stdout)
        if policy_response.get("result") != "policy_approved":
            raise SmokeFailure("TTY quota-only approval did not return policy_approved")
        approved_policy = policy_response.get("policy")
        if not isinstance(approved_policy, dict):
            raise SmokeFailure("TTY quota-only approval omitted its policy record")
        policy_revision = approved_policy.get("revision")
        if not isinstance(policy_revision, int) or policy_revision < 1:
            raise SmokeFailure("quota-only policy approval returned an invalid revision")
        if (
            approved_policy.get("goal_id") != fixture_goal
            or approved_policy.get("account_id") != fixture_account
            or approved_policy.get("binding_id") != binding_id
            or approved_policy.get("binding_revision") != binding_revision
            or approved_policy.get("new_policy") != "quota_only"
            or approved_policy.get("previous_policy") is not None
            or approved_policy.get("budget") is not None
            or approved_policy.get("origin") != "operator"
            or approved_policy.get("operator_label") != operator_label
            or approved_policy.get("acknowledge_no_sgd_cap") is not True
            or approved_policy.get("operator_confirmed") is not True
        ):
            raise SmokeFailure(
                "quota-only approval did not record a new, explicitly acknowledged fixture policy"
            )

        bound_settings_file = config_dir / "claude-settings-bound.json"
        original_bound_settings = {
            "theme": "dark",
            "statusLine": {"type": "command", "command": "printf existing"},
        }
        original_bound_settings_text = json.dumps(original_bound_settings)
        bound_settings_file.write_text(original_bound_settings_text, encoding="utf-8")
        bound_composed = invoke(
            "statusline-compose-bound-fixture",
            usage(
                "statusline",
                "compose",
                "--binding",
                binding_id,
                "--binding-revision",
                str(binding_revision),
                "--settings",
                str(bound_settings_file),
            ),
            0,
        )
        bound_settings = json.loads(bound_composed.stdout)
        bound_wrapper = bound_settings.get("statusLine", {}).get("command", "")
        if bound_settings.get("theme") != "dark":
            raise SmokeFailure("bound statusline compose lost unrelated fixture settings")
        for preserved in ("printf existing", "--binding", binding_id, str(binding_revision)):
            if preserved not in bound_wrapper:
                raise SmokeFailure(f"bound statusline wrapper omitted {preserved!r}")
        if bound_settings_file.read_text(encoding="utf-8") != original_bound_settings_text:
            raise SmokeFailure("bound statusline compose wrote its private settings file")

        bound_reset_epoch = int(time.time())
        bound_statusline = json.dumps(
            {
                "session_id": "installed-proof-quota-only-session",
                "transcript_path": str(root / "quota-only-session.jsonl"),
                "cwd": str(home_dir),
                "model": {"id": "claude-sonnet-4-5", "display_name": "Sonnet 4.5"},
                "version": "2.1.80",
                "rate_limits": {
                    "five_hour": {
                        "used_percentage": 12.34,
                        "resets_at": bound_reset_epoch + 3600,
                    },
                    "seven_day": {
                        "used_percentage": 8.5,
                        "resets_at": bound_reset_epoch + 7200,
                    },
                },
            }
        )
        wrapped_statusline = invoke_wrapper(
            "statusline-bound-wrapper-fixture",
            bound_wrapper,
            bound_statusline,
        )
        if "existing" not in wrapped_statusline.stdout:
            raise SmokeFailure("bound statusline wrapper did not preserve its existing command")
        if bound_settings_file.read_text(encoding="utf-8") != original_bound_settings_text:
            raise SmokeFailure("running the bound wrapper changed its private settings file")

        bound_observer_args = usage(
            "monitor",
            "observe",
            "--provider",
            "claude",
            "--binding",
            binding_id,
            "--binding-revision",
            str(binding_revision),
            "--idempotency-key",
            "installed-proof-bound-observer-1",
        )
        bound_observer = invoke("monitor-observe-bound-fixture", bound_observer_args, 0)
        bound_observer_reply = json.loads(bound_observer.stdout)
        bound_observer_id = bound_observer_reply["status"]["monitor_id"]
        bound_observer_status = bound_observer_reply["status"]
        if (
            bound_observer_status.get("purpose") != "observe_only"
            or bound_observer_status.get("scope", {}).get("scope") != "bound_account"
            or bound_observer_status.get("scope", {}).get("binding_id") != binding_id
            or bound_observer_status.get("scope", {}).get("binding_revision") != binding_revision
            or bound_observer_status.get("account_id") != fixture_account
            or bound_observer_status.get("goal_id") is not None
            or bound_observer_status.get("budget") is not None
            or bound_observer_status.get("readiness", {}).get("dispatch") != "not_authorized"
            or bound_observer_status.get("runnable") is not False
        ):
            raise SmokeFailure("bound observer did not remain unbound from goals and dispatch")
        if bound_observer_status.get("readiness", {}).get("quota") != "ready":
            raise SmokeFailure("bound observer did not see the fresh wrapped quota observation")
        if (
            bound_observer_status.get("five_hour", {}).get("used_percentage_basis_points") != 1234
            or bound_observer_status.get("seven_day", {}).get("used_percentage_basis_points") != 850
        ):
            raise SmokeFailure("bound observer did not preserve the fixture quota values")
        quota_start_args = usage(
            "monitor",
            "start",
            "--provider",
            "claude",
            "--binding",
            binding_id,
            "--binding-revision",
            str(binding_revision),
            "--goal",
            fixture_goal,
            "--policy-revision",
            str(policy_revision),
            "--idempotency-key",
            "installed-proof-quota-only-run-1",
        )
        quota_start = invoke("monitor-start-approved-quota-only", quota_start_args, 0)
        quota_start_reply = json.loads(quota_start.stdout)
        quota_status = quota_start_reply.get("status", {})
        quota_monitor_id = quota_status.get("monitor_id")
        if quota_start_reply.get("result") != "started" or not isinstance(quota_monitor_id, str):
            raise SmokeFailure("approved quota-only activation omitted its monitor ID")
        if (
            quota_status.get("purpose") != "dispatch_guard"
            or quota_status.get("scope", {}).get("scope") != "bound_account"
            or quota_status.get("account_id") != fixture_account
            or quota_status.get("goal_id") != fixture_goal
            or quota_status.get("policy", {}).get("new_policy") != "quota_only"
            or quota_status.get("policy", {}).get("revision") != policy_revision
            or quota_status.get("policy", {}).get("acknowledge_no_sgd_cap") is not True
            or quota_status.get("readiness", {}).get("tracking") != "ready"
            or quota_status.get("readiness", {}).get("quota") != "ready"
            or quota_status.get("readiness", {}).get("budget") != "disabled"
            or quota_status.get("readiness", {}).get("dispatch") != "ready"
            or quota_status.get("runnable") is not True
        ):
            raise SmokeFailure("approved quota-only fixture did not become runnable from fresh quota")
        if (
            quota_status.get("budget") is not None
            or quota_status.get("spend_period_baseline") is not None
            or quota_status.get("cumulative_goal_spend") is not None
        ):
            raise SmokeFailure("quota-only status implied an SGD budget or known spend total")
        print("quota_only_spend_enforcement=disabled; spend=unknown")

        repeated_quota_start = invoke(
            "monitor-start-approved-quota-only-idempotent-repeat",
            quota_start_args,
            0,
        )
        repeated_quota_status = json.loads(repeated_quota_start.stdout)["status"]
        if repeated_quota_status.get("monitor_id") != quota_monitor_id:
            raise SmokeFailure("repeating the quota-only idempotency key created a second monitor")
        if repeated_quota_status.get("runnable") is not True:
            raise SmokeFailure("idempotent quota-only repeat lost its runnable status")

        quota_watch = invoke(
            "watch-approved-quota-only",
            usage(
                "watch",
                "--monitor",
                quota_monitor_id,
                "--timeout-secs",
                "1",
                fmt="jsonl",
            ),
            0,
        )
        quota_events = [
            json.loads(line) for line in quota_watch.stdout.splitlines() if line.strip()
        ]
        if (
            not quota_events
            or quota_events[0].get("status", {}).get("monitor_id") != quota_monitor_id
            or quota_events[0].get("status", {}).get("runnable") is not True
        ):
            raise SmokeFailure("bounded watch did not passively report the current quota-only state")
        quota_evidence_received_at = (
            quota_status.get("five_hour", {})
            .get("used_evidence", {})
            .get("evidence_received_at_epoch")
        )
        if (
            quota_events[0]["status"]
            .get("five_hour", {})
            .get("used_evidence", {})
            .get("evidence_received_at_epoch")
            != quota_evidence_received_at
        ):
            raise SmokeFailure("passive watch renewed quota evidence without a statusline callback")

        quota_wait_started_at = time.monotonic()
        quota_wait = invoke(
            "wait-approved-quota-only-runnable",
            usage(
                "wait",
                "--monitor",
                quota_monitor_id,
                "--until",
                "runnable",
                "--timeout-secs",
                "1",
            ),
            0,
        )
        if time.monotonic() - quota_wait_started_at >= 3:
            raise SmokeFailure("bounded wait exceeded its offline time limit")
        if json.loads(quota_wait.stdout).get("status", {}).get("runnable") is not True:
            raise SmokeFailure("bounded wait did not return the runnable quota-only state")

        for label, monitor_to_stop in (
            ("monitor-stop-observer", monitor_id),
            ("monitor-stop-bound-observer", bound_observer_id),
            ("monitor-stop-quota-only", quota_monitor_id),
        ):
            invoke(label, usage("monitor", "stop", "--monitor", monitor_to_stop), 0)
        monitor_id = None
        bound_observer_id = None
        quota_monitor_id = None
        invoke("service-status", usage("service", "status"), 0)
        service_stop = invoke("service-stop", usage("service", "stop"), 0)
        if json.loads(service_stop.stdout).get("result") != "service_stopped":
            raise SmokeFailure("service stop did not report service_stopped")
        service_start_attempted = False

        run_dir = data_dir / "usage-broker" / "run"
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and run_dir.exists() and any(run_dir.iterdir()):
            time.sleep(0.05)
        if run_dir.exists() and any(run_dir.iterdir()):
            raise SmokeFailure(f"broker run files remain after stop: {list(run_dir.iterdir())}")

        if tripwire_log.exists() and tripwire_log.stat().st_size:
            raise SmokeFailure(f"credential tripwire fired: {tripwire_log.read_text()}")
        with ProxyCounter.requests_lock:
            proxy_requests = list(ProxyCounter.requests)
        if proxy_requests:
            raise SmokeFailure(f"local HTTP proxy saw requests: {proxy_requests}")

        print(f"watch_jsonl_events={len(events)}")
        print(f"credential_trips=0 ({tripwire_log})")
        print("http_proxy_requests=0")
        print("installed_smoke=PASS")
    except BaseException as error:
        failure = error
    finally:
        # Cleanup uses the public stop operation and never kills a process.
        # Preserve the fixture and transcript if any command or cleanup fails.
        cleanup_errors: list[str] = []
        if service_start_attempted:
            for label, pending_monitor_id in (
                ("finally-quota-only-monitor-stop", quota_monitor_id),
                ("finally-bound-observer-stop", bound_observer_id),
                ("finally-monitor-stop", monitor_id),
            ):
                if pending_monitor_id is not None:
                    try:
                        invoke(
                            label,
                            usage("monitor", "stop", "--monitor", pending_monitor_id),
                            0,
                        )
                    except BaseException as error:
                        cleanup_errors.append(f"{label}: {error}")
            try:
                invoke("finally-service-stop", usage("service", "stop"), 0)
            except BaseException as error:
                cleanup_errors.append(f"service stop: {error}")
        proxy.shutdown()
        proxy.server_close()
        proxy_thread.join(timeout=2)

        if cleanup_errors:
            failure = failure or SmokeFailure("; ".join(cleanup_errors))
        if failure is None:
            shutil.rmtree(root)
            print("fixture_removed_after_orderly_stop=true")
        else:
            print(f"installed_smoke=FAIL: {failure}", file=sys.stderr)
            print(f"fixture_preserved={root}", file=sys.stderr)

    return 0 if failure is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
