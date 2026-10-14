# C19 — Workflow CLI

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Add the `jackin workflow` namespace as a thin, scriptable client of C18. Keep argument parsing in the CLI layer, request construction in handlers and workflow decisions in the host service.

## 2. Existing foundation and gap

The inspected complete top-level `Command` enum has no Workflow variant. Existing CLI organization separates parsing from application routing and uses tuple-style command variants. Add a `Workflow(WorkflowCommand)` variant, `cli/workflow.rs`, handlers and help/docs in the current `crates/apps/jackin` layout. These commands are new targets, not currently available. [J20](#evidence-and-references).

## 3. Target command surface

| Command | Required behavior |
|---|---|
| `jackin workflow validate <source.pkl>` | Show syntax/type/graph/policy/capability diagnostics; no worker execution |
| `jackin workflow plan <source.pkl>` | Resolve approved inputs, store immutable plan, print hash and permission/resource summary |
| `jackin workflow run --plan <hash> [--wait]` | Start exactly the stored plan; print run identity and attach/status hints |
| `jackin workflow list` | Filter/paginate runs by workflow/state/workspace |
| `jackin workflow status <run>` | Show state/version, active nodes, candidate revision, gates and retained work |
| `jackin workflow graph <run-or-plan>` | Render source or expanded graph in terminal/JSON/Mermaid text |
| `jackin workflow attach <run> --node <id> --observe` | Connect to the exact original Capsule session read-only |
| `jackin workflow attach <run> --node <id> --control` | Explicit lease/takeover; show resume implications |
| `jackin workflow events <run> [--after <seq>] [--follow]` | Stream correlated events with resumable cursor |
| `jackin workflow pause <run> [--mode drain\|interrupt]` | Persist pause request and report actual boundary progress |
| `jackin workflow resume <run>` | Revalidate state, lease, policy and candidate before resuming |
| `jackin workflow cancel <run> --reason <text>` | Cancel execution while preserving work |
| `jackin workflow retry <run> --node <id>` | Request bounded failed-attempt continuation, never reset plan/counters |
| `jackin workflow approve <run> --gate <id> --checkpoint <id>` | Approve exact evidence/revision/action; never latest-by-guessing |
| `jackin workflow reject <run> --gate <id> --checkpoint <id>` | Record a scoped rejection |
| `jackin workflow export <run> --format markdown\|json\|bundle` | Authorized evidence export with manifest/redaction policy |
| `jackin workflow capabilities` | Inspect certified agent/backend features and reasons for denial |
| `jackin workflow resolve-input <run> --command <id> --resolution <resolution>` | Explicit authorized unknown-delivery resolution, with required reason and new ID for any replacement |

Human output is the default; `--format json` is the stable machine surface for non-terminal commands. IDs/digests are printed in full in JSON. Source parameter overrides are explicit flags/files whose resolved non-secret values become part of the plan; shell environment is not silently imported.

## 4. Completion and exit semantics

Without `--wait`, `run` exits after the host has durably accepted the start request and reports “started,” not “succeeded.” With `--wait`, terminal disposition determines the code. `status` exits successfully when retrieval succeeds even if the run itself failed; the JSON state carries that failure. Interrupted client waiting detaches from observation; it does not silently cancel the host run.

Proposed namespace exit codes: `0` request/completed operation successful, `2` invalid source/arguments/plan, `3` policy/capability denial, `4` waited run failed, `5` waited run cancelled, `6` stale-state/lease conflict, `7` transport/storage/infrastructure failure, `8` waited run requires operator action. Freeze and regression-test this mapping alongside existing application conventions before release. Error text includes a stable code and correlation ID, with private evidence available only through authorized export.

## 5. Operator safeguards

Default attach is Observe when no mode is supplied. Taking control requires an explicit mode and shows current owner/turn/checkpoint. Do not hide a stale approval behind a generic yes/no prompt. Non-interactive invocations with missing trust, account or approval information fail with actionable diagnostics; no unattended prompt hangs. Cancellation and retry never imply discard or force push.

## 6. Build instructions

Add parsing/dispatch fixtures for every command and error path. Implement an injectable workflow client so CLI tests use a deterministic fake service. Add JSON golden tests and bounded output rendering. Reuse existing format/help/diagnostics styles; add generated completions/man pages through current tooling. Add source examples to ordinary nextest tests and canonical user docs.

## 7. Dependencies and integration

C18 is the only workflow execution authority. Original-terminal attachment delegates to existing hardline/Capsule pathways with C07 mode/lease context. No scheduler, Git publisher or Pkl evaluator is independently reimplemented in CLI handlers.

## 8. Acceptance tests

`CLI-01`: all target commands parse and route correctly. `CLI-02`: JSON and exit semantics distinguish accepted request from finished run. `CLI-03`: default observe does not take input. `CLI-04`: non-interactive missing decisions fail, never prompt. `CLI-05`: cancelled client wait does not cancel run. `CLI-06`: manual pre-existing CLI commands retain baseline behavior.

## Evidence and references

- **J06 — [Programmatic launch options](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-launch-load-options/src/load_options.rs).** Lines 1–300. Non-interactive launch, programmatic constructor, identity sink, inputs and validation error vocabulary. Deeper launch call paths were not executed.
- **J19 — [Testing and CI rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/TESTING.md).** Lines 1–200. Pinned mise/mbx/nextest commands, e2e and conditional OrbStack lane, documentation examples and crate gates. Examples here are not evidence of execution.
- **J20 — [Current CLI root](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/apps/jackin/src/cli.rs).** Lines 1–230, including full top-level Command enum. Workflow namespace absent. CLI parsing separated from handlers.
