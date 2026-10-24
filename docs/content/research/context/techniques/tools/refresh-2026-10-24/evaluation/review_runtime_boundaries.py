# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Independent offline review probes; no producer command, host or model call."""
import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
from types import SimpleNamespace


def deny_network(*args, **kwargs):
    raise RuntimeError("network disabled by independent review")


socket.socket.connect = deny_network
socket.create_connection = deny_network
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--headroom-source", type=Path, required=True)
parser.add_argument("--lemon-source", type=Path, required=True)
parser.add_argument("--scratch", type=Path, required=True)
args = parser.parse_args()
args.scratch.mkdir(parents=True, exist_ok=True)

pins = {
    "headroom": (args.headroom_source, "76133cff59b71284399bf760c5578dc373c97231", [
        "headroom/compress.py", "headroom/transforms/kompress_compressor.py",
        "crates/headroom-core/src/transforms/kompress.rs",
        "crates/headroom-core/src/transforms/pipeline/reformats/json_minifier.rs",
        "crates/headroom-core/src/ccr/mod.rs", "crates/headroom-core/src/ccr/backends/sqlite.rs",
        "crates/headroom-core/Cargo.toml", "headroom/telemetry/beacon.py",
    ]),
    "lemoncrow": (args.lemon_source, "d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d", [
        "client/src/lemoncrow_client/lite/mcp.py", "client/src/lemoncrow_client/worktrees.py",
        "client/src/lemoncrow_client/localtools/shell.py", "client/src/lemoncrow_client/kit/output_delta.py",
        "client/src/lemoncrow_client/lite/index.py", "docs/reference/open-source-boundary.md",
    ]),
}
identities = []
for product, (repo, commit, files) in pins.items():
    for relative in files:
        pinned = subprocess.run(["git", "-C", str(repo), "show", f"{commit}:{relative}"],
                                check=True, capture_output=True).stdout
        local = (repo / relative).read_bytes()
        assert pinned == local, (product, relative)
        identities.append({"product": product, "commit": commit, "path": relative,
                           "sha256": hashlib.sha256(local).hexdigest(), "matches_git_object": True})

import headroom
from headroom.compress import _message_is_lossless, _string_pair_is_lossless
from lemoncrow_client.kit.output_delta import RunHistory
from lemoncrow_client.localtools.shell import _present

installed = Path(headroom.__file__).parent
assert (installed / "compress.py").read_bytes() == (args.headroom_source / "headroom/compress.py").read_bytes()
headroom_cases = {
    "duplicate_key_last_value_accepted": _string_pair_is_lossless('[{"a":1,"a":2}]', '[1]{a:int}\n2\n'),
    "float_to_integer_rejected_in_json_string": not _string_pair_is_lossless('[{"a":1.0}]', '[1]{a:int}\n1\n'),
    "scalar_metadata_false_to_zero_accepted": _message_is_lossless(
        {"role": "tool", "content": "same", "flag": False},
        {"role": "tool", "content": "same", "flag": 0}),
}
assert all(headroom_cases.values())

raw = "review record preserves an exact repeated observation\n" * 32
with tempfile.TemporaryDirectory(prefix="spill-review-", dir=args.scratch) as temporary:
    root = Path(temporary)
    blocker = root / "not-a-directory"
    blocker.write_text("Deliberate regular file preventing spill-directory creation.")
    os.environ["LEMONCROW_MCP_SPILL_DIR"] = str(blocker)
    os.environ["LEMONCROW_BASH_UNCHANGED_DELTA"] = "1"
    context = SimpleNamespace(memory=SimpleNamespace(command_runs=RunHistory()))
    first = _present(context, "review-fixture", raw, 0, root, "")
    second = _present(context, "review-fixture", raw, 0, root, "")
    assert "unchanged:" not in first and "unchanged:" in second
    assert list(root.iterdir()) == [blocker]
    fresh = SimpleNamespace(memory=SimpleNamespace(command_runs=RunHistory()))
    fresh_result = _present(fresh, "review-fixture", raw, 0, root, "")
    assert "unchanged:" not in fresh_result
    os.environ["LEMONCROW_BASH_UNCHANGED_DELTA"] = "0"
    disabled = _present(context, "review-fixture", raw, 0, root, "")
    assert "unchanged:" not in disabled
    lemon_cases = {
        "raw_chars": len(raw), "first_chars": len(first), "second_output": second,
        "spill_unavailable_from_first_call": True, "no_original_spill_created": True,
        "fresh_history_has_no_unchanged_marker": True, "disabled_delta_has_no_unchanged_marker": True,
        "scope": "Direct presentation/RunHistory calls with synthetic strings. No shell producer or native host; failed path is a regular file, not disk-full.",
    }

print(json.dumps({
    "reviewer": "/root/rtk_leanctx", "source_identities": identities,
    "headroom_package": {"version": importlib.metadata.version("headroom-ai"),
                         "installed_module": str(installed), "compress_matches_pinned_source": True},
    "headroom": headroom_cases, "lemoncrow": lemon_cases,
    "new_scalar_guard_limit": "False/0 accepted by the verifier's non-string scalar equality. No actual compression pipeline changing this metadata was demonstrated.",
    "provider_calls": 0, "model_inference": 0, "rust_builds": 0,
}, indent=2))
