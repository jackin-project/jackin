#!/usr/bin/env python3
"""Exercise installed local-only usage CLI commands in private state.

Run only after installing matching `jackin` and `jackin-usage-broker` sibling
binaries. The script clears the child environment, keeps credential tripwires
and an HTTP proxy counter active, and never performs auth/provider work.
"""

from __future__ import annotations

import argparse
import http.server
import json
import os
import pathlib
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any


DEFAULT_BIN_DIR = pathlib.Path(
    "/Users/donbeave/.local/share/jackin-claude-monitor/bin"
)
EXPECTED_HELP = (
    "doctor",
    "service",
    "monitor",
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
    child_env = {
        "HOME": str(home_dir),
        "USER": os.environ.get("USER", "offline"),
        "LOGNAME": os.environ.get("LOGNAME", "offline"),
        "PATH": f"{tripwire_dir}:/usr/bin:/bin:/usr/sbin:/sbin",
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
    }
    # Deliberately omit JACKIN_USAGE_BROKER_BIN: the installed CLI must find
    # the broker as its sibling executable.
    assert "JACKIN_USAGE_BROKER_BIN" not in child_env

    monitor_id: str | None = None
    service_start_attempted = False
    failure: BaseException | None = None
    binary_versions: list[str] = []

    def invoke(label: str, arguments: list[str], expected_exit: int) -> subprocess.CompletedProcess[str]:
        command = [str(jackin), *arguments]
        completed = subprocess.run(
            command,
            env=child_env,
            input="",
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

    def usage(*arguments: str, fmt: str = "json") -> list[str]:
        return [
            "usage",
            "--format",
            fmt,
            "--data-dir",
            str(data_dir),
            *arguments,
        ]

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

        help_result = invoke("usage-help", ["usage", "--help"], 0)
        for word in EXPECTED_HELP:
            if word not in help_result.stdout:
                raise SmokeFailure(f"usage --help omitted `{word}`")

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

        service_start_attempted = True
        service_start = invoke("service-start", usage("service", "start"), 0)
        service_status = json.loads(service_start.stdout)
        if service_status.get("status", {}).get("running") is not True:
            raise SmokeFailure("service start did not report running=true")

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

        start = invoke(
            "monitor-start",
            usage(
                "monitor",
                "start",
                "--provider",
                "claude",
                "--account",
                "installed-proof-account",
                "--goal",
                "installed-proof-goal",
            ),
            2,
        )
        start_reply = json.loads(start.stdout)
        monitor_id = start_reply["status"]["monitor_id"]
        if start_reply["status"]["runnable"]:
            raise SmokeFailure("empty-evidence fixture unexpectedly became runnable")

        status = invoke("status", usage("status", "--monitor", monitor_id), 2)
        status_reply = json.loads(status.stdout)
        if status_reply["status"]["monitor_id"] != monitor_id:
            raise SmokeFailure("status returned a different monitor ID")
        for repeat in range(2):
            repeated = invoke(
                f"status-repeat-{repeat + 1}",
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

        invoke("monitor-stop", usage("monitor", "stop", "--monitor", monitor_id), 0)
        monitor_id = None
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
            if monitor_id is not None:
                try:
                    invoke(
                        "finally-monitor-stop",
                        usage("monitor", "stop", "--monitor", monitor_id),
                        0,
                    )
                except BaseException as error:
                    cleanup_errors.append(f"monitor stop: {error}")
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
