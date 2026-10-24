"""Strict baseline reconciliation: explicit canonical arrays and exact destinations.

No recursive ledger walk, numeric/material overwrite, fuzzy/substring heading
match, or Summary fallback. Unresolved, ambiguous and duplicate mappings fail.
"""
from pathlib import Path
from collections import defaultdict
import csv
import hashlib
import json
import re
import unicodedata

HERE = Path(__file__).resolve().parent
REPO = next(p for p in HERE.parents if (p / "AGENTS.md").exists() and (p / "Cargo.toml").exists())
PACKS = HERE.parent

# The final flag is only for the reviewed representation ledger: its disposition
# cells are explanatory prose, not generic status labels.
CANONICAL_LEDGERS = (
    ("caveman-ponytail/coverage-dispositions.json", "old_headings", False),
    ("evaluation/coverage-dispositions.json", "rows", False),
    ("evaluation/frontier-coverage-dispositions.json", "rows", False),
    ("headroom-lemoncrow/coverage-dispositions.json", "pages[].headings", False),
    ("integration/coordinator-dispositions.json", "rows", False),
    ("integration/artifact-dispositions.json", "rows", False),
    ("jackin-architecture/coverage-dispositions.json", "headings", False),
    ("jackin-architecture/integration-coverage-dispositions.json", "headings", False),
    ("providers/coverage-dispositions.json", "headings", False),
    ("representation/coverage-dispositions.json", "chapters[].headings", True),
    ("rtk-leanctx/heading-dispositions.json", "rows", False),
    ("market-literature/coverage-dispositions.json", "headings", False),
)


class CoverageError(ValueError):
    pass


def normalize_heading(value):
    return re.sub(r"^#{1,6}\s+", "", str(value or "")).strip()


def heading_slug(value):
    """GFM slug for the current plain-text heading domain.

    The review checks current results against the site's github-slugger.
    Markup is rejected instead of silently approximated.
    """
    if any(char in value for char in chr(96) + "*[]<>"):
        raise CoverageError("Heading markup requires an explicit parser update")
    return "".join(
        char for char in value.lower()
        if char in " -_" or unicodedata.category(char)[0] in "LMN"
    ).replace(" ", "-")


def document_headings(repo, document):
    path = repo / document
    if not path.is_file() or path.suffix not in {".md", ".mdx"}:
        return {}
    result, used = defaultdict(list), set()
    fence, frontmatter = None, False
    for index, line in enumerate(path.read_text().splitlines()):
        if index == 0 and line == "---":
            frontmatter = True
            continue
        if frontmatter:
            if line == "---":
                frontmatter = False
            continue
        marker = re.match("(" + chr(96) + "{3,}|~{3,})", line.lstrip())
        if marker:
            if fence is None:
                fence = marker.group(1)[0]
            elif marker.group(1)[0] == fence:
                fence = None
            continue
        if fence:
            continue
        match = re.match(r"^#{1,6}\s+(.+?)\s*$", line)
        if not match:
            continue
        heading = match.group(1)
        base = heading_slug(heading)
        slug, suffix = base, 0
        while slug in used:
            suffix += 1
            slug = f"{base}-{suffix}"
        used.add(slug)
        result[heading].append(slug)
    return dict(result)


def selected_records(data, selector):
    """Read the declared heading array, never recursively walk a ledger."""
    if "[]." not in selector:
        values = data.get(selector)
        if not isinstance(values, list):
            raise CoverageError(f"Missing canonical array {selector}")
        for index, value in enumerate(values):
            if not isinstance(value, dict):
                raise CoverageError(f"{selector}[{index}] is not an object")
            yield dict(value), f"{selector}[{index}]"
        return
    outer, inner = selector.split("[].", 1)
    parents = data.get(outer)
    if not isinstance(parents, list):
        raise CoverageError(f"Missing canonical array {outer}")
    for parent_index, parent in enumerate(parents):
        if not isinstance(parent, dict) or not isinstance(parent.get(inner), list):
            raise CoverageError(f"Missing {outer}[{parent_index}].{inner}")
        for index, value in enumerate(parent[inner]):
            if not isinstance(value, dict):
                raise CoverageError(f"{outer}[{parent_index}].{inner}[{index}] is not an object")
            row = dict(value)
            row.setdefault("document", parent.get("path"))
            if parent.get("old_sha256"):
                row.setdefault("blob_sha256", parent["old_sha256"])
            if parent.get("old_revision"):
                row.setdefault("baseline_commit", parent["old_revision"])
            yield row, f"{outer}[{parent_index}].{inner}[{index}]"


def identify_baseline(value, rows):
    by_id = {row["id"]: row for row in rows}
    document = next((value[key] for key in ("old_document", "old_path", "document", "path")
                     if isinstance(value.get(key), str)), None)
    line = value.get("old_line", value.get("line"))
    old = value.get("old_heading", value.get("heading"))
    census_id = value.get("id", "")
    if re.fullmatch(r"S\d+", str(census_id)):
        if census_id not in by_id:
            raise CoverageError(f"Unknown census ID {census_id}")
        matches = [by_id[census_id]]
    elif document is not None and line is not None:
        matches = [row for row in rows if row["document"] == document and row["line"] == str(line)]
    elif document is not None and old is not None:
        matches = [row for row in rows if row["document"] == document
                   and normalize_heading(row["heading"]) == normalize_heading(old)]
    else:
        raise CoverageError("Canonical record has no unambiguous baseline identity")
    if len(matches) != 1:
        raise CoverageError(f"Baseline identity has {len(matches)} matches")
    row = matches[0]
    checks = (
        (document, row["document"], "document"),
        (None if line is None else str(line), row["line"], "line"),
        (None if old is None else normalize_heading(old), normalize_heading(row["heading"]), "heading"),
        (value.get("baseline_commit"), row["baseline_commit"], "baseline commit"),
        (value.get("blob_sha256"), row["blob_sha256"], "baseline blob hash"),
    )
    for actual, expected, label in checks:
        if actual is not None and actual != expected:
            raise CoverageError(f"{row['id']}: conflicting {label}: {actual!r} != {expected!r}")
    return row


def anchor_document(anchor, repo):
    path, mark, fragment = anchor.partition("#")
    if path.startswith("/research/"):
        stem = "docs/content" + path.rstrip("/")
        matches = [candidate for candidate in (stem + ".mdx", stem + "/index.mdx")
                   if (repo / candidate).is_file()]
        if len(matches) != 1:
            raise CoverageError(f"Anchor route has {len(matches)} destinations: {anchor}")
        path = matches[0]
    if not path.startswith("docs/") or ".." in Path(path).parts:
        raise CoverageError(f"Unsupported anchor path: {anchor}")
    return path, fragment if mark else ""


def destination_record(value, baseline, repo, prose_disposition=False):
    document = value.get("replacement_document", value.get("current_document", baseline["document"]))
    if not isinstance(document, str) or not document.startswith("docs/") or ".." in Path(document).parts:
        raise CoverageError("Destination must be a repository docs path")
    if not (repo / document).is_file():
        raise CoverageError(f"Missing destination {document}")
    disposition = value.get("disposition") or value.get("status")
    reason = value.get("evidence_or_reason") or value.get("rationale") or value.get("reason")
    if not reason and prose_disposition:
        reason = disposition
    if not isinstance(disposition, str) or not disposition.strip() or disposition == "pending":
        raise CoverageError("Missing explicit disposition")
    if not isinstance(reason, str) or not reason.strip():
        raise CoverageError("Missing authored rationale; a generic status is not evidence")
    candidates = value.get("current_headings")
    if candidates is None:
        candidate = next((value[key] for key in (
            "current_heading", "replacement_heading", "new_heading", "new_section", "replacement_section"
        ) if value.get(key)), None)
        candidates = [] if candidate is None else [candidate]
    if not isinstance(candidates, list) or any(not isinstance(x, str) for x in candidates):
        raise CoverageError("current_headings must be an explicit string array")
    candidates = [normalize_heading(candidate) for candidate in candidates]
    if len(candidates) != len(set(candidates)):
        raise CoverageError("Duplicate destination headings within one record")
    supplied_anchor = value.get("current_anchor")
    if supplied_anchor is not None and not isinstance(supplied_anchor, str):
        raise CoverageError("current_anchor must be a string")
    supplied_anchor = supplied_anchor or ""
    if not candidates:
        if not value.get("whole_artifact") or baseline["heading"] != "Whole artifact":
            raise CoverageError("Missing exact destination heading; no fallback permitted")
        if supplied_anchor and anchor_document(supplied_anchor, repo) != (document, ""):
            raise CoverageError("Whole-artifact anchor does not identify the whole destination")
        anchors = [supplied_anchor or document]
        primary_heading, primary_anchor = "", anchors[0]
    else:
        available, anchors = document_headings(repo, document), []
        for candidate in candidates:
            if candidate not in available:
                raise CoverageError(f"Missing exact destination heading: {candidate!r}")
            slugs = available[candidate]
            if len(slugs) != 1:
                raise CoverageError(f"Ambiguous duplicate heading requires explicit structural mapping: {candidate!r}")
            anchors.append(f"{document}#{slugs[0]}")
        primary_heading, primary_anchor = candidates[0], anchors[0]
        if supplied_anchor:
            supplied_path, supplied_fragment = anchor_document(supplied_anchor, repo)
            normalized = f"{supplied_path}#{supplied_fragment}"
            if normalized not in anchors:
                raise CoverageError(f"Supplied anchor disagrees with exact heading mapping: {supplied_anchor}")
            index = anchors.index(normalized)
            anchors[index] = supplied_anchor
            primary_heading, primary_anchor = candidates[index], supplied_anchor
    return {
        **baseline, "workstream": value.get("workstream", ""),
        "disposition": disposition.strip(), "current_document": document,
        "current_heading": primary_heading, "current_headings": candidates,
        "current_anchor": primary_anchor, "current_anchors": anchors,
        "evidence_or_reason": reason.strip(),
    }


def main():
    census_path = HERE / "baseline-census.csv"
    rows = list(csv.DictReader(census_path.open()))
    if not rows or len({row["id"] for row in rows}) != len(rows):
        raise CoverageError("Baseline census must have nonempty unique IDs")
    if len({row["baseline_commit"] for row in rows}) != 1:
        raise CoverageError("Baseline census has inconsistent commits")
    records, duplicates, errors, inventory = {}, set(), [], []
    for relative, selector, prose_rationale in CANONICAL_LEDGERS:
        ledger = PACKS / relative
        ledger_path = str(ledger.relative_to(REPO))
        if not ledger.is_file():
            errors.append({"ledger": ledger_path, "reason": "Missing required canonical ledger"})
            continue
        raw = ledger.read_bytes()
        try:
            data = json.loads(raw)
            for key in ("baseline_commit", "base_commit", "base_revision"):
                if key in data and data[key] != rows[0]["baseline_commit"]:
                    raise CoverageError(f"Ledger has conflicting {key}")
            entries = list(selected_records(data, selector))
        except (ValueError, TypeError) as error:
            errors.append({"ledger": ledger_path, "reason": str(error)})
            continue
        accepted = 0
        for value, source_index in entries:
            baseline = None
            try:
                baseline = identify_baseline(value, rows)
                result = destination_record(value, baseline, REPO, prose_rationale)
                result["workstream"] = result["workstream"] or ledger.parent.name
                result["detail_ledger"], result["detail_selector"] = ledger_path, source_index
                census_id = baseline["id"]
                if census_id in records or census_id in duplicates:
                    earlier = records.pop(census_id, None)
                    duplicates.add(census_id)
                    comparable = lambda item: {key: item[key] for key in (
                        "disposition", "current_document", "current_headings", "current_anchors", "evidence_or_reason"
                    )}
                    same = earlier is not None and comparable(earlier) == comparable(result)
                    errors.append({
                        "id": census_id, "ledger": ledger_path, "selector": source_index,
                        "reason": "Duplicate identical canonical record" if same else "Conflicting/duplicate canonical records",
                        "earlier_ledger": earlier["detail_ledger"] if earlier else "Already duplicated",
                    })
                    continue
                records[census_id] = result
                accepted += 1
            except CoverageError as error:
                errors.append({
                    "id": baseline["id"] if baseline else value.get("id"),
                    "ledger": ledger_path, "selector": source_index, "reason": str(error),
                })
        inventory.append({
            "path": ledger_path, "selector": selector, "selected_records": len(entries),
            "accepted_records_before_duplicate_resolution": accepted,
            "sha256": hashlib.sha256(raw).hexdigest(),
            "prose_disposition_is_authored_rationale": prose_rationale,
        })
    missing = [row for row in rows if row["id"] not in records]
    fields = list(rows[0]) + [
        "workstream", "disposition", "current_document", "current_heading", "current_headings",
        "current_anchor", "current_anchors", "evidence_or_reason", "detail_ledger", "detail_selector"
    ]
    with (HERE / "coverage-inventory.csv").open("w", newline="") as out:
        writer = csv.DictWriter(out, fieldnames=fields)
        writer.writeheader()
        for row in rows:
            result = dict(records.get(row["id"], {**row, "workstream": "unresolved", "disposition": "pending"}))
            for field in ("current_headings", "current_anchors"):
                if field in result:
                    result[field] = json.dumps(result[field], ensure_ascii=False)
            writer.writerow(result)
    summary = {
        "baseline_commit": rows[0]["baseline_commit"], "cutoff_utc": "2026-10-24T01:15:00Z",
        "census_sha256": hashlib.sha256(census_path.read_bytes()).hexdigest(),
        "reconciliation": "Explicit canonical arrays; exact baseline identity, heading and anchor checks; no fuzzy, substring, old-heading or Summary fallback; duplicates are errors.",
        "census_entries": len(rows), "documents": len({row["document"] for row in rows}),
        "mdx_documents": len({row["document"] for row in rows if row["document"].endswith(".mdx")}),
        "mapped_entries": len(records), "missing_entries": missing,
        "invalid_destinations": errors, "duplicate_ids": sorted(duplicates),
        "workstream_ledgers": inventory,
        "meaning": "Every accepted census entry maps to an explicit authored disposition and exact destination. Split destinations are retained as arrays. This verifies mapping provenance and integrity, not scientific proof, full within-section coverage or paid execution.",
    }
    (HERE / "coverage-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({key: summary[key] for key in ("census_entries", "mapped_entries", "documents", "mdx_documents")}))
    print("missing_ids", [row["id"] for row in missing])
    print("errors", json.dumps(errors, indent=2))
    return bool(missing or errors)


if __name__ == "__main__":
    raise SystemExit(main())
