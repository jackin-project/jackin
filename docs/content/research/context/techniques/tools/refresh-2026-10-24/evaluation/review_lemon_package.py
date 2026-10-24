# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Independent archive-member comparison against immutable Git objects; no extraction/install."""
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import zipfile

archive, repo = map(Path, sys.argv[1:3])
commit = "d96bad0e59dbc799d54ac5ca1a6a4fe9b8a8ab8d"
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
assert digest == "73bb7c7d062f1f622dff50a851d36a71b1c8efa9ebebd7017d1d09cf81529e94"
with tarfile.open(archive) as container:
    members = [m for m in container.getmembers() if m.name.endswith(".whl")]
    assert len(members) == 1
    wheel_name = members[0].name
    wheel_bytes = container.extractfile(members[0]).read()
rows = []
with zipfile.ZipFile(io.BytesIO(wheel_bytes)) as wheel:
    for name in wheel.namelist():
        if not name.startswith("lemoncrow_client/") or name.endswith("/"):
            continue
        pinned = subprocess.run(["git", "-C", str(repo), "show", f"{commit}:client/src/{name}"],
                                check=True, capture_output=True).stdout
        payload = wheel.read(name)
        assert payload == pinned, name
        rows.append({"member": name, "sha256": hashlib.sha256(payload).hexdigest()})
    metadata_names = [n for n in wheel.namelist() if n.endswith(".dist-info/METADATA")]
    assert len(metadata_names) == 1
    metadata = wheel.read(metadata_names[0]).decode()
assert len(rows) == 87
print(json.dumps({"reviewer": "/root/rtk_leanctx", "commit": commit, "archive_sha256": digest,
                  "wheel_member": wheel_name, "wheel_sha256": hashlib.sha256(wheel_bytes).hexdigest(),
                  "files_match_immutable_git_objects": len(rows), "files": rows,
                  "metadata": [line for line in metadata.splitlines() if line.startswith(("Name:", "Version:", "Requires-Python:", "Requires-Dist:"))],
                  "installed": False, "extracted_to_filesystem": False}, indent=2))
