import concurrent.futures
import importlib.util
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path("/tmp/jackin-a15-final-review")
AUDIT = pathlib.Path("/tmp/jackin-a15-e2e-t01/.audit")
LOGS = AUDIT / "a15-final-review-logs"
RESULTS = AUDIT / "a15-final-review-results.tsv"
PRIOR = AUDIT / "a15-final-review-prior-results.tsv"
HOOK = AUDIT / "a15-final-review-hook.py"
TRACKED = ["crates/xtask/src/e2e.rs", "crates/xtask/src/e2e/target.rs"]
NEW = ROOT / "crates/xtask/src/e2e/a15mut.rs"
DIRECT_FILTER = "test(~every_target_identity_input_changes_the_digest)"


def run(args, env=None):
    return subprocess.run(args, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)


def restore():
    subprocess.run(["rtk", "git", "checkout", "HEAD", "--", *TRACKED], cwd=ROOT, check=True)
    NEW.unlink(missing_ok=True)
    status = run(["rtk", "git", "status", "--porcelain"])
    if status.returncode != 0 or status.stdout.strip():
        raise RuntimeError(f"restore failed: {status.returncode} {status.stdout}")


def apply_hook():
    spec = importlib.util.spec_from_file_location("a15_final_review_hook", HOOK)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    for rel, old, new in module.PATCHES:
        path = ROOT / rel
        text = path.read_text()
        count = text.count(old)
        if count != 1:
            raise RuntimeError(f"patch match count {count} in {rel}: {old[:100]!r}")
        path.write_text(text.replace(old, new))
    for rel, body in module.NEWFILES:
        (ROOT / rel).write_text(body)


def nextest(filter_expr, env, log):
    args = ["rtk", "mise", "exec", "--", "mbx", "+1.99.1", "nextest", "run", "-p", "jackin-xtask", "-E", filter_expr, "--no-fail-fast"]
    with log.open("w") as output:
        proc = subprocess.run(args, cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT)
    text = log.read_text()
    fails = len({line.strip() for line in text.splitlines() if line.lstrip().startswith("FAIL [")})
    passes = len({line.strip() for line in text.splitlines() if line.lstrip().startswith("PASS [")})
    compile_failure = "could not compile" in text or "error[E" in text
    return proc.returncode, passes, fails, compile_failure


def red_state(result):
    code, _, fails, compile_failure = result
    if compile_failure:
        return "COMPILE"
    if code == 0:
        return "NOT-RED"
    if code != 100 or fails < 1:
        return "INVALID-RED"
    return "RED"


def green_state(result):
    code, passes, fails, _ = result
    if code == 0 and passes == 1 and fails == 0:
        return "GREEN"
    return "GREEN-FAIL"


def execute(row):
    row_id, pkg, filter_expr, envspec = row
    env = os.environ.copy()
    for item in envspec.split("|"):
        if item:
            key, value = item.split("=", 1)
            env[key] = value
    red_log = LOGS / f"{row_id}.red.log"
    green_log = LOGS / f"{row_id}.green.log"
    red = nextest(filter_expr, env, red_log)
    green = nextest(filter_expr, None, green_log)
    return [row_id, pkg, filter_expr, envspec, f"red={red_state(red)} exit={red[0]} pass={red[1]} fail={red[2]}", f"green={green_state(green)} exit={green[0]} pass={green[1]} fail={green[2]}", "instrumentation-dormant", str(red_log), str(green_log)]


def execute_direct(row):
    row_id, pkg, _, envspec = row
    env = os.environ.copy()
    for item in envspec.split("|"):
        if item:
            key, value = item.split("=", 1)
            env[key] = value
    red_log = LOGS / f"{row_id}.red.log"
    green_log = LOGS / f"{row_id}.green.log"
    red = nextest(DIRECT_FILTER, env, red_log)
    return row_id, pkg, envspec, red, red_log, green_log


def append(fields):
    with RESULTS.open("a") as output:
        output.write("\t".join(fields) + "\n")


def main():
    LOGS.mkdir(parents=True, exist_ok=True)
    prior = [line.rstrip("\n").split("\t")[:4] for line in PRIOR.read_text().splitlines() if line.strip()]
    ordinary = prior[:49] + prior[50:66]
    direct_rows = [prior[49]] + prior[66:70]
    if len(ordinary) != 65 or len(direct_rows) != 5:
        raise RuntimeError("the approved matrix does not split into 65 ordinary and 5 direct rows")
    status = run(["rtk", "git", "status", "--porcelain"])
    if status.returncode != 0 or status.stdout.strip():
        raise RuntimeError(f"worktree is not clean: {status.stdout}")
    RESULTS.write_text("id\tpkg\tfilter\tenvspec\tred\tgreen\tgreen_mode\tred_log\tgreen_log\n")
    apply_hook()
    direct = []
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
            ordinary_results = {result[0]: result for result in pool.map(execute, ordinary)}
        direct = [execute_direct(row) for row in direct_rows]
    finally:
        restore()
    direct_results = {}
    for row_id, pkg, envspec, red, red_log, green_log in direct:
        green = nextest(DIRECT_FILTER, None, green_log)
        direct_results[row_id] = [row_id, pkg, DIRECT_FILTER, envspec, f"red={red_state(red)} exit={red[0]} pass={red[1]} fail={red[2]}", f"green={green_state(green)} exit={green[0]} pass={green[1]} fail={green[2]}", "restored-clean", str(red_log), str(green_log)]
    rows = ordinary[:49] + direct_rows[:1] + ordinary[49:] + direct_rows[1:]
    for row in rows:
        result = direct_results.get(row[0]) or ordinary_results[row[0]]
        append(result)
    print(f"completed {len(rows)} rows; source restored")


if __name__ == "__main__":
    sys.exit(main())
