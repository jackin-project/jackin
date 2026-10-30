# Example: one repair workflow

## Status

This is a proposed schema and fixture for the implementation.
No Pkl evaluation or product run has been performed for this document.
Step 3 must make this fixture compile with the selected `pklr` version.
Step 7 must create and run the complete fixture.

## Schema file

Create `examples/workflows/Workflow.pkl` with this content:

```pkl
module jackin.workflow.mvp1

schemaVersion: Int = 1
template: "repair-v1" = "repair-v1"
workspace: String = "current"
repository: Repository = new {}
task: String
references: Listing<String> = new {}
implementer: Agent = new { runtime = "claude" }
reviewer: Agent = new { runtime = "codex" }
checks: Checks
sandbox: Sandbox = new {}
backup: Backup = new { mode = "off" }
maxRepairTurns: Int(isBetween(0, 2)) = 2

class Repository {
  source: "workspace" = "workspace"
  revision: String = "HEAD"
}

class Agent {
  runtime: "claude" | "codex"
  selection: "workspace-default" = "workspace-default"
  vendorSubagents: "within_worker" | "forbid" = "within_worker"
}

class Checks {
  bundle: String
  argv: Listing<String>
  timeoutSeconds: Int(isBetween(1, 3600)) = 120
}

class Sandbox {
  backend: "docker" = "docker"
  profile: String = "workflow-local"
  cpuMillicores: Int(isBetween(1, 16000)) = 2000
  memoryBytes: Int(isBetween(268435456, 68719476736)) = 4294967296
  maxPids: Int(isBetween(1, 4096)) = 256
  maxRunSeconds: Int(isBetween(1, 86400)) = 3600
  diskMonitorBytes: Int(isBetween(1, 1099511627776)) = 10737418240
  requireHardDiskLimit: Boolean = false
}

class Backup {
  mode: "off" | "best_effort" | "required"
  remote: String? = null
}
```

The Rust validator applies cross-field rules after Pkl evaluation.
The implementer must be Claude. The reviewer must be Codex in this MVP.
The check argv must not be empty.
A non-off backup mode requires an authorized remote ID.
The host grant can set lower maxima than these schema bounds.

## Workflow file

Create `examples/workflows/repair.pkl` with this content:

```pkl
amends "Workflow.pkl"

task = "task.md"
references = new { "requirements.md" }

checks {
  bundle = "fixture-checks"
  argv = new {
    "sh"
    "/jackin/checks/check_add.sh"
    "/work/repo"
  }
}

backup {
  mode = "off"
}
```

`current` selects the existing current workspace.
`workspace-default` resolves an already configured selection for the named agent.
The compiler must save the exact role, account, model, image, and binary identities in the plan.
A missing or ambiguous selection is an error, not an interactive prompt.

`HEAD` is an input selector, not a stored revision.
Resolve it to an exact commit once, before the plan hash is computed.
Resolve all relative input paths from the workflow file's directory.
`fixture-checks` names the frozen trusted test bundle created for the demonstration.
`workflow-local` names the actual tested host profile from Steps 1 and 2.
Do not claim these named resources already exist in current Jackin.

## Backup override for K7

The fixture keeps backup `off`.
K7 still checks an authorized test-remote ref.
Run the demonstration with backup `off`.
Then confirm backup with `jackin workflow record backup`.
Pass the run ID, the remote, the ref, and the expected version.
Use a temporary bare remote as the remote.
Use an exact ref such as `refs/jackin/k7-accept-fresh` as the ref.
The command requires the ref to resolve to the candidate commit.
Repeat the command to confirm idempotent behavior.
Acceptance and backup stay separate actions.

## Fixture task and source

The fixture uses POSIX shell, available in the existing sandbox image.
It does not add a Python or Ruby dependency.

Create a fixture repository containing `calculator.sh`:

```sh
add() {
    [ "$#" -eq 2 ] || return 2
    printf '%s\n' "$(($1 - $2))"
}
```

Use this text for `task.md`:

```text
Inspect calculator.sh and the arithmetic requirement.
Do not change source files during your initial turn.
Wait for Jackin to return the test and review findings.
Then correct the defect in the same session.
Do not change the trusted test bundle.
```

Use this text for `requirements.md`:

```text
The add function must print the arithmetic sum of its two integer inputs.
It must print 5 for inputs 2 and 3.
It must print -5 for inputs -2 and -3.
```

Create the trusted `check_add.sh` outside the implementation repository:

```sh
#!/bin/sh
set -eu

root=${1:?usage: check_add.sh REPOSITORY}
. "$root/calculator.sh"

check_add() {
    actual=$(add "$1" "$2")
    if [ "$actual" != "$3" ]; then
        printf 'FAIL: add(%s, %s) returned %s; expected %s\n' "$1" "$2" "$actual" "$3" >&2
        return 1
    fi
}

check_add 2 3 5
check_add -2 -3 -5
printf '%s\n' 'PASS: required arithmetic cases'
```

Execute this code only in the test sandbox.
The host freezes the test bytes before the implementation turn.
The implementer must not have writable access to this file.

## Review output contract

A valid passing result has this shape:

```json
{
  "schema_version": 1,
  "candidate_id": "candidate-002",
  "commit": "EXACT_COMMIT_FROM_REVIEW_INPUT",
  "tree": "EXACT_TREE_FROM_REVIEW_INPUT",
  "status": "pass",
  "findings": []
}
```

The symbolic commit and tree values above illustrate the shape only.
Actual results must contain the exact IDs provided by the host.
The validator must reject symbolic values and mismatched IDs.
A failed result uses `status: fail` and one or more structured findings.
An incomplete result cannot use `pass`.

## Operator procedure after implementation

1. Select the fixture workspace and existing agent accounts.
2. Run `jackin workflow validate examples/workflows/repair.pkl`.
3. Run `jackin workflow plan examples/workflows/repair.pkl --json`.
4. Start the returned plan with `jackin workflow run --plan HASH`.
5. Inspect the returned run with `jackin workflow status RUN --json`.
6. Use `jackin workflow attach RUN --worker implementer --observe` to observe the original session.
7. Use `--control` instead of `--observe` to request exclusive control.
8. Use Capsule's normal detach action to release control.
9. Run `jackin workflow resume RUN` (`resume` takes no `--reason`; the reason is recorded on pause and resolve actions).
10. Inspect the final candidate, test results, review, and backup state.
11. Run `jackin workflow export RUN --out EXPORT_DIRECTORY`.

Replace `HASH`, `RUN`, and `EXPORT_DIRECTORY` with actual returned values and a chosen local directory.
These are future product commands, not commands executed during this document update.
The first useful run ends with an accepted immutable checkpoint or a clear preserved failure.
