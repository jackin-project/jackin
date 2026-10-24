"""Offline source-level boundary probe; no coding-agent/model call."""
import concurrent.futures
import json
import os
import pathlib
import subprocess
import tempfile
import time

from lemoncrow_client.lite.mcp import LiteMcp


def git(root, *args):
    subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True)


def text(result):
    return "\n".join(block.get("text", "") for block in result.get("content", []))


def main():
    with tempfile.TemporaryDirectory(prefix="lc-boundaries-") as tmp:
        tmp = pathlib.Path(tmp)
        root = tmp / "main"
        root.mkdir()
        git(root, "init", "-q")
        git(root, "config", "user.name", "Offline fixture")
        git(root, "config", "user.email", "fixture@example.invalid")
        (root / "app.py").write_text("def selected_value():\n    return 'MAIN'\n")
        (root / "report.py").write_text("for i in range(70): print(f'record {i:03d} keeps exact observation ABCDEFGHIJKLMNOPQRSTUVWXYZ')\n")
        git(root, "add", ".")
        git(root, "commit", "-qm", "fixture")
        a, b = tmp / "a", tmp / "b"
        git(root, "worktree", "add", "-qb", "tree-a", str(a))
        git(root, "worktree", "add", "-qb", "tree-b", str(b))
        (a / "app.py").write_text("def selected_value():\n    return 'TREE_A'\n")
        (b / "app.py").write_text("def selected_value():\n    return 'TREE_B'\n")
        def server(path):
            return LiteMcp(environment={"LEMONCROW_WORKSPACE_ROOT": str(path), "LEMONCROW_HOME": str(tmp / "state"), "LEMONCROW_RUNTIME_TRACE": "off"})
        sa, sb, shared = server(a), server(b), server(root)
        try:
            started = time.perf_counter()
            sa.prepare()
            cold_ms = (time.perf_counter() - started) * 1000
            started = time.perf_counter()
            sa.prepare()
            warm_ms = (time.perf_counter() - started) * 1000
            with concurrent.futures.ThreadPoolExecutor(2) as pool:
                results = list(pool.map(lambda s: text(s.call("read", {"files": ["app.py"]})), [sa, sb]))
            assert "TREE_A" in results[0] and "TREE_B" not in results[0]
            assert "TREE_B" in results[1] and "TREE_A" not in results[1]
            print(json.dumps({"probe": "explicit_roots_parallel", "passed": True, "results": results}))
            shared.call("bash", {"command": "pwd", "cwd": str(a)})
            relative = text(shared.call("read", {"files": ["app.py"]}))
            absolute = text(shared.call("read", {"files": [str(a / "app.py")]}))
            assert "MAIN" in relative and "TREE_A" in absolute
            print(json.dumps({"probe": "shared_process_no_sticky_cwd", "relative_uses_main": True, "explicit_uses_worktree_a": True}))
            mixed = shared.call("read", {"files": [str(a / "app.py"), str(b / "app.py")]})
            assert mixed.get("isError")
            print(json.dumps({"probe": "mixed_worktrees_refused", "text": text(mixed)}))
            schemas = shared.handle({"id": 1, "method": "tools/list"})["result"]["tools"]
            names = [x["name"] for x in schemas]
            print(json.dumps({"probe": "advertised_surface", "names": names, "compact_json_utf8_bytes": len(json.dumps(schemas, ensure_ascii=False, separators=(",", ":")).encode())}))
            hidden = shared.call("grep", {"pattern": "selected_value", "paths": ["app.py"]})
            print(json.dumps({"probe": "unadvertised_grep_callable", "is_error": hidden.get("isError"), "text": text(hidden)}))
            os.environ["LEMONCROW_MCP_SPILL_DIR"] = str(tmp / "spills")
            first = text(shared.call("bash", {"command": "python3 report.py"}))
            second = text(shared.call("bash", {"command": "python3 report.py"}))
            assert "unchanged:" in second
            blocker = tmp / "not-a-directory"
            blocker.write_text("blocks spill creation")
            os.environ["LEMONCROW_MCP_SPILL_DIR"] = str(blocker)
            third = text(shared.call("bash", {"command": "python3 report.py"}))
            assert "unchanged:" in third
            print(json.dumps({"probe": "repeat_with_spill_failure", "first_chars": len(first), "repeat_with_recovery": second, "repeat_when_spill_unavailable": third}))
            print(json.dumps({"probe": "tiny_fixture_index_timings", "files": 2, "cold_ms": cold_ms, "warm_ms": warm_ms, "not_task_benchmark": True}))
        finally:
            for s in (sa, sb, shared):
                s.close()


if __name__ == "__main__":
    main()
