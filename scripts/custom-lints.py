#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Build, UI-test, and run the explicitly pinned rustc-private lint library."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

NIGHTLY = "nightly-2026-10-18"
COMMIT = "8006091eb71ae7d9e9bccb66c6122c9d68af4466"
COMPONENTS = ("clippy", "llvm-tools", "rustc-dev", "rustfmt")


def command(argv, cwd, env, capture=False):
    print("custom-lints:", " ".join(map(str, argv)), flush=True)
    result = subprocess.run(argv, cwd=cwd, env=env, check=True,
                            text=True, stdout=subprocess.PIPE if capture else None)
    return result.stdout if capture else None


def compiler_host(version):
    fields = dict(line.split(": ", 1) for line in version.splitlines() if ": " in line)
    if fields.get("commit-hash") != COMMIT or fields.get("release") != "1.101.0-nightly":
        raise ValueError("custom lint compiler does not match the qualified nightly")
    host = fields.get("host", "")
    if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)+", host):
        raise ValueError("invalid compiler host")
    return host


def build_environment(root):
    env = os.environ.copy()
    cargo_home = Path(env.get("CARGO_HOME", Path.home() / ".cargo")).resolve()
    tools = {}
    for name in ("dylint-link", "cargo-nextest", "cargo-dylint"):
        selected = shutil.which(name, path=env.get("PATH"))
        if selected is None:
            raise ValueError(f"missing Mise-pinned {name}")
        tools[name] = str(Path(selected).resolve())
    env["PATH"] = str(cargo_home / "bin") + os.pathsep + env.get("PATH", "")
    for key in list(env):
        if key.startswith("DYLINT_"):
            env.pop(key)
    for key in ("CARGO", "RUSTUP_TOOLCHAIN", "RUSTC", "RUSTDOC", "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"):
        env.pop(key, None)
    target = Path(env.get("CARGO_TARGET_DIR", root / "target" / "custom-lints")).resolve()
    env["CARGO_TARGET_DIR"] = str(target / "library")
    env["DYLINT_LINK_ENABLE_COPY_LIBRARY"] = "1"
    env["CARGO_ENCODED_RUSTFLAGS"] = "-C\x1flinker=" + tools["dylint-link"]
    return env, target, tools


def library_path(target, host):
    if sys.platform == "darwin":
        name = f"libjackin_lints@{NIGHTLY}-{host}.dylib"
    elif sys.platform == "linux":
        name = f"libjackin_lints@{NIGHTLY}-{host}.so"
    else:
        raise ValueError("custom lint qualification supports macOS and Linux")
    path = target / "library" / "debug" / name
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"missing regular nightly-labelled lint library: {path}")
    return path


def run(root):
    lint = root / "crates" / "tools" / "jackin-lints"
    env, target, tools = build_environment(root)
    nightly = ["rustup", "run", NIGHTLY]
    host = compiler_host(command(nightly + ["rustc", "-Vv"], lint, env, True))
    installed = command(["rustup", "component", "list", "--toolchain", NIGHTLY,
                         "--installed"], lint, env, True).splitlines()
    for component in COMPONENTS:
        if f"{component}-{host}" not in installed:
            raise ValueError(f"missing pinned nightly component: {component}")
    command(nightly + ["cargo", "build", "--locked"], lint, env)
    command(nightly + [tools["cargo-nextest"], "nextest", "run", "--locked", "--no-tests", "pass"], lint, env)
    library = library_path(target, host)
    with tempfile.TemporaryDirectory(prefix="jackin-custom-lints-") as directory:
        destination = Path(directory) / library.name
        shutil.copyfile(library, destination)
        workspace_env = env.copy()
        workspace_env["CARGO_TARGET_DIR"] = str(target / "workspace")
        workspace_env["DYLINT_LIBRARY_PATH"] = directory
        workspace_env.pop("DYLINT_LINK_ENABLE_COPY_LIBRARY", None)
        workspace_env.pop("CARGO_ENCODED_RUSTFLAGS", None)
        workspace_env.pop("RUSTFLAGS", None)
        command([tools["cargo-dylint"], "dylint", "--no-metadata", "--all",
                 "--fail-on-no-libraries", "--", "--workspace", "--locked"], root, workspace_env)


if __name__ == "__main__":
    try:
        run(Path(__file__).resolve().parents[1])
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"custom-lints: {error}", file=sys.stderr)
        sys.exit(1)
