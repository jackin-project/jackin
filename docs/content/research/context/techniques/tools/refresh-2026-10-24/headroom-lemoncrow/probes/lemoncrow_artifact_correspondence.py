"""Compare package members without installing or extracting an upstream archive."""
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import zipfile

archive = Path(sys.argv[1])
source = Path(sys.argv[2]) / "client" / "src"
data = archive.read_bytes()
with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
    wheels = [item for item in tar.getmembers() if item.isfile() and item.name.endswith(".whl")]
    assert len(wheels) == 1, [item.name for item in wheels]
    member = wheels[0]
    with zipfile.ZipFile(io.BytesIO(tar.extractfile(member).read())) as wheel:
        names = [name for name in wheel.namelist() if name.startswith("lemoncrow_client/") and not name.endswith("/")]
        changed, missing, equal = [], [], []
        for name in names:
            path = source / name
            if not path.is_file():
                missing.append(name)
            elif path.read_bytes() != wheel.read(name):
                changed.append(name)
            else:
                equal.append(name)
        metadata = next(name for name in wheel.namelist() if name.endswith(".dist-info/METADATA"))
        print(json.dumps({
            "archive_sha256": hashlib.sha256(data).hexdigest(),
            "wheel": member.name,
            "byte_identical_source_files": len(equal),
            "changed": changed,
            "not_in_source": missing,
            "compared_files": names,
            "metadata": wheel.read(metadata).decode("utf-8"),
        }, indent=2))
        assert len(equal) == 87 and not changed and not missing
