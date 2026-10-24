"""Verify the installed package identity and the duplicate-key verification boundary."""
import hashlib
import importlib.metadata
import json
from pathlib import Path
import sys

import headroom
import headroom._core as core
from headroom.compress import _string_pair_is_lossless

source = Path(sys.argv[1]) / "headroom"
installed = Path(headroom.__file__).parent
print("python", sys.version)
print("headroom", importlib.metadata.version("headroom-ai"), headroom.__file__)
print("compiled_core", core.__file__)
for relative in ("compress.py", "transforms/kompress_compressor.py", "transforms/content_router.py"):
    data = (installed / relative).read_bytes()
    equal = data == (source / relative).read_bytes()
    print(json.dumps({"path": relative, "package_source_equal": equal, "sha256": hashlib.sha256(data).hexdigest()}))
    assert equal
print(json.dumps({
    "probe": "parsed_json_duplicate_key_roundtrip",
    "result": _string_pair_is_lossless('[{"a":1,"a":2}]', '[1]{a:int}\n2\n'),
}))
