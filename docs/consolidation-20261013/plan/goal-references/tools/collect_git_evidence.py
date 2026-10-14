#!/usr/bin/env python3
"""Read-only local Git evidence collector. Does not fetch, checkout, merge or run project code.
Python 3.10+, Git. Output must be a NEW directory outside the worktree and Git directory.
Patch IDs are syntactic aids, never semantic equivalence or merge approval.
"""
from __future__ import annotations
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import urlsplit, urlunsplit

SHA = re.compile(r"^[0-9a-f]{40,64}$")

class EvidenceError(RuntimeError):
    pass

class Git:
    def __init__(self, repo: Path, timeout: int = 120):
        self.repo = repo.resolve()
        self.timeout = timeout
        self.env = dict(os.environ, GIT_OPTIONAL_LOCKS="0", GIT_TERMINAL_PROMPT="0",
                        GIT_NO_REPLACE_OBJECTS="1", GIT_NO_LAZY_FETCH="1")
        self.prefix = ["git", "--no-optional-locks", "-C", str(self.repo),
                       "-c", "core.fsmonitor=false", "-c", "color.ui=false",
                       "-c", "core.hooksPath=/dev/null", "-c", "log.showSignature=false"]

    def run(self, *args: str, allow: tuple[int, ...] = (0,)) -> tuple[int, bytes]:
        p = subprocess.run(self.prefix + list(args), stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, env=self.env, timeout=self.timeout)
        if p.returncode not in allow:
            raise EvidenceError(f"Git {args[0]} failed ({p.returncode}): "
                                + p.stderr.decode("utf-8", "replace")[:2000])
        return p.returncode, p.stdout

    def text(self, *args: str) -> str:
        return self.run(*args)[1].decode("utf-8", "surrogateescape").strip()

    def patch_ids(self, tips: list[str]) -> dict[str, list[str]]:
        # Pipe patch bytes directly; do not retain potentially sensitive source patches in reports.
        args = ["log", "--no-merges", "--root", "--format=commit %H", "-p", "--binary",
                "--full-index", "--no-ext-diff", "--no-textconv", *tips, "--"]
        with subprocess.Popen(self.prefix + args, stdout=subprocess.PIPE,
                              stderr=subprocess.DEVNULL, env=self.env) as producer:
            assert producer.stdout is not None
            try:
                consumer = subprocess.run(self.prefix + ["patch-id", "--stable"],
                    stdin=producer.stdout, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    env=self.env, timeout=self.timeout)
                producer.stdout.close()
                producer_code = producer.wait(timeout=self.timeout)
            except BaseException:
                producer.kill()
                producer.wait()
                raise
        if producer_code or consumer.returncode:
            raise EvidenceError("patch-ID pipeline failed; missing objects or incomplete history may be the cause")
        groups: dict[str, list[str]] = {}
        for line in consumer.stdout.decode("ascii").splitlines():
            patch, commit = line.split()
            groups.setdefault(patch, []).append(commit)
        return groups


def sanitize_remote(value: str) -> str:
    """Keep useful remote identity but remove embedded HTTP credentials/query strings."""
    if "://" in value:
        u = urlsplit(value)
        host = u.hostname or ""
        if u.port:
            host += f":{u.port}"
        return urlunsplit((u.scheme, host, u.path, "", ""))
    # Standard scp syntax is usually git@host:path, but do not retain an arbitrary username.
    return re.sub(r"^[^/@]+@", "[user]@", value)


def tree_manifest(git: Git, sha: str) -> list[dict[str, str]]:
    rows = []
    for entry in git.run("ls-tree", "-r", "-z", sha)[1].split(b"\0"):
        if not entry:
            continue
        meta, path = entry.split(b"\t", 1)
        mode, kind, oid = meta.decode("ascii").split()
        rows.append({"mode": mode, "type": kind, "oid": oid,
                     "path": path.decode("utf-8", "surrogateescape")})
    return rows


def file_changes(git: Git, a: str, b: str) -> list[dict[str, str]]:
    fields = iter(git.run("diff", "--no-ext-diff", "--no-textconv", "--name-status",
                          "-M", "-C", "-z", a, b, "--")[1].split(b"\0"))
    rows = []
    for item in fields:
        if not item:
            continue
        status = item.decode("ascii")
        path = next(fields).decode("utf-8", "surrogateescape")
        row = {"status": status, "path": path}
        if status.startswith(("R", "C")):
            row["destination"] = next(fields).decode("utf-8", "surrogateescape")
        rows.append(row)
    return rows


def relationship(git: Git, a: str, b: str) -> dict:
    code, raw = git.run("merge-base", "--all", a, b, allow=(0, 1))
    behind, ahead = map(int, git.text("rev-list", "--left-right", "--count", f"{a}...{b}").split())
    return {"base": a, "head": b, "merge_bases": raw.decode().split(),
            "head_ahead": ahead, "head_behind": behind, "related": code == 0}


def collect(repo: Path, target: str, output: Path, workers: int = 4,
            pairwise: bool = False, patch_ids: bool = False) -> dict:
    git = Git(repo)
    root = Path(git.text("rev-parse", "--show-toplevel")).resolve()
    gitdir = Path(git.text("rev-parse", "--absolute-git-dir")).resolve()
    common = Path(git.text("rev-parse", "--git-common-dir"))
    common = (git.repo / common).resolve() if not common.is_absolute() else common.resolve()
    output = output.resolve()
    if any(output == p or p in output.parents for p in (root, gitdir, common)):
        raise EvidenceError("Output must be outside the repository worktree and Git directory")
    if output.exists():
        raise EvidenceError("Refusing to overwrite an existing evidence directory")
    target = git.text("rev-parse", "--verify", "--end-of-options", target + "^{commit}")
    if not SHA.fullmatch(target):
        raise EvidenceError("Target did not resolve to a full commit ID")
    head_before = git.text("rev-parse", "HEAD")
    status_before = git.run("status", "--porcelain=v2", "-z", "--untracked-files=all")[1]
    refs_text = git.text("for-each-ref", "--format=%(refname)%00%(objectname)%00%(upstream)%00%(symref)")
    refs = []
    for line in refs_text.splitlines():
        name, oid, upstream, symref = line.split("\0")
        refs.append({"ref": name, "tip": oid, "upstream": upstream, "symbolic_target": symref})
    branches = [r for r in refs if r["ref"].startswith(("refs/heads/", "refs/remotes/"))
                and not r["symbolic_target"]]
    tips = sorted({target, *(r["tip"] for r in branches)})
    shallow = git.text("rev-parse", "--is-shallow-repository") == "true"
    remotes = {}
    for name in git.text("remote").splitlines():
        remotes[name] = [sanitize_remote(v) for v in git.text("remote", "get-url", "--all", name).splitlines()]
    output.mkdir(parents=True, exist_ok=False)
    (output / "tips").mkdir()
    errors: list[str] = []
    metadata = {}
    for line in git.text("log", "--format=%H%x00%P%x00%T%x00%s", *tips, "--").splitlines():
        sha, parents, tree, subject = line.split("\0", 3)
        metadata[sha] = {"parents": parents.split(), "tree": tree, "subject": subject}

    def inspect_tip(sha: str) -> dict:
        parents = metadata[sha]["parents"]
        result = {"sha": sha, "graph": relationship(git, target, sha),
                  "unique_commits": git.text("rev-list", "--reverse", "--topo-order", f"{target}..{sha}").splitlines(),
                  "target_to_tip_files": file_changes(git, target, sha),
                  "tree": tree_manifest(git, sha), "parents": parents,
                  "parent_deltas": [{"parent": parent, "files": file_changes(git, parent, sha)} for parent in parents],
                  "semantic_coverage": "NOT_REVIEWED"}
        if len(parents) >= 3:
            # A multi-parent commit is not necessarily a stash. Retain every parent tree without relabeling it.
            result["multi_parent_trees"] = {parent: tree_manifest(git, parent) for parent in parents}
        (output / "tips" / f"{sha}.json").write_text(json.dumps(result, indent=2) + "\n")
        return {"sha": sha, "unique_count": len(result["unique_commits"]), "graph": result["graph"]}

    summaries = []
    with ThreadPoolExecutor(max_workers=max(1, min(workers, 10))) as pool:
        for summary in pool.map(inspect_tip, tips):
            summaries.append(summary)
    pairs = []
    if pairwise:
        with ThreadPoolExecutor(max_workers=max(1, min(workers, 10))) as pool:
            pairs = list(pool.map(lambda pair: relationship(git, *pair), itertools.combinations(tips, 2)))
    patches = git.patch_ids(tips) if patch_ids else {}
    if shallow:
        errors.append("SHALLOW_HISTORY: cannot certify ancestry or complete commit coverage")
    status_after = git.run("status", "--porcelain=v2", "-z", "--untracked-files=all")[1]
    if status_before != status_after:
        errors.append("WORKTREE_CHANGED_DURING_COLLECTION: snapshot is not stable")
    head_after = git.text("rev-parse", "HEAD")
    refs_after = git.text("for-each-ref", "--format=%(refname)%00%(objectname)%00%(upstream)%00%(symref)")
    refs_unchanged = refs_text == refs_after and head_before == head_after
    if not refs_unchanged:
        errors.append("REFS_CHANGED_DURING_COLLECTION: HEAD or refs changed; repeat from a stable snapshot")
    report = {"generated_at_utc": datetime.now(timezone.utc).isoformat(), "repository": str(root),
              "target": target, "head": head_before, "head_after": head_after,
              "refs_unchanged": refs_unchanged, "remotes": remotes,
              "shallow": shallow, "refs": refs, "branches": branches, "tip_summaries": summaries,
              "pairwise_enabled": pairwise, "pairwise": pairs,
              "patch_ids_enabled": patch_ids, "patch_id_groups": patches,
              "commit_metadata": metadata, "worktree_status_unchanged": status_before == status_after,
              "status_porcelain_v2": status_before.decode("utf-8", "surrogateescape").split("\0"),
              "errors": errors,
              "limitations": ["No fetch; remote-tracking freshness must be independently established.",
                              "No project tests or semantic code review.",
                              "Non-merge patch IDs do not prove semantic equivalence, squash preservation, or merge resolution correctness.",
                              "All-parent trees are saved for multi-parent tips; inspect internal stash/merge commits from commit_metadata too.",
                              "Unreachable objects and user reflogs are not inferred or discarded."]}
    (output / "inventory.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--repo", type=Path, default=Path.cwd())
    ap.add_argument("--target", required=True)
    ap.add_argument("--output", required=True, type=Path)
    ap.add_argument("--workers", type=int, default=4, choices=range(1, 11))
    ap.add_argument("--pairwise", action="store_true")
    ap.add_argument("--patch-ids", action="store_true")
    ns = ap.parse_args()
    try:
        result = collect(ns.repo, ns.target, ns.output, ns.workers, ns.pairwise, ns.patch_ids)
    except (EvidenceError, OSError, subprocess.SubprocessError, ValueError) as exc:
        print(f"BLOCKED: {exc}", file=sys.stderr)
        return 2
    print(json.dumps({"branches": len(result['branches']), "tips": len(result['tip_summaries']),
                      "output": str(ns.output.resolve()), "errors": result['errors']}))
    return 2 if result["errors"] else 0

if __name__ == "__main__":
    raise SystemExit(main())
