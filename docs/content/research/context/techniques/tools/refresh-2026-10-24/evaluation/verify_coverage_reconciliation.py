"""Focused negative controls for the strict coverage reconciler; no network."""
from pathlib import Path
import contextlib
import csv
import hashlib
import importlib.util
import io
import json
import re
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
SCRIPT = HERE.parent / "integration/reconcile_coverage.py"
spec = importlib.util.spec_from_file_location("coverage_under_review", SCRIPT)
coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coverage)
checks = {}


def rejects(name, function):
    try:
        function()
    except coverage.CoverageError:
        checks[name] = True
    else:
        raise AssertionError(name + " was silently accepted")


with tempfile.TemporaryDirectory(prefix="jackin-coverage-guards-") as temporary:
    repo = Path(temporary)
    document = "docs/content/research/test.mdx"
    target = repo / document
    target.parent.mkdir(parents=True)
    target.write_text("# Title\n\n## Summary\n\n## Actual result\n")
    baseline = {
        "id": "S0001", "baseline_commit": "a" * 40, "document": document,
        "line": "3", "heading": "## Summary", "blob_sha256": "b" * 64,
    }
    value = {
        **baseline, "disposition": "changed", "current_heading": "Summary",
        "evidence_or_reason": "Explicit reviewed destination and reason.",
    }
    checks["exact_destination_passes"] = coverage.destination_record(value, baseline, repo)["current_heading"] == "Summary"
    rejects("misspelling_is_not_fuzzy_mapped", lambda: coverage.destination_record(dict(value, current_heading="Summry"), baseline, repo))
    rejects("substring_is_not_mapped", lambda: coverage.destination_record(dict(value, current_heading="Actual result and another section"), baseline, repo))
    missing = {key: item for key, item in value.items() if key != "current_heading"}
    rejects("missing_heading_has_no_summary_fallback", lambda: coverage.destination_record(missing, baseline, repo))
    rejects("conflicting_anchor_rejected", lambda: coverage.destination_record(dict(value, current_heading="Actual result", current_anchor=document + "#summary"), baseline, repo))
    rejects("conflicting_id_document_rejected", lambda: coverage.identify_baseline(dict(value, document="docs/other.mdx"), [baseline]))
    rejects("unknown_census_id_rejected", lambda: coverage.identify_baseline(dict(value, id="S9999"), [baseline]))
    rejects("generic_status_is_not_rationale", lambda: coverage.destination_record({key: item for key, item in value.items() if key != "evidence_or_reason"}, baseline, repo))
    route = "/research/test/#actual-result"
    routed = coverage.destination_record(dict(value, current_heading="Actual result", current_anchor=route), baseline, repo)
    checks["valid_author_route_preserved"] = routed["current_anchor"] == route
    split = coverage.destination_record(dict(value, current_headings=["Summary", "Actual result"]), baseline, repo)
    checks["split_destinations_preserved"] = split["current_headings"] == ["Summary", "Actual result"] and len(split["current_anchors"]) == 2
    payload = {"headings": [value], "numerical_claim_candidates": [dict(value, disposition="numeric overwrite")]}
    selected = list(coverage.selected_records(payload, "headings"))
    checks["numeric_array_excluded"] = len(selected) == 1 and selected[0][0]["disposition"] == "changed"

    saved = {name: getattr(coverage, name) for name in ["HERE", "REPO", "PACKS", "CANONICAL_LEDGERS"]}
    pack = repo / "docs/packs"
    integration = pack / "integration"
    integration.mkdir(parents=True)
    with (integration / "baseline-census.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(baseline))
        writer.writeheader()
        writer.writerow(baseline)
    (pack / "one.json").write_text(json.dumps({"headings": [value]}))
    try:
        coverage.HERE, coverage.REPO, coverage.PACKS = integration, repo, pack
        coverage.CANONICAL_LEDGERS = (("one.json", "headings", False),)
        with contextlib.redirect_stdout(io.StringIO()):
            checks["valid_small_inventory_passes"] = not coverage.main()
        for name, second in [
            ("identical_duplicate_fails", value),
            ("conflicting_duplicate_fails", dict(value, current_heading="Actual result")),
        ]:
            (pack / "two.json").write_text(json.dumps({"headings": [second]}))
            coverage.CANONICAL_LEDGERS = (("one.json", "headings", False), ("two.json", "headings", False))
            with contextlib.redirect_stdout(io.StringIO()):
                failed = coverage.main()
            summary = json.loads((integration / "coverage-summary.json").read_text())
            checks[name] = failed and summary["duplicate_ids"] == ["S0001"] and summary["mapped_entries"] == 0
        (pack / "one.json").write_text(json.dumps({"headings": [], "numerical_claim_candidates": [value]}))
        coverage.CANONICAL_LEDGERS = (("one.json", "headings", False),)
        with contextlib.redirect_stdout(io.StringIO()):
            failed = coverage.main()
        checks["numeric_only_cannot_complete_inventory"] = failed and json.loads((integration / "coverage-summary.json").read_text())["mapped_entries"] == 0
    finally:
        for name, item in saved.items():
            setattr(coverage, name, item)

assert all(checks.values()), checks
integration = HERE.parent / "integration"
summary = json.loads((integration / "coverage-summary.json").read_text())
rows = list(csv.DictReader((integration / "coverage-inventory.csv").open()))
assert summary["mapped_entries"] == summary["census_entries"] == len(rows) == 905
assert not summary["missing_entries"] and not summary["invalid_destinations"] and not summary["duplicate_ids"]
assert len({row["id"] for row in rows}) == 905
assert all("numerical" not in row["detail_selector"] and "material" not in row["detail_selector"] for row in rows)
for ledger in summary["workstream_ledgers"]:
    assert hashlib.sha256((coverage.REPO / ledger["path"]).read_bytes()).hexdigest() == ledger["sha256"]

# Compare every generated document anchor with the actual site slugger, with a
# Markdown/MDX parser deriving heading text independently from the Python code.
documents = sorted({row["current_document"] for row in rows if Path(row["current_document"]).suffix in {".md", ".mdx"}})
node_script = r"""
import fs from 'node:fs';
const root = process.argv[1];
const {unified} = await import(root + '/docs/node_modules/unified/index.js');
const {default: remarkParse} = await import(root + '/docs/node_modules/remark-parse/index.js');
const {default: remarkMdx} = await import(root + '/docs/node_modules/remark-mdx/index.js');
const {toString} = await import(root + '/docs/node_modules/mdast-util-to-string/index.js');
const {default: GithubSlugger} = await import(root + '/docs/node_modules/github-slugger/index.js');
const documents = JSON.parse(fs.readFileSync(0, 'utf8'));
const output = {};
for (const document of documents) {
  const source = fs.readFileSync(root + '/' + document, 'utf8');
  const body = source.replace(/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/, '');
  const tree = unified().use(remarkParse).use(remarkMdx).parse(body);
  const slugger = new GithubSlugger();
  output[document] = {};
  function walk(node) {
    if (node.type === 'heading') {
      const title = toString(node);
      (output[document][title] ||= []).push(slugger.slug(title));
    }
    for (const child of node.children || []) walk(child);
  }
  walk(tree);
}
process.stdout.write(JSON.stringify(output));
"""
node = subprocess.run(
    ["node", "--input-type=module", "-e", node_script, str(coverage.REPO)],
    input=json.dumps(documents), text=True, capture_output=True, check=True,
)
actual = json.loads(node.stdout)
differences = []
for document in documents:
    expected = coverage.document_headings(coverage.REPO, document)
    if actual[document] != expected:
        differences.append({"document": document, "site": actual[document], "reconciler": expected})
assert not differences, [{"document": row["document"], "site_only": sorted(set(row["site"]) - set(row["reconciler"])), "reconciler_only": sorted(set(row["reconciler"]) - set(row["site"]))} for row in differences]
result = {
    "scope": "Original reconciler independently audited, then remediated by this reviewer at root request. These are implementation checks of that remediation, not an independent review of my own authored fix.",
    "reconciler_sha256": hashlib.sha256(SCRIPT.read_bytes()).hexdigest(),
    "inventory_sha256": hashlib.sha256((integration / "coverage-inventory.csv").read_bytes()).hexdigest(),
    "summary_sha256": hashlib.sha256((integration / "coverage-summary.json").read_bytes()).hexdigest(),
    "guard_checks": checks, "guard_check_count": len(checks), "canonical_rows": len(rows),
    "canonical_ledgers": len(summary["workstream_ledgers"]),
    "slugger_compared_documents": len(documents),
    "slugger_compared_heading_occurrences": sum(len(slugs) for headings in actual.values() for slugs in headings.values()),
    "slugger_mismatches": differences, "provider_calls": 0, "network_requests": 0,
}
print(json.dumps(result, indent=2))
