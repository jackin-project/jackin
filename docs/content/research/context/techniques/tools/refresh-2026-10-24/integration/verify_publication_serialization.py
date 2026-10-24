"""Verify the reversible publication formatting of four evidence JSON files."""

import hashlib
import json
from pathlib import Path


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def main():
    here = Path(__file__).resolve().parent
    repo = here.parents[7]
    record = json.loads((here / "publication-json-formatting.json").read_text())
    checked = []
    for item in record["files"]:
        path = (repo / item["path"]).resolve()
        if not path.is_relative_to(repo):
            raise ValueError("Evidence path escapes the repository")
        published = path.read_bytes()
        data = json.loads(published, object_pairs_hook=unique_object)
        original = (json.dumps(data, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
        checks = {
            "published_digest_matches": hashlib.sha256(published).hexdigest() == item["published_artifact_sha256"],
            "published_size_matches": len(published) == item["published_bytes"],
            "original_digest_reconstructed": hashlib.sha256(original).hexdigest() == item["original_snapshot_sha256"],
            "original_size_reconstructed": len(original) == item["original_bytes"],
            "parsed_values_identical": json.loads(original, object_pairs_hook=unique_object) == data,
        }
        if not all(checks.values()):
            raise ValueError(f"Publication serialization mismatch: {item['path']}: {checks}")
        checked.append({"path": item["path"], **checks})
    print(json.dumps({"files": len(checked), "passed": True, "checks": checked,
                      "scope": "Read-only verification of published bytes and exact original JSON serialization; no document/build/browser experiment rerun."}, indent=2))


if __name__ == "__main__":
    main()
