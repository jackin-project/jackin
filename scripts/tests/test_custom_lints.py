# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Regression proof for explicit nightly selection and closed library discovery."""

import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "custom-lints.py"
SPEC = importlib.util.spec_from_file_location("custom_lints", SCRIPT)
LINTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LINTS)
HOST = "aarch64-apple-darwin"
VERSION = f"commit-hash: {LINTS.COMMIT}\nrelease: 1.101.0-nightly\nhost: {HOST}\n"


class CustomLintTests(unittest.TestCase):
    def test_wrong_compiler_or_invalid_host_rejected(self):
        self.assertEqual(LINTS.compiler_host(VERSION), HOST)
        for version in (VERSION.replace(LINTS.COMMIT, "bad"),
                        VERSION.replace("1.101.0-nightly", "1.99.1"),
                        VERSION.replace(HOST, "../../escape")):
            with self.assertRaises(ValueError):
                LINTS.compiler_host(version)

    def test_missing_or_symlink_library_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(ValueError):
                LINTS.library_path(root, HOST)
            debug = root / "library" / "debug"
            debug.mkdir(parents=True)
            name = f"libjackin_lints@{LINTS.NIGHTLY}-{HOST}.dylib"
            (root / "other").write_bytes(b"other ABI")
            (debug / name).symlink_to(root / "other")
            with patch.object(LINTS.sys, "platform", "darwin"):
                with self.assertRaises(ValueError):
                    LINTS.library_path(root, HOST)

    def test_entire_pipeline_uses_nightly_cwd_and_one_fresh_library(self):
        calls = []
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "build"
            debug = target / "library" / "debug"
            debug.mkdir(parents=True)
            tagged = f"libjackin_lints@{LINTS.NIGHTLY}-{HOST}.dylib"
            (debug / tagged).write_bytes(b"built lint")
            (debug / "libforeign@stable.dylib").write_bytes(b"stale")

            def run(argv, cwd, env, capture=False):
                calls.append((argv, cwd, env.copy()))
                if argv[-1] == "-Vv":
                    return VERSION
                if "component" in argv:
                    return "\n".join(f"{component}-{HOST}" for component in LINTS.COMPONENTS)
                if "dylint" in argv:
                    search = Path(env["DYLINT_LIBRARY_PATH"])
                    self.assertEqual([p.name for p in search.iterdir()], [tagged])
                    self.assertEqual((search / tagged).read_bytes(), b"built lint")
                return None

            with patch.dict(os.environ, {"CARGO_TARGET_DIR": str(target),
                                         "RUSTUP_TOOLCHAIN": "stable",
                                         "DYLINT_LIBRARY_PATH": "/stale",
                                         "DYLINT_RUSTFLAGS": "--cfg disable_checks",
                                         "DYLINT_DRIVER_PATH": "/foreign-driver"}), \
                 patch.object(LINTS.shutil, "which", side_effect=lambda name, **_: "/pinned tools/bin/" + name), \
                 patch.object(LINTS.sys, "platform", "darwin"), \
                 patch.object(LINTS, "command", side_effect=run):
                LINTS.run(root)
            for _, _, env in calls:
                self.assertNotIn("DYLINT_RUSTFLAGS", env)
                self.assertNotIn("DYLINT_DRIVER_PATH", env)
            self.assertEqual(calls[2][0], ["rustup", "run", LINTS.NIGHTLY,
                                          "cargo", "build", "--locked"])
            self.assertEqual(calls[3][0], ["rustup", "run", LINTS.NIGHTLY,
                                          "/pinned tools/bin/cargo-nextest", "nextest", "run", "--locked"])
            for _, cwd, env in calls[:4]:
                self.assertEqual(cwd, root / "crates/tools/jackin-lints")
                self.assertNotIn("RUSTUP_TOOLCHAIN", env)
                self.assertNotIn("DYLINT_LIBRARY_PATH", env)
                self.assertEqual(env["PATH"].split(os.pathsep)[0],
                                 str(Path.home() / ".cargo" / "bin"))
                self.assertEqual(env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f"),
                                 ["-C", "linker=/pinned tools/bin/dylint-link"])
            self.assertNotIn("CARGO_ENCODED_RUSTFLAGS", calls[-1][2])
            self.assertNotIn("RUSTFLAGS", calls[-1][2])
            self.assertEqual(calls[-1][0], ["/pinned tools/bin/cargo-dylint", "dylint", "--no-metadata", "--all",
                                            "--fail-on-no-libraries", "--", "--workspace", "--locked"])
            self.assertEqual(calls[-1][1], root)
            self.assertFalse(Path(calls[-1][2]["DYLINT_LIBRARY_PATH"]).exists())

    def test_missing_component_stops_before_build(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(LINTS.shutil, "which", side_effect=lambda name, **_: "/pinned tools/bin/" + name), \
                 patch.object(LINTS, "command", side_effect=[VERSION, ""]) as command:
                with self.assertRaisesRegex(ValueError, "missing pinned nightly component"):
                    LINTS.run(Path(directory))
                self.assertEqual(command.call_count, 2)


if __name__ == "__main__":
    unittest.main()
