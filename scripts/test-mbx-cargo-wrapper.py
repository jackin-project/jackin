#!/usr/bin/env python3
"""Exercise the pinned Mise Cargo wrapper with fake Cargo/compiler children.

Run only in the MBX source-rebind sandbox, with network disabled. The harness
creates all mutable state below one mode-0700 temporary directory and never
invokes a real Cargo or Rust compiler. Its Rustc probe calls the configured MBX
shim with ``--version`` and a fake compiler, so it proves the shim path was
invoked, not that an actual compile was intercepted.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import resource
import shutil
import signal
import subprocess
import sys
import tempfile
import textwrap
import time
import tomllib
from pathlib import Path
from typing import Any


EXPECTED_MISE_VERSION = "2026.10.1"
EXPECTED_MISE_SHA256 = "31e6859cf639ed4594906da3fcd0fe2055e9daddae75e9786dbe50b3fb3c0f4a"
EXPECTED_MBX_VERSION = "1.22.0"
EXPECTED_MBX_SHA256 = "124a1c7d856c2355e1a53b1e9bd04342d952eb6845b7edf7ac81d0eb1766f296"
PROCESS_TIMEOUT_SECONDS = 25
TERM_GRACE_SECONDS = 2
KILL_GRACE_SECONDS = 2
MAX_EVENT_LOG_BYTES = 1_000_000
MAX_CAPTURE_BYTES = 64_000
ACTIVE_CHILDREN: dict[int, subprocess.Popen[bytes]] = {}

REPO_ROOT = Path(__file__).resolve().parents[1]
EVENT_LOG_ENV = "MBX_FIXTURE_EVENT_LOG"


class HarnessFailure(Exception):
    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def require(condition: bool, code: str) -> None:
    if not condition:
        raise HarnessFailure(code)


def within(path: Path, parent: Path) -> bool:
    try:
        path.resolve().relative_to(parent.resolve())
        return True
    except ValueError:
        return False


def lexically_within(path: Path, parent: Path) -> bool:
    try:
        Path(os.path.abspath(path)).relative_to(Path(os.path.abspath(parent)))
        return True
    except ValueError:
        return False


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def require_immutable_executable(path: Path, code: str) -> Path:
    require(path.is_absolute(), f"{code}-not-absolute")
    require(".." not in path.parts and "." not in path.parts, f"{code}-noncanonical-path")
    current = Path(path.anchor)
    for index, part in enumerate(path.parts[1:]):
        current = current / part
        try:
            info = current.lstat()
        except OSError as error:
            raise HarnessFailure(f"{code}-missing") from error
        require(not current.is_symlink(), f"{code}-path-symlink")
        require(info.st_uid == 0, f"{code}-not-root-owned")
        require(info.st_mode & 0o022 == 0, f"{code}-path-group-or-world-writable")
        if index < len(path.parts[1:]) - 1:
            require(current.is_dir(), f"{code}-parent-not-directory")
    require(path.is_file(), f"{code}-not-file")
    info = path.stat()
    require(info.st_mode & 0o111 != 0, f"{code}-not-executable")
    require(info.st_mode & 0o222 == 0, f"{code}-writable")
    return path.resolve(strict=True)


def read_private(path: Path, limit: int = 64_000) -> str:
    try:
        if path.stat().st_size > limit:
            raise HarnessFailure("private-capture-too-large")
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError as error:
        raise HarnessFailure("private-capture-unreadable") from error


def kill_process_group(process: subprocess.Popen[bytes], sig: int) -> None:
    if os.name != "posix":
        if process.poll() is None:
            process.send_signal(sig)
        return
    try:
        os.killpg(process.pid, sig)
    except ProcessLookupError:
        pass


def stop_process_group(process: subprocess.Popen[bytes]) -> None:
    process_group = process.pid
    kill_process_group(process, signal.SIGTERM)
    deadline = time.monotonic() + TERM_GRACE_SECONDS
    while time.monotonic() < deadline and process_group_exists(process):
        if process.poll() is None:
            try:
                process.wait(timeout=0.05)
            except subprocess.TimeoutExpired:
                pass
        time.sleep(0.05)
    if not process_group_exists(process):
        return
    kill_process_group(process, signal.SIGKILL)
    deadline = time.monotonic() + KILL_GRACE_SECONDS
    while time.monotonic() < deadline and process_group_exists(process):
        if process.poll() is None:
            try:
                process.wait(timeout=0.05)
            except subprocess.TimeoutExpired:
                pass
        time.sleep(0.05)
    if process_group_exists(process):
        raise HarnessFailure("child-not-reaped-after-kill")
    if process.poll() is None:
        try:
            process.wait(timeout=0.1)
        except subprocess.TimeoutExpired as error:
            raise HarnessFailure("child-not-reaped-after-kill") from error


def process_group_exists(process: subprocess.Popen[bytes]) -> bool:
    if os.name != "posix":
        return process.poll() is None
    try:
        os.killpg(process.pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def run_bounded(
    argv: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    capture_dir: Path,
    label: str,
    timeout_seconds: int = PROCESS_TIMEOUT_SECONDS,
) -> tuple[int, str, str]:
    stdout_path = capture_dir / f"{label}.stdout"
    stderr_path = capture_dir / f"{label}.stderr"
    with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
        os.chmod(stdout_path, 0o600)
        os.chmod(stderr_path, 0o600)
        try:
            def limit_capture_file() -> None:
                resource.setrlimit(
                    resource.RLIMIT_FSIZE,
                    (MAX_CAPTURE_BYTES, MAX_CAPTURE_BYTES),
                )

            process = subprocess.Popen(
                argv,
                cwd=cwd,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                start_new_session=(os.name == "posix"),
                preexec_fn=limit_capture_file,
            )
        except OSError as error:
            raise HarnessFailure(f"{label}-spawn-failed") from error
        ACTIVE_CHILDREN[process.pid] = process
        try:
            code = process.wait(timeout=timeout_seconds)
        except subprocess.TimeoutExpired as error:
            stop_process_group(process)
            raise HarnessFailure(f"{label}-timed-out") from error
        finally:
            ACTIVE_CHILDREN.pop(process.pid, None)
        if process_group_exists(process):
            stop_process_group(process)
            raise HarnessFailure(f"{label}-left-descendant")
    return code, read_private(stdout_path), read_private(stderr_path)


def assert_production_contract() -> None:
    try:
        project = tomllib.loads((REPO_ROOT / "mise.toml").read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise HarnessFailure("production-mise-config-invalid") from error
    tools = project.get("tools", {})
    wrapper = project.get("wrappers", {}).get("cargo", {})
    require(tools.get("mr-boxington") == EXPECTED_MBX_VERSION, "production-mbx-pin-mismatch")
    require(wrapper.get("command") == "mbx", "production-cargo-wrapper-mismatch")
    require(
        wrapper.get("env") == {"MBX_CARGO_SHIM_MODE": "1"},
        "production-wrapper-mode-mismatch",
    )


def write_executable(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")
    os.chmod(path, 0o700)


def fake_programs(python: str) -> tuple[str, str, str, str]:
    common = r"""
import json, os, pathlib, shutil, signal, subprocess, sys, time

def run_id(argv):
    for item in argv:
        if item.startswith("--fixture-run-id="):
            return item.split("=", 1)[1]
    return os.environ.get("MBX_FIXTURE_RUN_ID", "")

def event(kind, argv, **extra):
    wrapper = os.environ.get("RUSTC_WRAPPER", "")
    socket_path = os.environ.get("MBX_SOCKET", "")
    item = {
        "kind": kind,
        "executable": sys.argv[0],
        "run_id": run_id(argv),
        "argv": list(argv),
        "socket": socket_path,
        "socket_exists": bool(socket_path and pathlib.Path(socket_path).exists()),
        "rustc_wrapper": wrapper,
        "wrapper_exists": bool(wrapper and pathlib.Path(wrapper).exists()),
        "cargo_env": os.environ.get("CARGO", ""),
        "shim_mode_leaked": "MBX_CARGO_SHIM_MODE" in os.environ,
    }
    item.update(extra)
    record = pathlib.Path(os.environ["MBX_FIXTURE_EVENT_LOG"])
    encoded = json.dumps(item, sort_keys=True) + "\n"
    if record.stat().st_size + len(encoded.encode("utf-8")) > 1_000_000:
        raise SystemExit(96)
    with record.open("a", encoding="utf-8") as output:
        output.write(encoded)

def fixture_metadata():
    root = pathlib.Path(os.environ["MBX_FIXTURE_ROOT"]).resolve()
    manifest = root / "Cargo.toml"
    source = root / "src" / "lib.rs"
    package_id = f"path+file://{root}#mbx-wrapper-fixture@0.1.0"
    target_dir = pathlib.Path(os.environ["MBX_FIXTURE_TARGET"]).resolve()
    package = {
        "name": "mbx-wrapper-fixture",
        "version": "0.1.0",
        "id": package_id,
        "license": None,
        "license_file": None,
        "description": None,
        "source": None,
        "dependencies": [],
        "targets": [{
            "kind": ["lib"], "crate_types": ["lib"],
            "name": "mbx_wrapper_fixture", "src_path": str(source),
            "edition": "2024", "doc": True, "doctest": True, "test": True,
        }],
        "features": {},
        "manifest_path": str(manifest),
        "metadata": None,
        "publish": None,
        "authors": [],
        "categories": [],
        "keywords": [],
        "readme": None,
        "repository": None,
        "homepage": None,
        "documentation": None,
        "edition": "2024",
        "links": None,
        "default_run": None,
        "rust_version": None,
    }
    return {
        "packages": [package],
        "workspace_members": [package_id],
        "workspace_default_members": [package_id],
        "resolve": None,
        "target_directory": str(target_dir),
        "build_directory": str(target_dir / "build"),
        "version": 1,
        "workspace_root": str(root),
        "metadata": None,
    }
"""

    cargo_body = r"""
original_args = sys.argv[1:]
event("cargo-entry", original_args)
args = list(original_args)
if args and args[0] == "xtask":
    alias_file = pathlib.Path(os.environ["MBX_FIXTURE_ROOT"]) / ".cargo" / "config.toml"
    alias_config = alias_file.read_text(encoding="utf-8")
    if '[alias]' not in alias_config or 'xtask = "build"' not in alias_config:
        raise SystemExit(95)
    args[0] = "build"
event("cargo", args)
if args in (["--version"], ["-V"], ["-v"]):
    print("cargo 1.97.1 (fixture)")
    raise SystemExit(0)
if args and args[0] in ("--version", "-V"):
    print("cargo 1.97.1 (fixture)")
    raise SystemExit(0)
if args and args[0] == "--list":
    print("Installed Commands:")
    raise SystemExit(0)
if args and args[0] == "metadata":
    print(json.dumps(fixture_metadata(), sort_keys=True))
    raise SystemExit(0)
if args and args[0] == "build":
    run = run_id(args)
    if not run or not os.environ.get("MBX_SOCKET"):
        raise SystemExit(81)
    wrapper = os.environ.get("RUSTC_WRAPPER", "")
    fake_rustc = os.environ["MBX_TEST_FAKE_RUSTC"]
    if not wrapper or not pathlib.Path(wrapper).is_file() or pathlib.Path(wrapper).resolve() == pathlib.Path(fake_rustc).resolve():
        raise SystemExit(82)
    nested_env = os.environ.copy()
    nested_env["MBX_FIXTURE_RUN_ID"] = run
    cargo = os.environ["MBX_TEST_FAKE_CARGO"]
    nested = subprocess.run(
        [cargo, "metadata", "--no-deps", "--format-version", "1", f"--fixture-run-id={run}"],
        env=nested_env, stdin=subprocess.DEVNULL, check=False,
    )
    if nested.returncode != 0:
        raise SystemExit(83)
    explicit = subprocess.run(
        [os.environ["MBX_TEST_REAL_MBX"], "cargo", "metadata", "--no-deps", "--format-version", "1", f"--fixture-run-id={run}"],
        env=nested_env, stdin=subprocess.DEVNULL, check=False,
    )
    if explicit.returncode != 0:
        raise SystemExit(84)
    boltffi = shutil.which("boltffi", path=os.environ.get("PATH", ""))
    if not boltffi:
        raise SystemExit(85)
    packed = subprocess.run(
        [boltffi, "generate", "swift", f"--fixture-run-id={run}"],
        env=nested_env, stdin=subprocess.DEVNULL, check=False,
    )
    if packed.returncode != 0:
        raise SystemExit(86)
    gate = f"wrapped:{run}"
    compiler_env = nested_env.copy()
    compiler_env["MBX_TEST_RUSTC_GATE"] = gate
    compiler_probe_args = ["--version", f"--fixture-run-id={run}"]
    shim = subprocess.run(
        [wrapper, fake_rustc, *compiler_probe_args],
        env=compiler_env, stdin=subprocess.DEVNULL, check=False,
    )
    event("rustc-wrapper-invoked", compiler_probe_args, run_id=run, wrapper=wrapper)
    if shim.returncode != 0:
        raise SystemExit(87)
    mode = next((item.split("=", 1)[1] for item in args if item.startswith("--fixture-mode=")), "ok")
    if mode == "exit37":
        raise SystemExit(37)
    if mode == "term":
        os.kill(os.getpid(), signal.SIGTERM)
        time.sleep(30)
        raise SystemExit(88)
    if mode != "ok":
        raise SystemExit(89)
    print("fixture final child complete")
    raise SystemExit(0)
raise SystemExit(90)
"""

    boltffi_body = r"""
args = sys.argv[1:]
event("boltffi", args)
if args[:2] != ["generate", "swift"]:
    raise SystemExit(91)
run = run_id(args)
env = os.environ.copy()
env["MBX_FIXTURE_RUN_ID"] = run
nested = subprocess.run(
    [os.environ["MBX_TEST_FAKE_CARGO"], "metadata", "--no-deps", "--format-version", "1", f"--fixture-run-id={run}"],
    env=env, stdin=subprocess.DEVNULL, check=False,
)
raise SystemExit(nested.returncode)
"""

    rustc_body = r"""
args = sys.argv[1:]
gate = os.environ.get("MBX_TEST_RUSTC_GATE", "")
is_version_probe = "--version" in args or "-vV" in args
if is_version_probe and not gate:
    event("fake-rustc-probe", args)
    if "-vV" in args:
        print("rustc 1.97.1 (fixture)\nhost: x86_64-unknown-linux-gnu\nrelease: 1.97.1")
    else:
        print("rustc 1.97.1 (fixture)")
    raise SystemExit(0)
event("fake-rustc" if gate else "fake-rustc-raw", args, gate_ok=gate == f"wrapped:{os.environ.get('MBX_FIXTURE_RUN_ID', '')}")
if not gate or not gate.startswith("wrapped:"):
    raise SystemExit(97)
if is_version_probe:
    if "-vV" in args:
        print("rustc 1.97.1 (fixture)\nhost: x86_64-unknown-linux-gnu\nrelease: 1.97.1")
    else:
        print("rustc 1.97.1 (fixture)")
    raise SystemExit(0)
raise SystemExit(98)
"""

    rustup_body = r"""
args = sys.argv[1:]
event("rustup", args)
if args[:2] == ["show", "active-toolchain"]:
    print("1.97.1-x86_64-unknown-linux-gnu (fixture)")
    raise SystemExit(0)
if args[:1] == ["which"]:
    print(os.environ["MBX_TEST_FAKE_RUSTC"])
    raise SystemExit(0)
raise SystemExit(99)
"""

    def program(body: str) -> str:
        return f"#!{python}\n" + textwrap.dedent(common + body)

    return program(cargo_body), program(boltffi_body), program(rustc_body), program(rustup_body)


def parse_events(path: Path) -> list[dict[str, Any]]:
    try:
        if path.stat().st_size > MAX_EVENT_LOG_BYTES:
            raise HarnessFailure("fixture-event-log-too-large")
        lines = path.read_text(encoding="utf-8").splitlines()
        events = [json.loads(line) for line in lines]
    except (OSError, json.JSONDecodeError) as error:
        raise HarnessFailure("fixture-event-log-invalid") from error
    require(all(isinstance(item, dict) for item in events), "fixture-event-shape-invalid")
    return events


def run_id_events(events: list[dict[str, Any]], run_id: str) -> list[dict[str, Any]]:
    return [event for event in events if event.get("run_id") == run_id]


def assert_single_session(
    events: list[dict[str, Any]], run_id: str, fake_rustc: Path, private_root: Path
) -> None:
    selected = run_id_events(events, run_id)
    cargo_events = [event for event in selected if event.get("kind") == "cargo"]
    require(any(event.get("argv", [None])[0] == "build" for event in cargo_events), "final-cargo-child-missing")
    require(any(event.get("kind") == "boltffi" for event in selected), "nested-boltffi-missing")
    require(any(event.get("kind") == "rustc-wrapper-invoked" for event in selected), "rustc-shim-path-not-invoked")
    require(any(event.get("kind") == "fake-rustc" for event in selected), "fake-rustc-not-reached")
    fake_rustup = fake_rustc.parent / "rustup"
    require(
        all(
            event.get("executable") == str(fake_rustup)
            for event in selected
            if event.get("kind") == "rustup"
        ),
        "non-fixture-rustup-used",
    )
    live = [event for event in selected if event.get("socket")]
    sockets = {event["socket"] for event in live}
    wrappers = {event["rustc_wrapper"] for event in live}
    require(len(sockets) == 1, "nested-child-opened-different-mbx-session")
    require(len(wrappers) == 1, "nested-child-changed-rustc-wrapper")
    socket_path = Path(next(iter(sockets)))
    wrapper_path = Path(next(iter(wrappers)))
    require(within(socket_path, private_root), "socket-outside-private-temp")
    require(socket_path.is_absolute(), "socket-path-not-absolute")
    require(wrapper_path.is_absolute(), "rustc-wrapper-not-absolute")
    require(wrapper_path.resolve() != fake_rustc.resolve(), "raw-rustc-selected-as-wrapper")
    require(all(event.get("socket_exists") for event in live), "session-socket-missing-during-child")
    require(all(event.get("wrapper_exists") for event in live), "rustc-wrapper-missing-during-child")
    require(not any(event.get("shim_mode_leaked") for event in live), "mise-shim-mode-leaked-to-cargo")
    compiler_events = [event for event in selected if event.get("kind") == "fake-rustc"]
    require(len(compiler_events) == 1, "unexpected-fake-rustc-call-count")
    require(compiler_events[0].get("gate_ok") is True, "fake-rustc-bypassed-mbx-shim")
    require(
        not any(event.get("kind") == "fake-rustc-raw" for event in selected),
        "raw-fake-rustc-bypass",
    )
    require(any(event.get("kind") == "cargo" and event.get("socket") for event in selected), "outer-session-not-inherited")


def expected_version(output: str, component: str, version: str) -> bool:
    if component == "mise":
        pattern = (
            rf"(?:mise\s+)?{re.escape(version)}"
            r"(?:\s+linux-[a-zA-Z0-9_-]+(?:\s+\(\d{4}-\d{2}-\d{2}\))?)?"
        )
        return re.fullmatch(pattern, output.strip()) is not None
    return re.search(rf"\b{re.escape(component)}\s+{re.escape(version)}(?:\b|$)", output) is not None


def assert_version_parser_fixtures() -> None:
    fixtures = (
        ("2026.10.1 linux-x64 (2026-10-03)\n", "mise", "2026.10.1", True),
        ("mise 2026.10.1 linux-x64 (2026-10-03)\n", "mise", "2026.10.1", True),
        ("2026.10.10 linux-x64 (2026-10-03)\n", "mise", "2026.10.1", False),
        ("mise 2026.10.1 linux-x64 (2026-10-03)\nextra\n", "mise", "2026.10.1", False),
        ("mbx 1.22.0\n", "mbx", "1.22.0", True),
        ("mbx 1.22.1\n", "mbx", "1.22.0", False),
    )
    require(
        all(expected_version(output, component, version) is expected for output, component, version, expected in fixtures),
        "version-parser-fixtures-failed",
    )


def make_env(temp_root: Path, fixture: Path, fake_bin: Path, mbx_bin: Path, event_log: Path) -> dict[str, str]:
    paths = [str(fake_bin), str(mbx_bin.parent), "/usr/bin", "/bin"]
    env = {
        "HOME": str(temp_root / "home"),
        "PATH": os.pathsep.join(paths),
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "TERM": "dumb",
        "CI": "1",
        "NO_COLOR": "1",
        "TMPDIR": str(temp_root / "tmp"),
        "XDG_CONFIG_HOME": str(temp_root / "xdg-config"),
        "XDG_CACHE_HOME": str(temp_root / "xdg-cache"),
        "XDG_DATA_HOME": str(temp_root / "xdg-data"),
        "XDG_STATE_HOME": str(temp_root / "xdg-state"),
        "CARGO_HOME": str(temp_root / "cargo-home"),
        "RUSTUP_HOME": str(temp_root / "rustup-home"),
        "CARGO": str(fake_bin / "cargo"),
        "RUSTC": str(fake_bin / "rustc"),
        "CARGO_TARGET_DIR": str(temp_root / "target"),
        "MBX_CACHE_DIR": str(temp_root / "mbx-cache"),
        "MBX_TARGET_ROOT": str(temp_root / "mbx-target"),
        "CARGO_NET_OFFLINE": "true",
        "RUSTUP_OFFLINE": "1",
        "MISE_CONFIG_DIR": str(temp_root / "mise-config"),
        "MISE_DATA_DIR": str(temp_root / "mise-data"),
        "MISE_CACHE_DIR": str(temp_root / "mise-cache"),
        "MISE_STATE_DIR": str(temp_root / "mise-state"),
        "MISE_INSTALLS_DIR": str(temp_root / "mise-installs"),
        "MISE_OFFLINE": "1",
        "MISE_TRUSTED_CONFIG_PATHS": str(fixture),
        "MISE_COLOR": "0",
        EVENT_LOG_ENV: str(event_log),
        "MBX_TEST_FAKE_CARGO": str(fake_bin / "cargo"),
        "MBX_TEST_FAKE_RUSTC": str(fake_bin / "rustc"),
        "MBX_TEST_REAL_MBX": str(mbx_bin),
        "MBX_FIXTURE_ROOT": str(fixture),
        "MBX_FIXTURE_TARGET": str(temp_root / "target"),
    }
    for directory in (
        "home", "tmp", "xdg-config", "xdg-cache", "xdg-data", "xdg-state",
        "cargo-home", "rustup-home", "target", "mbx-cache", "mbx-target",
        "mise-config", "mise-data", "mise-cache", "mise-state", "mise-installs",
    ):
        (temp_root / directory).mkdir(mode=0o700, parents=True, exist_ok=True)
    return env


def execute_harness(arguments: argparse.Namespace) -> None:
    require(os.name == "posix", "requires-posix")
    require(sys.version_info >= (3, 11), "python-311-required")
    assert_version_parser_fixtures()
    assert_production_contract()

    mise_bin = require_immutable_executable(Path(arguments.mise_bin), "mise")
    mbx_bin = require_immutable_executable(Path(arguments.mbx_bin), "mbx")
    require(sha256(mise_bin) == EXPECTED_MISE_SHA256, "mise-binary-hash-mismatch")
    require(sha256(mbx_bin) == EXPECTED_MBX_SHA256, "mbx-binary-hash-mismatch")

    old_umask = os.umask(0o077)
    try:
        with tempfile.TemporaryDirectory(prefix="jackin-mbx-wrapper-") as temporary:
            temp_root = Path(temporary).resolve()
            os.chmod(temp_root, 0o700)
            fixture = temp_root / "project"
            fake_bin = temp_root / "fake-bin"
            captures = temp_root / "captures"
            fixture.mkdir(mode=0o700)
            fake_bin.mkdir(mode=0o700)
            captures.mkdir(mode=0o700)

            fixture_config = fixture / "mise.toml"
            fixture_config.write_text(
                '[wrappers.cargo]\ncommand = "mbx"\nenv = { MBX_CARGO_SHIM_MODE = "1" }\n',
                encoding="utf-8",
            )
            (fixture / ".cargo").mkdir(mode=0o700)
            (fixture / ".cargo" / "config.toml").write_text(
                '[alias]\nxtask = "build"\n', encoding="utf-8"
            )
            (fixture / "src").mkdir(mode=0o700)
            (fixture / "src" / "lib.rs").write_text("pub fn fixture() {}\n", encoding="utf-8")
            (fixture / "Cargo.toml").write_text(
                '[package]\nname = "mbx-wrapper-fixture"\nversion = "0.1.0"\nedition = "2024"\n',
                encoding="utf-8",
            )
            event_log = temp_root / "events.jsonl"
            event_log.touch(mode=0o600)
            os.chmod(event_log, 0o600)

            cargo_program, boltffi_program, rustc_program, rustup_program = fake_programs(sys.executable)
            write_executable(fake_bin / "cargo", cargo_program)
            write_executable(fake_bin / "boltffi", boltffi_program)
            write_executable(fake_bin / "rustc", rustc_program)
            write_executable(fake_bin / "rustup", rustup_program)
            env = make_env(temp_root, fixture, fake_bin, mbx_bin, event_log)

            require(shutil.which("cargo", path=env["PATH"]) == str(fake_bin / "cargo"), "fake-cargo-not-first-on-path")
            require(shutil.which("mbx", path=env["PATH"]) == str(mbx_bin), "pinned-mbx-not-on-path")

            code, mise_version, _ = run_bounded(
                [str(mise_bin), "--version"], cwd=fixture, env=env,
                capture_dir=captures, label="mise-version", timeout_seconds=8,
            )
            require(code == 0 and expected_version(mise_version, "mise", EXPECTED_MISE_VERSION), "mise-version-mismatch")

            code, mbx_version, _ = run_bounded(
                [str(mise_bin), "exec", "--deny-net", "--", "mbx", "--version"],
                cwd=fixture, env=env, capture_dir=captures, label="mbx-version-through-mise", timeout_seconds=12,
            )
            require(code == 0 and expected_version(mbx_version, "mbx", EXPECTED_MBX_VERSION), "mbx-version-through-mise-failed")

            code, _, _ = run_bounded(
                [str(mise_bin), "reshim"], cwd=fixture, env=env,
                capture_dir=captures, label="mise-reshim", timeout_seconds=12,
            )
            require(code == 0, "mise-reshim-failed")

            code, which_output, _ = run_bounded(
                [str(mise_bin), "which", "cargo"], cwd=fixture, env=env,
                capture_dir=captures, label="mise-which-cargo", timeout_seconds=8,
            )
            require(code == 0, "mise-which-cargo-failed")
            which_lines = [line.strip() for line in which_output.splitlines() if line.strip()]
            require(bool(which_lines), "mise-which-cargo-empty")
            which_path = Path(which_lines[-1])
            if not which_path.is_absolute():
                resolved_which = Path(shutil.which(which_lines[-1], path=env["PATH"]) or "")
            else:
                resolved_which = which_path
            require(resolved_which.exists(), "mise-which-cargo-path-missing")
            require(
                resolved_which.resolve() == mbx_bin
                or lexically_within(resolved_which, Path(env["MISE_DATA_DIR"])),
                "mise-which-cargo-not-wrapper",
            )
            require(resolved_which.resolve() != Path(env["MBX_TEST_FAKE_CARGO"]).resolve(), "mise-which-cargo-bypassed-wrapper")

            scenarios = (
                ("normal", "ok", 0),
                ("exit37", "exit37", 37),
                ("sigterm", "term", 128 + signal.SIGTERM),
            )
            for run_id, mode, expected_status in scenarios:
                argv = [
                    str(mise_bin), "exec", "--deny-net", "--", "cargo", "xtask",
                    "--fixture-arg", "value with spaces", "single 'quote'",
                    f"--fixture-mode={mode}", f"--fixture-run-id={run_id}",
                ]
                code, _, _ = run_bounded(
                    argv, cwd=fixture, env=env, capture_dir=captures,
                    label=f"run-{run_id}", timeout_seconds=PROCESS_TIMEOUT_SECONDS,
                )
                require(code == expected_status, f"status-mismatch-{run_id}")
                events = parse_events(event_log)
                selected = run_id_events(events, run_id)
                final = next(
                    (event for event in selected if event.get("kind") == "cargo" and event.get("argv", [None])[0] == "build"),
                    None,
                )
                require(final is not None, f"final-child-missing-{run_id}")
                original = next(
                    (event for event in selected if event.get("kind") == "cargo-entry" and event.get("argv", [None])[0] == "xtask"),
                    None,
                )
                require(original is not None, f"cargo-alias-input-missing-{run_id}")
                require(
                    final["argv"][1:] == ["--fixture-arg", "value with spaces", "single 'quote'", f"--fixture-mode={mode}", f"--fixture-run-id={run_id}"],
                    f"argv-boundaries-changed-{run_id}",
                )
                assert_single_session(
                    events, run_id, Path(env["MBX_TEST_FAKE_RUSTC"]), temp_root
                )

            require(sha256(mise_bin) == EXPECTED_MISE_SHA256, "mise-binary-changed-during-probe")
            require(sha256(mbx_bin) == EXPECTED_MBX_SHA256, "mbx-binary-changed-during-probe")
    finally:
        os.umask(old_umask)


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mise-bin", default=os.environ.get("MBX_TEST_MISE_BIN", "/tools/mise/bin/mise"))
    parser.add_argument("--mbx-bin", default=os.environ.get("MBX_TEST_MBX_BIN", "/tools/mbx-1.22.0/bin/mbx"))
    return parser.parse_args()


def main() -> int:
    def terminate_children(signum: int, _frame: Any) -> None:
        for process in tuple(ACTIVE_CHILDREN.values()):
            try:
                stop_process_group(process)
            except Exception:
                kill_process_group(process, signal.SIGKILL)
        raise SystemExit(128 + signum)

    for handled_signal in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(handled_signal, terminate_children)
    try:
        execute_harness(arguments())
    except HarnessFailure as error:
        print(f"FAIL mbx-cargo-wrapper: {error.code}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("FAIL mbx-cargo-wrapper: interrupted", file=sys.stderr)
        return 130
    print(
        "PASS mbx-cargo-wrapper: pinned Mise/MBX, argv, nested session/socket and shim-path checks, exit 37, TERM 143; real compilation not tested"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
