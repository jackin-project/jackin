# Jackin Build Split

- Status: IN PROGRESS
- Record date: 2026-10-05
- Scope: Initial evidence record.

## Starting state

- Host: `bastion`, Debian 13.7, x86_64, kernel 6.12.94.
- Repository: `https://github.com/jackin-project/jackin.git`.
- Base: `main` at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`.
- Task worktree: `/root/Projects/tailrocks/jackin-project/jackin-refactor-build-split`.
- Task branch: `refactor/build-split`.
- The task worktree was clean at the base SHA.
- The original `main` worktree had one unrelated `mise.lock` modification.
- The original lock hash was `6be630be77daa3073ec74340cd6a113b969ee9117a69b8d48ebe5d5501df764c`.
- The task worktree lock hash was `ac3b0998f110538c5bb34f2653afaf7f2713d7f9f026dde1bb3f50845cef06ef`.
- The task worktree lock blob was `c78d5f9ff2b48c70d488fa9a4cfbf5f750ef11ce`.
- Documentation work preserves both lock states. No lock edit is in scope.

## Requirement trace

The IDs below map each assigned outcome to its scenario, interface, gate, owner, dependencies, and evidence.

| ID | Required outcome, scenario, and interface | Status | Owner | Dependencies | Gate | Evidence |
|---|---|---|---|---|---|---|
| DOC-01 | Create eight task records in `plans/jackin-build-split/`. | PASS | `task_records` | Assigned worktree | All eight files exist and link. | [Records](final-report.md) |
| STYLE-01 | Apply ASD-STE100 to descriptions and procedural sentences. | PASS | `task_records` | None | Descriptions ≤25 words; imperatives ≤20 words. | All eight Markdown records. |
| FACT-01 | Record evidence and mark unknown outcomes with reasons. | IN PROGRESS | `coordinator` and work owners | Static and execution reviews | No unverified result is complete. | Records linked below. |
| MODEL-01 | Check coordinator model prerequisite for section 2.1. | FAIL | `coordinator` | Active root process settings | Required model is Luna/max. | [Codex review](reviews.md) |
| MODEL-02 | Confirm work-agent runtime settings. | IN PROGRESS | `codex_schema_runtime` | Runtime evidence | Confirm every active agent. | [Catalog result](#codex-catalog-command); [Codex review](reviews.md) |
| HOST-01 | Record Debian host inventory and repository identity. | PASS | `task_records` | Initial memory sample is historical. | Keep timestamped command evidence. | [Host results](debian-results.md); [host command](#host-inventory-command) |
| REPO-01 | Record base SHA, task worktree, and branch. | PASS | `task_records` | Clean task worktree | Match initial `main` SHA. | Starting state above; [branches](branches.md) |
| LOCK-01 | Record the initial dirty lock hash and preserve it. | PASS | `task_records` | Original and task worktrees | Hashes remain unchanged. | Starting state above; [branches](branches.md) |
| BRANCH-01 | Inventory branch and PR scenario; record branch interface and owners. | IN PROGRESS | `branches` | Fetched refs and PR review | Ref fetch passed; dispositions remain pending. | [Branch record](branches.md) |
| CRATE-01 | Inspect crate boundaries and build metadata statically. | IN PROGRESS | `crate_design` | Build evidence | Approve no split from static facts alone. | [Crate plan](crate-plan.md) |
| BUILD-01 | Measure baseline before extraction and compare after extraction. | NOT RUN | `build_baseline` | Security gate and implementation | Record both builds and MBX provenance. | [Build results](build-results.md) |
| CI-01 | Inspect generator and CI scenario; preserve required-job interface. | IN PROGRESS | `velnor_recon` and `jackin_generator_config` | Current generator refs | Complete coverage review. | [CI coverage](ci-coverage.md) |
| CI-02 | Decide if the generated workflow meets required coverage. | NOT APPROVED | `preflight_security_review` | Missing coverage findings | Close gaps before approval. | [Workflow review](reviews.md#ci-workflow-review) |
| ARCH-01 | Review Architect manifest at the exact PR head. | PASS | `architect_contract` | Head `0592d0deeaeaa5b785fa67a43d23d3b627552720` | Static review only. | [Architect review](reviews.md#architect-integration-review) |
| ARCH-02 | Run role parser and repository validation. | NOT RUN | `architect_contract` | Reviewed MBX and local role checkout | Record validator output. | [Architect review](reviews.md#architect-integration-review) |
| ARCH-03 | Run the actual role through Jackin. | NOT RUN | `architect_contract` | Security approval and Jackin runtime | Record the live role response. | [Architect review](reviews.md#architect-integration-review) |
| ACCOUNT-01 | Trace Codex account discovery through workspace launch. | NOT RUN | `debian_codex_route` | Jackin binary, config, security gate | Complete registration and launch route. | [Debian results](debian-results.md) |
| SEC-01 | Review execution boundary, container, auth, and cache provenance. | IN PROGRESS | `preflight_security_review` and `unprivileged_exec_design` | Exact execution design | Resolve every preliminary gate. | [Security review](reviews.md) |
| CODEX-01 | Record official configuration schema and local model catalog. | IN PROGRESS | `codex_schema_runtime` | Active-agent runtime confirmation | Confirm settings at runtime. | [Official reference](https://developers.openai.com/codex/config-reference); [catalog command](#codex-catalog-command) |
| MBX-01 | Verify official MBX artifact provenance and activation. | IN PROGRESS | `unprivileged_exec_design` | Security review | Verify before compilation. | [Security review](reviews.md#mbx-provenance) |
| PRIV-01 | Avoid credential or auth-content disclosure. | PASS | `task_records` | None | Do not read or copy auth contents. | [Debian results](debian-results.md) |
| SOURCE-01 | Leave source and `mise.lock` unchanged. | PASS | `task_records` | Documentation-only edits | Commit contains only task records. | Commit `34c32ca31e58b5e3dac71e88892778568f6f70d1` |
| BUILD-02 | Keep builds and compilation unrun in this checkpoint. | NOT RUN | `build_baseline` | Later execution authorization | No build commands. | [Build results](build-results.md) |
| TEST-01 | Keep tests unrun in this checkpoint. | NOT RUN | `build_baseline` | Later execution authorization | No test commands. | [Build results](build-results.md) |
| LIVE-01 | Keep provider, account, and role requests unrun. | NOT RUN | `debian_codex_route` | Binary, config, security review | No live runtime requests. | [Debian results](debian-results.md) |
| REVIEW-01 | Complete implementation acceptance review. | NOT RUN | `execution_crosscheck` | Proposed implementation | Review an exact implementation SHA. | No implementation change exists. |
| MD-01 | Check every local link, path, and Markdown structure. | PASS | `task_records` | Eight task records | Resolve paths and validate tables and ticks. | [Records](final-report.md) |
| GIT-01 | Commit and push documentation progress. | PASS | `task_records` | Task branch | Push without opening a PR. | Commit and branch in [final report](final-report.md) |
| PR-01 | Do not open or merge a PR in this checkpoint. | PASS | `task_records` | Documentation-only scope | No task PR exists. | [Branch record](branches.md) |

## Active owners

| Owner | Area | State |
|---|---|---|
| `branches` | Branch and PR inventory | Inventory complete; full diff review pending |
| `build_baseline` | Build measurements | IN PROGRESS; measurements not run |
| `crate_design` | Crate and dependency analysis | IN PROGRESS |
| `architect_contract` | Architect manifest contract | IN PROGRESS |
| `velnor_recon` / `jackin_generator_config` | Generator and CI coverage | IN PROGRESS |
| `codex_schema_runtime` | Codex schema and runtime settings | IN PROGRESS |
| `unprivileged_exec_design` | Execution boundary | IN PROGRESS |
| `preflight_security_review` | Preliminary security review | Initial review complete; follow-up pending |
| `debian_codex_route` | Debian account route | Static review complete; runtime route NOT RUN |

## Records

- [Branch and PR findings](branches.md)
- [Crate and build plan](crate-plan.md)
- [Build results](build-results.md)
- [CI coverage](ci-coverage.md)
- [Debian results](debian-results.md)
- [Security and Codex review](reviews.md)
- [Initial report](final-report.md)

## Host inventory command

Captured at `2026-10-05T02:37:32+02:00` in the task worktree. The commands were read-only.

```sh
hostname -s
cat /etc/os-release
uname -sr
id -u
nproc
free -h
df -h .
```

The output reported `bastion`, Debian `13.7`, Linux `6.12.94+deb13-amd64`, UID `0`, and `96` logical processors. It reported `125Gi` total RAM and `113Gi` available RAM. It reported `3.5T` total and available disk space.

## Codex catalog command

Captured at `2026-10-05T02:38:11+02:00`. The filter printed model slugs and supported reasoning levels only.

```sh
codex --version
codex debug models | python3 -c 'import json,sys; data=json.load(sys.stdin); print("\n".join(model["slug"] + "\t" + ",".join(level["effort"] for level in model["supported_reasoning_levels"]) for model in data["models"] if model["slug"] in {"gpt-6-luna", "gpt-6.1-sol"}))'
```

Output: `codex-cli 0.160.0`; Luna supports low, medium, high, xhigh, and max. Sol also supports ultra. This catalog does not confirm active agent settings.
