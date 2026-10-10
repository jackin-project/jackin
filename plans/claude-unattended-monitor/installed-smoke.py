#!/usr/bin/env python3
"""Exercise the installed passive usage CLI in private fixture state.

Requires an explicit installed binary directory and a JSON provenance manifest
with a full source commit and SHA-256 digests for `jackin` and
`jackin-usage-broker`. The script uses isolated HOME/config/data directories,
PATH tripwires, and a local HTTP proxy. It never runs successful auth
preparation or provider collection. PATH tripwires do not instrument native
Security Framework calls; this smoke test therefore does not claim OS-level
Keychain or egress tracing. The manifest is caller-supplied and is not
signature-verified.

The accepted manifest shape is `{"source_commit": "<40 hex chars>",
"binaries": [{"name": "jackin", "sha256": "<64 hex chars>"},
{"name": "jackin-usage-broker", "sha256": "<64 hex chars>"}]}`.
"""

from __future__ import annotations

import argparse
import errno
import hashlib
import http.server
import json
import os
import pathlib
import pty
import re
import select
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any


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
EXPECTED_PROTOCOL_VERSION = "v8"
EXPECTED_MONITOR_SCHEMA_VERSION = 4
EXPECTED_BINARY_NAMES = {"jackin", "jackin-usage-broker"}


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
        required=True,
        help="explicit directory containing the installed sibling binaries",
    )
    parser.add_argument(
        "--provenance-manifest",
        type=pathlib.Path,
        required=True,
        help="JSON manifest with source_commit and SHA-256 digests for both binaries",
    )
    args = parser.parse_args()

    try:
        bin_dir = args.bin_dir.expanduser().resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise SmokeFailure(f"cannot resolve installed binary directory: {error}") from error
    jackin = bin_dir / "jackin"
    broker = bin_dir / "jackin-usage-broker"
    for binary in (jackin, broker):
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise SmokeFailure(f"missing executable sibling binary: {binary}")

    manifest_path = args.provenance_manifest.expanduser()
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SmokeFailure(f"cannot read provenance manifest {manifest_path}: {error}") from error
    if not isinstance(manifest, dict):
        raise SmokeFailure("provenance manifest must be a JSON object")
    source_commit = manifest.get("source_commit")
    if not isinstance(source_commit, str) or re.fullmatch(r"[0-9a-f]{40}", source_commit) is None:
        raise SmokeFailure("provenance manifest source_commit must be 40 lowercase hex characters")
    manifest_binaries = manifest.get("binaries")
    if not isinstance(manifest_binaries, list):
        raise SmokeFailure("provenance manifest binaries must be a list")
    expected_digests: dict[str, str] = {}
    for record in manifest_binaries:
        if not isinstance(record, dict):
            raise SmokeFailure("each provenance binary record must be an object")
        name = record.get("name")
        digest = record.get("sha256")
        if not isinstance(name, str):
            raise SmokeFailure("each provenance binary record must have a string name")
        if name not in EXPECTED_BINARY_NAMES:
            continue
        if name in expected_digests:
            raise SmokeFailure(f"provenance manifest repeats binary {name!r}")
        if not isinstance(digest, str) or re.fullmatch(r"[0-9a-f]{64}", digest) is None:
            raise SmokeFailure(f"provenance manifest has an invalid SHA-256 for {name!r}")
        expected_digests[name] = digest
    if set(expected_digests) != EXPECTED_BINARY_NAMES:
        raise SmokeFailure("provenance manifest must contain both named installed binaries")

    def sha256_file(path: pathlib.Path) -> str:
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
        return digest.hexdigest()

    actual_digests = {
        "jackin": sha256_file(jackin),
        "jackin-usage-broker": sha256_file(broker),
    }
    manifest_digest = sha256_file(manifest_path)
    for name, expected_digest in expected_digests.items():
        if actual_digests[name] != expected_digest:
            raise SmokeFailure(f"installed {name} SHA-256 does not match the provenance manifest")

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

    def monitor_status(reply: dict[str, Any], label: str) -> dict[str, Any]:
        status = reply.get("status")
        if not isinstance(status, dict):
            raise SmokeFailure(f"{label} omitted its monitor status")
        if status.get("schema_version") != EXPECTED_MONITOR_SCHEMA_VERSION:
            raise SmokeFailure(
                f"{label} returned monitor schema {status.get('schema_version')!r}; "
                f"expected {EXPECTED_MONITOR_SCHEMA_VERSION}"
            )
        return status

    def error_code(reply: dict[str, Any], label: str) -> str:
        error = reply.get("error")
        code = error.get("code") if isinstance(error, dict) else None
        if not isinstance(code, str):
            raise SmokeFailure(f"{label} omitted its JSON error code")
        return code

    def assert_no_shell_or_proxy_activity(label: str) -> None:
        if tripwire_log.exists() and tripwire_log.stat().st_size:
            raise SmokeFailure(
                f"{label}: shell credential tripwire fired: {tripwire_log.read_text()}"
            )
        with ProxyCounter.requests_lock:
            requests = list(ProxyCounter.requests)
        if requests:
            raise SmokeFailure(f"{label}: local HTTP proxy saw requests: {requests}")

    def merge_statusline_patch_fixture(
        source_settings: pathlib.Path,
        patch: dict[str, Any],
        patched_fixture: pathlib.Path,
        secret_marker: str,
        label: str,
    ) -> dict[str, Any]:
        if set(patch) != {"statusLine"}:
            raise SmokeFailure(f"{label} returned keys beyond the statusLine merge patch")
        serialized_patch = json.dumps(patch)
        if secret_marker in serialized_patch:
            raise SmokeFailure(f"{label} echoed unrelated fixture environment data")
        status_line_patch = patch.get("statusLine")
        if not isinstance(status_line_patch, dict):
            raise SmokeFailure(f"{label} statusLine patch was not an object")

        original_bytes = source_settings.read_bytes()
        original_settings = json.loads(original_bytes)
        original_status_line = original_settings.get("statusLine")
        if not isinstance(original_status_line, dict):
            raise SmokeFailure(f"{label} source settings omitted the fixture statusLine object")
        for key, value in original_status_line.items():
            if key != "command" and status_line_patch.get(key) != value:
                raise SmokeFailure(f"{label} did not preserve statusLine.{key}")

        # Apply the proposed merge patch only to a copy inside the private fixture.
        shutil.copyfile(source_settings, patched_fixture)
        merged_settings = json.loads(patched_fixture.read_bytes())
        merged_settings.update(patch)
        patched_fixture.write_text(json.dumps(merged_settings), encoding="utf-8")
        if source_settings.read_bytes() != original_bytes:
            raise SmokeFailure(f"{label} changed the original fixture settings bytes")
        if merged_settings.get("env", {}).get("ANTHROPIC_API_KEY") != secret_marker:
            raise SmokeFailure(f"{label} fixture merge did not preserve its private sentinel")
        return merged_settings

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
        if lease.get("protocol_version") != EXPECTED_PROTOCOL_VERSION:
            raise SmokeFailure(
                f"broker owner lease protocol_version was {lease.get('protocol_version')!r}; "
                f"expected {EXPECTED_PROTOCOL_VERSION}"
            )
        process_id = lease.get("process_id")
        if not isinstance(process_id, int) or isinstance(process_id, bool) or process_id <= 1:
            raise SmokeFailure("broker owner lease omitted a valid process_id")
        if lease.get("build_id") != binary_versions[0]:
            raise SmokeFailure("broker owner lease build_id did not match the installed pair")
        # The build ID is only the package version; the manifest and binary
        # digests above identify this installed pair independently.

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
            "--config-root",
            str(config_dir),
            "--operator-home",
            str(home_dir),
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

    try:
        print(f"fixture_root={root}")
        print(f"provenance_manifest_source_commit={source_commit}")
        print(f"provenance_manifest_sha256={manifest_digest}")
        print(f"installed_jackin_sha256={actual_digests['jackin']}")
        print(f"installed_broker_sha256={actual_digests['jackin-usage-broker']}")
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

        for label, arguments, required_words in [
            (
                "monitor-start-help",
                usage("monitor", "start", "--help"),
                ("--binding", "--binding-revision", "--goal", "--policy-revision", "--idempotency-key"),
            ),
            (
                "monitor-observe-help",
                usage("monitor", "observe", "--help"),
                (
                    "--session",
                    "--binding",
                    "--binding-revision",
                    "--idempotency-key",
                    "--experimental-collector",
                    "undocumented endpoint",
                    "Jackin identity",
                    "policy is unchanged",
                ),
            ),
            (
                "auth-prepare-help",
                usage("auth", "prepare", "--help"),
                ("--provider", "--keychain-service"),
            ),
            (
                "binding-confirm-help",
                usage("binding", "confirm", "--help"),
                ("--provider-account", "--approve-experimental-collector"),
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
            if label in {
                "monitor-start-help",
                "monitor-observe-help",
                "statusline-ingest-help",
            }:
                for removed_word in ("--account", "--budget-sgd"):
                    if removed_word in result.stdout:
                        raise SmokeFailure(f"{label} retained removed option `{removed_word}`")

        auth = invoke(
            "headless-auth-prepare",
            usage(
                "auth",
                "prepare",
                "--provider",
                "claude",
                "--keychain-service",
                "jackin-offline-smoke-no-access",
            ),
            2,
        )
        auth_reply = json.loads(auth.stdout)
        if (
            auth_reply.get("version") != 1
            or error_code(auth_reply, "headless auth") != "interaction_required"
        ):
            raise SmokeFailure("headless auth preparation did not return interaction_required")
        if auth.stderr:
            raise SmokeFailure("headless auth preparation wrote interactive stderr")
        if (data_dir / "usage-broker" / "run").exists():
            raise SmokeFailure("headless auth preparation started the broker")
        assert_no_shell_or_proxy_activity("headless auth preparation")

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
            rejected_reply = json.loads(rejected.stdout)
            if (
                rejected_reply.get("version") != 1
                or error_code(rejected_reply, label) != "interaction_required"
            ):
                raise SmokeFailure(f"{label} did not return interaction_required")
            if (data_dir / "usage-broker" / "run").exists():
                raise SmokeFailure(f"{label} contacted or started a broker without TTYs")
            assert_no_shell_or_proxy_activity(label)

        unbound_experimental = invoke(
            "unbound-experimental-collector-rejected",
            usage(
                "monitor",
                "observe",
                "--provider",
                "claude",
                "--session",
                "installed-proof-experimental-session",
                "--idempotency-key",
                "installed-proof-unbound-experimental-1",
                "--experimental-collector",
            ),
            3,
        )
        unbound_experimental_reply = json.loads(unbound_experimental.stdout)
        if (
            unbound_experimental_reply.get("version") != 1
            or error_code(unbound_experimental_reply, "unbound experimental observe")
            != "invalid_argument"
        ):
            raise SmokeFailure("unbound experimental observe was not rejected as invalid_argument")
        if (data_dir / "usage-broker" / "run").exists():
            raise SmokeFailure("unbound experimental observe started the broker")
        assert_no_shell_or_proxy_activity("unbound experimental observe")

        bare_before_service = invoke("bare-usage-before-service", usage(), 3)
        bare_before_service_reply = json.loads(bare_before_service.stdout)
        if (
            error_code(bare_before_service_reply, "bare usage before service")
            != "broker_unavailable"
        ):
            raise SmokeFailure("bare usage did not report the missing passive projection")
        if (data_dir / "usage-broker" / "run").exists():
            raise SmokeFailure("bare usage started the broker or created its run directory")
        assert_no_shell_or_proxy_activity("bare usage before service")

        service_start_attempted = True
        service_start = invoke("service-start", usage("service", "start"), 0)
        service_status = json.loads(service_start.stdout)
        service_state = service_status.get("status", {})
        if service_state.get("running") is not True:
            raise SmokeFailure("service start did not report running=true")
        if service_state.get("experimental_collector_source") is not None:
            raise SmokeFailure("ordinary service start unexpectedly enabled a foreground collector")
        if service_state.get("active_monitors") != 0:
            raise SmokeFailure("fresh passive service started with unexpected monitors")
        verify_installed_broker_process()

        doctor = invoke(
            "doctor",
            usage("doctor", "--provider", "claude", "--unattended"),
            0,
        )
        doctor_reply = json.loads(doctor.stdout)
        if not doctor_reply.get("report", {}).get("broker_available"):
            raise SmokeFailure("doctor did not report broker_available=true")
        if doctor_reply.get("report", {}).get("auth_state") != "unknown":
            raise SmokeFailure("passive doctor inspected or changed authentication state")
        for repeat in range(2):
            repeated = invoke(
                f"doctor-repeat-{repeat + 1}",
                usage("doctor", "--provider", "claude", "--unattended"),
                0,
            )
            repeated_report = json.loads(repeated.stdout).get("report", {})
            if not repeated_report.get("broker_available"):
                raise SmokeFailure("repeated doctor did not report broker_available=true")
            if repeated_report.get("auth_state") != "unknown":
                raise SmokeFailure("repeated passive doctor inspected authentication state")

        passive_bare_usage = invoke("bare-usage-current-projection", usage(), 0)
        passive_projection = json.loads(passive_bare_usage.stdout)
        if not isinstance(passive_projection, dict) or not isinstance(
            passive_projection.get("providers"), list
        ):
            raise SmokeFailure("bare usage did not return the current projection")
        passive_service_status = json.loads(
            invoke("passive-service-status", usage("service", "status"), 0).stdout
        ).get("status", {})
        if (
            passive_service_status.get("running") is not True
            or passive_service_status.get("experimental_collector_source") is not None
        ):
            raise SmokeFailure("bare projection read changed passive service mode")

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
        observer_status = monitor_status(start_reply, "session observer start")
        monitor_id = observer_status["monitor_id"]
        if observer_status.get("purpose") != "observe_only":
            raise SmokeFailure("monitor observe created a non-observation monitor")
        if observer_status.get("scope", {}).get("scope") != "session":
            raise SmokeFailure("monitor observe did not keep its unbound session scope")
        if observer_status.get("account_id") is not None or observer_status.get("goal_id") is not None:
            raise SmokeFailure("unbound observer acquired an account or goal")
        if observer_status.get("budget") is not None:
            raise SmokeFailure("unbound observer acquired an SGD budget")
        if observer_status.get("policy") is not None:
            raise SmokeFailure("observation-only monitor unexpectedly has a dispatch policy")
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
        session_secret_marker = "jackin-session-fixture-secret-never-return"
        original_settings = {
            "theme": "dark",
            "env": {"ANTHROPIC_API_KEY": session_secret_marker},
            "statusLine": {"type": "command", "command": "printf existing"},
        }
        original_settings_bytes = json.dumps(original_settings).encode("utf-8")
        settings_file.write_bytes(original_settings_bytes)
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
        session_patch = json.loads(composed.stdout)
        merged_session_settings = merge_statusline_patch_fixture(
            settings_file,
            session_patch,
            config_dir / "claude-settings-session-patched.json",
            session_secret_marker,
            "session statusline compose",
        )
        if (
            "--session-only"
            not in merged_session_settings.get("statusLine", {}).get("command", "")
        ):
            raise SmokeFailure("statusline compose omitted the unbound session scope")
        if settings_file.read_bytes() != original_settings_bytes:
            raise SmokeFailure("statusline compose changed the source fixture settings bytes")

        repeated_start = invoke("monitor-observe-idempotent-repeat", observer_args, 0)
        repeated_status = monitor_status(
            json.loads(repeated_start.stdout), "session observer idempotent repeat"
        )
        if repeated_status["monitor_id"] != monitor_id:
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
        observer_status = monitor_status(status_reply, "session observer status")
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
            repeated_status = monitor_status(
                json.loads(repeated.stdout), f"session observer status repeat {repeat + 1}"
            )
            if repeated_status["monitor_id"] != monitor_id:
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
        if not events:
            raise SmokeFailure("watch did not emit the current monitor status as JSONL")
        first_watch_status = monitor_status(events[0], "session observer watch")
        if first_watch_status["monitor_id"] != monitor_id:
            raise SmokeFailure("watch emitted a different monitor ID")

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
        wait_status = monitor_status(wait_reply, "session observer wait")
        issue_codes = {issue["code"] for issue in wait_status["issues"]}
        if "wait_timeout" not in issue_codes:
            raise SmokeFailure("bounded wait did not return a wait_timeout issue")

        fixture_account = "installed-proof-observer-account"
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
            or binding.get("provider_account_id") is not None
            or binding.get("experimental_collector_approved") is not False
        ):
            raise SmokeFailure("fixture binding escaped its unmapped, unapproved private scope")

        before_collector_rejection_reply = json.loads(
            invoke(
                "service-status-before-experimental-rejection",
                usage("service", "status"),
                0,
            ).stdout
        )
        before_collector_rejection = before_collector_rejection_reply.get("status", {})
        active_monitors_before_collector_rejection = before_collector_rejection.get(
            "active_monitors"
        )
        if before_collector_rejection.get("experimental_collector_source") is not None:
            raise SmokeFailure("passive service unexpectedly exposes a collector source")
        if active_monitors_before_collector_rejection != 1:
            raise SmokeFailure("passive fixture should contain only its session observer")
        experimental_bound_observe = invoke(
            "passive-bound-experimental-collector-rejected",
            usage(
                "monitor",
                "observe",
                "--provider",
                "claude",
                "--binding",
                binding_id,
                "--binding-revision",
                str(binding_revision),
                "--idempotency-key",
                "installed-proof-passive-experimental-1",
                "--experimental-collector",
            ),
            3,
        )
        experimental_rejection_reply = json.loads(experimental_bound_observe.stdout)
        if (
            experimental_rejection_reply.get("version") != 1
            or error_code(experimental_rejection_reply, "passive experimental observe")
            != "collector_auth_required"
        ):
            raise SmokeFailure(
                "passive experimental observe did not require a foreground prepared-auth service"
            )
        service_after_collector_rejection = json.loads(
            invoke(
                "passive-service-status-after-experimental-rejection",
                usage("service", "status"),
                0,
            ).stdout
        ).get("status", {})
        if (
            service_after_collector_rejection.get("running") is not True
            or service_after_collector_rejection.get("experimental_collector_source") is not None
            or service_after_collector_rejection.get("active_monitors")
            != active_monitors_before_collector_rejection
        ):
            raise SmokeFailure(
                "rejected experimental observe changed service mode or created a monitor"
            )
        assert_no_shell_or_proxy_activity("passive experimental observe")

        bound_settings_file = config_dir / "claude-settings-bound.json"
        bound_secret_marker = "jackin-bound-fixture-secret-never-return"
        original_bound_settings = {
            "theme": "dark",
            "env": {"ANTHROPIC_API_KEY": bound_secret_marker},
            "statusLine": {"type": "command", "command": "printf existing"},
        }
        original_bound_settings_bytes = json.dumps(original_bound_settings).encode("utf-8")
        bound_settings_file.write_bytes(original_bound_settings_bytes)
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
        bound_patch = json.loads(bound_composed.stdout)
        merged_bound_settings = merge_statusline_patch_fixture(
            bound_settings_file,
            bound_patch,
            config_dir / "claude-settings-bound-patched.json",
            bound_secret_marker,
            "bound statusline compose",
        )
        bound_wrapper = merged_bound_settings.get("statusLine", {}).get("command", "")
        for preserved in ("printf existing", "--binding", binding_id, str(binding_revision)):
            if preserved not in bound_wrapper:
                raise SmokeFailure(f"bound statusline wrapper omitted {preserved!r}")
        if bound_settings_file.read_bytes() != original_bound_settings_bytes:
            raise SmokeFailure("bound statusline compose changed the source fixture bytes")

        bound_reset_epoch = int(time.time())
        bound_statusline = json.dumps(
            {
                "session_id": "installed-proof-bound-session",
                "transcript_path": str(root / "bound-session.jsonl"),
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
        if bound_settings_file.read_bytes() != original_bound_settings_bytes:
            raise SmokeFailure("running the bound wrapper changed the source fixture bytes")

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
        bound_observer_status = monitor_status(
            bound_observer_reply, "bound observer start"
        )
        bound_observer_id = bound_observer_status["monitor_id"]
        if (
            bound_observer_status.get("purpose") != "observe_only"
            or bound_observer_status.get("scope", {}).get("scope") != "bound_account"
            or bound_observer_status.get("scope", {}).get("binding_id") != binding_id
            or bound_observer_status.get("scope", {}).get("binding_revision") != binding_revision
            or bound_observer_status.get("account_id") != fixture_account
            or bound_observer_status.get("goal_id") is not None
            or bound_observer_status.get("budget") is not None
            or bound_observer_status.get("policy") is not None
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
        final_service_status = json.loads(
            invoke("passive-service-status-after-callback", usage("service", "status"), 0).stdout
        ).get("status", {})
        if (
            final_service_status.get("running") is not True
            or final_service_status.get("experimental_collector_source") is not None
            or final_service_status.get("active_monitors") != 2
        ):
            raise SmokeFailure("fixture observation changed the passive collector mode")
        for label, monitor_to_stop in (
            ("monitor-stop-observer", monitor_id),
            ("monitor-stop-bound-observer", bound_observer_id),
        ):
            invoke(label, usage("monitor", "stop", "--monitor", monitor_to_stop), 0)
        monitor_id = None
        bound_observer_id = None
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

        assert_no_shell_or_proxy_activity("passive installed smoke")

        print(f"watch_jsonl_events={len(events)}")
        print(f"shell_credential_trips=0 ({tripwire_log})")
        print("http_proxy_requests=0")
        print("native_security_framework_calls=not_instrumented")
        print("foreground_auth_success=not_exercised")
        print("dispatch_policy_approvals=none")
        print("installed_smoke=PASS")
    except BaseException as error:
        failure = error
    finally:
        # Cleanup uses the public stop operation and never kills a process.
        # Preserve the fixture and transcript if any command or cleanup fails.
        cleanup_errors: list[str] = []
        if service_start_attempted:
            for label, pending_monitor_id in (
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
