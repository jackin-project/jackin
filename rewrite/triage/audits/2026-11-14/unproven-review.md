# UNPROVEN CONTAINED candidates — per-branch proof (jackin-project/jackin)

> **Committed as the capture-time record (2026-11-14).** The published, operator-facing document is
> `rewrite/triage/debris-ratification-2026-11-14.md`; where the two differ, **the publication wins**.
> The four `CONTAINED-safe-to-delete` verdicts below are **SUPERSEDED**: the merged inventory
> (`rewrite/triage/branch-inventory-2026-11-12.md`) classes these branches as **"CONTAINED (candidate) -
> UNPROVEN, KEPT"** and its class rule says such a branch "must be treated as VALUABLE (NOT deletable)
> until every absent path is proven to be scratch/archive". The publication's §3 records the proof that
> is still missing: the five usage fixtures these branches carry are **absent from `main`** and the
> product-code paths have no inventory-grade (merged-PR / patch-id / ancestry) containment proof.
> **Do not delete these branches.** The per-path proof below is kept as the record of the review.

Repo: https://github.com/jackin-project/jackin (https clone into `/tmp/triage/repo`, read-only inspection).
Main tip inspected: `origin/main` = 7a53f6abe "fix: land the accepted review findings from #1264 (#1275)" (2026-11-13).
Key structural fact used by every proof: main received the OpenShell rewrite as ONE squash,
`f683619b5` "feat!: rebuild jackin on NVIDIA OpenShell (#1264)" (2026-11-12, 4861 files changed), plus
follow-ups 97b1b58f7 / d19cce66f / 7a53f6abe. All 5 branches share merge-base 470fc5d6f (2026-10-29) and are
pre-squash checkpoints of that rewrite program. Therefore `git patch-id` cannot be used as containment proof
here (measured: only 2 of 1031 unique branch patch-ids match any main commit — the squash re-contextualised every
patch). Proof is instead per-path: (1) never-in-main history, (2) blob uniqueness, (3) functional equivalence in
main today, (4) branch wiring (the path was live, compiled code in the branch, not an orphan).

Absent-path set was re-derived independently and matches the prior audit exactly:
"branch-added (not in merge-base) AND not in main" = 25 / 25 / 11 / 6 / 20 for branches 1..5; of those the
product-code subset is 12 / 12 / 6 / 1 / 20 (the other entries are 5 usage JSON fixtures and 8 rewrite docs).

---

## Branch 1 — `codex/openshell-integration-checkpoint-20261108-122311` (tip 6965f5d1c, 2026-11-08)
## Branch 2 — `codex/openshell-verified-checkpoint-20261108` (tip ce119545c, 2026-11-08)
(Branches 1 and 2 carry **byte-identical** blobs for all 12 paths — same blob SHAs in both trees — so the two
proof tables below are the same evidence; branch 2 adds no unique residue for these paths.)

| branch | absent path | main's disposition | proof | final class | recommended action |
|---|---|---|---|---|---|
| 1, 2 | `crates/jackin-session/src/agent_metadata.rs` (blob 1e69d25d, 8451 B) | (b) superseded by later main rewrite | Never in main: `git log --all -- <path>` = only 010238a10 / fa8d64133 (branch commits); blob unique (`git log --all --find-object=1e69d25d` → no main commit). Live in branch: `crates/jackin-session/src/lib.rs:5,40` declares `mod agent_metadata; pub use agent_metadata::*`. Main has **no** agent-metadata module: `git grep -il 'AgentMetadata\|agent_metadata' origin/main -- crates` = 0 hits; `VerifiedAgentMetadata`/`AgentMetadataIdentity`/`AgentMetadataError`/`AGENT_METADATA_LIMIT_BYTES` = 0 hits. Main's shipped identity verification is label + agents.json based: `crates/jackin-session/src/launch/decide.rs:190-202` compares the sandbox agents.json to the plan under `AGENTS_LABEL` and raises `LaunchError::StaleSandbox`; covered by `crates/jackin-session/tests/launch/instance_identity.rs`. The branch module is the earlier file+hash protocol; the shipped rewrite replaced it (content rewritten, not lost). | CONTAINED-safe-to-delete | Delete branch; no port. If the file-hash protocol is ever wanted it must be re-derived against main's label/agents.json identity model — it cannot be cherry-picked (it depends on branch-only APIs). |
| 1, 2 | `crates/jackin-session/src/secret_broker.rs` (blob efbad388, 3255 B) | (a) already in main under different path/name | Never in main (`git log --all -- <path>` → 010238a10 only; `find-object=efbad388` → branch only). Live in branch (`lib.rs:29` `mod secret_broker;`). Main has no `secret_broker` module but has both capabilities elsewhere: Muse keychain read at `crates/jackin-session/src/accounts.rs:1174-1204` (`find-generic-password` + `-s ai.meta.dev.credentials` + `parse_muse_keychain_secret`), and 1Password `op read` at `crates/jackin-session/src/op.rs` (`OP_TIMEOUT = 30 s` = branch `BROKER_TIMEOUT`; `Program::Op`; `run_secret` + zeroize on the reply — same 30 s/secret-wiping semantics as the branch file). | CONTAINED-safe-to-delete | Delete branch; no port (main's `accounts.rs`/`op.rs` are the richer form). |
| 1, 2 | `crates/jackin-session/tests/agent_metadata.rs` (blob 9e99de1c, 11291 B) | (b) superseded with its subject module | Never in main (`git log --all -- <path>` → branch commits only). It is the integration test of `agent_metadata.rs`; main's equivalent subject is `launch/decide.rs` + `tests/launch/instance_identity.rs` (identity/stale-sandbox detection). | CONTAINED-safe-to-delete | Delete branch; no port. |
| 1, 2 | `crates/jackin-testkit/src/docker.rs` (blob d3f6afe1, 36724 B) | (b) superseded by later main rewrite (Docker harness → OpenShell engine) | Never in main (`git log --all -- <path>` → 010238a10 only). Live in branch (`crates/jackin-testkit/src/lib.rs:7,25` `mod docker; pub use docker::*`). Main's testkit `lib.rs` has **no** `mod docker` (modules: connector, console, console_fixture, doctor_host, **engine**, exec, fixtures, gateway, git, host, pty, rig, snapshot, supervisor, sync, usage, world) — the Docker rig was replaced by `crates/jackin-testkit/src/engine.rs` + `gateway/*` + `tests/gateway/*`. | CONTAINED-safe-to-delete | Delete branch; no port. |
| 1, 2 | `crates/jackin-testkit/src/docker/stdin.rs` (blob 043c7f93, 6682 B) | (b) superseded (child of the removed Docker harness) | Never in main; only reachable through `docker.rs` (`mod stdin` inside the removed module). Main has no Docker stdin driver; the OpenShell path uses `crates/jackin-openshell/src/grpc/main_pty.rs` / `grpc/shell.rs`. | CONTAINED-safe-to-delete | Delete branch; no port. |
| 1, 2 | `crates/jackin-testkit/src/host/lock_signal_fs.rs` (blob 30dc6553, 3792 B) | (b) superseded by a later main rewrite of the same test | Never in main (`git log --all -- <path>` → 010238a10 only). Sole consumer in the branch: `crates/jackin-session/tests/launch/image_handoff.rs:12,22-23` (Docker build + a `LockSignalFs` decorator that signals lock contention over mpsc so the test can prove "the rival waited on the held image lock"). Main keeps the same test at the same path but rewrote it: `crates/jackin-session/tests/launch/image_handoff.rs` is 3103 B in main vs 10364 B in the branch and no longer imports `LockSignalFs` (`use jackin_testkit::{BuildEffect, EngineMoment, Fault, Trigger}`; `tokio::sync::{mpsc, oneshot}` + `Trigger` instead). Main's `crates/jackin-testkit/src/host/fs.rs:373` still raises `FsError::LockHeld` and `tests/host.rs:102` asserts it, so lock contention itself is covered. | CONTAINED-safe-to-delete | Delete branch; no port (the helper is a test-only signal channel, ~120 LOC, superseded by `Trigger`). |
| 1, 2 | `crates/jackin-image/images/construct/write-file.c` (blob e1281b06, 13680 B) | (b) superseded by later main rewrite (static writer → OpenShell `write_file` RPC) | Never in main: `git log --all -- <path>` → 010238a10 / 16de1eedf only; `find-object=e1281b06` → branch only. Main's `crates/jackin-image/images/construct/Dockerfile` has a **single stage** (no `write-file-build`, no `cc`, no `/usr/local/lib/jackin/write-file`); the branch Dockerfile has the 2-stage `write-file-build` + `COPY write-file.c`. Main writes sandbox files through the OpenShell RPC instead: `crates/jackin-openshell/src/grpc/program.rs:192 write_file`, `src/gateway.rs:56`, contract `write_file_mode_and_atomicity` (`src/contract.rs:678`), tests `crates/jackin-openshell/tests/gateway/exec.rs:179-341` (mode/atomicity/bytes/limit). The C helper is not referenced by any main file (`git grep write-file origin/main -- crates` → only the RPC `write_file`). | CONTAINED-safe-to-delete | Delete branch; no port (the RPC supersedes the privileged static writer). |
| 1, 2 | `crates/jackin-openshell/tests/gateway/cancellation.rs` (blob 663c4751, 7467 B) | (a)/(b) covered in main under different test files | Never in main (`git log --all -- <path>` → 010238a10, 16de1eedf). Live in branch: `crates/jackin-openshell/tests/gateway/main.rs:13` `mod cancellation;`. Main's gateway test module list has **no** `cancellation` (attach, attach_rpc, attach_ssh, connect, contract, discovery, download_bound, ended, error_text, exec, exec_rpc, fakes, harness, pki, provider_rpc, providers, sandbox_rpc, sandboxes, server, service, ssh_server, status, support, transfer) but main keeps cancellation coverage in the matching places: `tests/gateway/connect.rs:335,354`, `tests/gateway/exec.rs:166`, `tests/gateway/download_bound.rs:156`, `tests/gateway/status.rs`, `tests/gateway/error_text.rs`, `tests/gateway/transfer.rs` all assert `GatewayError::Cancelled` after `cancel.cancel()`. | CONTAINED-safe-to-delete | Delete branch; no port (invariant covered; a port would only duplicate). |
| 1, 2 | `crates/jackin-openshell/tests/gateway/writer_bootstrap.rs` (blob 25338732, 14677 B) | (b) superseded — its subject (the privileged static writer) no longer exists in main | Never in main; live in branch (`main.rs:35` `mod writer_bootstrap;`). The test compiles `crates/jackin-image/images/construct/write-file.c` + the shim/driver C to prove the root-side writer can bootstrap `.codex/auth.json` before the agent user runs. Both the helper (`write-file.c`) and the bootstrap mechanism are absent from main; main's auth/agent-file delivery is the OpenShell `write_file` RPC, proven by `crates/jackin-openshell/tests/gateway/exec.rs:179` and contract `write_file_mode_and_atomicity`. | CONTAINED-safe-to-delete | Delete branch; no port (the security property is now carried by the RPC path + its contract tests). |
| 1, 2 | `crates/jackin-openshell/tests/gateway/writer_bootstrap_driver.c` (blob abf98709, 12296 B) | (b) superseded (C harness for a removed mechanism) | Never in main; byte-identical in branches 1, 2 and 3 (`find-object=abf98709` → only branch commits). Only purpose is to be `cc`-compiled by `writer_bootstrap.rs` against `write-file.c`, which main does not ship. | CONTAINED-safe-to-delete | Delete branch; no port. |
| 1, 2 | `crates/jackin-openshell/tests/gateway/writer_bootstrap_shim.c` (blob d920e27c, 4875 B) | (b) superseded (C harness for a removed mechanism) | Never in main; byte-identical in branches 1, 2 and 3. Only purpose is the LD_PRELOAD-style shim for `writer_bootstrap.rs`. | CONTAINED-safe-to-delete | Delete branch; no port. |
| 1, 2 | `crates/jackin-tui/tests/console/files.rs` (blob 46e156a7, 1360 B) | (a)/(b) invariant covered in main under a different test file | Never in main (`git log --all -- <path>` → 010238a10, 16de1eedf, 78d8b5f02). Live in branch (`crates/jackin-tui/tests/console/main.rs:9` `mod files;`). Main's console test list has **no** `files` module, but the same invariant (a pipe or a link to a pipe is not offered/accepted as a credential file) is covered in main at `crates/jackin-tui/tests/console/sources.rs:563-600` ("an_oversized_file_or_a_link_to_a_pipe_fails_the_check_and_the_save" — mkfifo + symlink-to-pipe, config must stay unchanged) and the Claude twin at `sources.rs:596+` ("an_oversized_claude_login_file_or_a_link_to_a_pipe_is_refused_and_the_config_stays"). Main's file-browser source code also filters non-regular entries (`crates/jackin-tui/src/...` file picker; `crates/jackin-core/src/fs.rs` `FileKind`). | CONTAINED-safe-to-delete | Delete branch; no port. |

**Branch 1 final class: CONTAINED-safe-to-delete. Branch 2 final class: CONTAINED-safe-to-delete**
(each of the 12 absent product-code paths is (a) present in main under a different path/name or (b) superseded by
the later main rewrite; none is genuinely unique work that must be preserved. Branch 2 adds nothing beyond branch 1
for these paths — byte-identical blobs.)

---

## Branch 3 — `preserve/session-boundaries-20261109` (tip 16de1eedf, 2026-11-06)

Product-code absent subset = 6 of 11 (the other 5 are usage JSON fixtures, see Limitations).

| branch | absent path | main's disposition | proof | final class | recommended action |
|---|---|---|---|---|---|
| 3 | `crates/jackin-image/images/construct/write-file.c` (blob dafb9560, 13658 B — *earlier* variant than branch 1/2's e1281b06) | (b) superseded by later main rewrite | Never in main; `find-object=dafb9560` → only 16de1eedf. Same proof as above: main's construct Dockerfile has no write-file stage; the file is delivered by the OpenShell `write_file` RPC (`crates/jackin-openshell/src/grpc/program.rs:192`). Branch 3 holds an *older* revision of the same helper → strictly less content than branches 1/2, which are themselves superseded. | CONTAINED-safe-to-delete | Delete branch; no port. |
| 3 | `crates/jackin-openshell/tests/gateway/cancellation.rs` (blob 663c4751 — byte-identical to branches 1/2) | (a)/(b) covered in main under different test files | Identical blob to branch 1/2 → same evidence (main keeps `GatewayError::Cancelled` coverage in `tests/gateway/connect.rs`, `exec.rs`, `download_bound.rs`, `status.rs`, `error_text.rs`, `transfer.rs`; no `cancellation` module). | CONTAINED-safe-to-delete | Delete branch; no port. |
| 3 | `crates/jackin-openshell/tests/gateway/writer_bootstrap.rs` (blob 3b100e3c, 10088 B — *earlier, smaller* variant of branch 1/2's 25338732) | (b) superseded — subject mechanism removed from main | Never in main; `find-object=3b100e3c` → only 16de1eedf. Same subject removal as above; the branch-3 revision is the earliest of the three, so it carries strictly less than branches 1/2 (which are superseded anyway). | CONTAINED-safe-to-delete | Delete branch; no port. |
| 3 | `crates/jackin-openshell/tests/gateway/writer_bootstrap_driver.c` (blob abf98709 — identical to branches 1/2) | (b) superseded (C harness for a removed mechanism) | Never in main; `find-object` → branch commits only; consumed only by `writer_bootstrap.rs`. | CONTAINED-safe-to-delete | Delete branch; no port. |
| 3 | `crates/jackin-openshell/tests/gateway/writer_bootstrap_shim.c` (blob d920e27c — identical to branches 1/2) | (b) superseded (C harness for a removed mechanism) | Never in main; `find-object` → branch commits only; consumed only by `writer_bootstrap.rs`. | CONTAINED-safe-to-delete | Delete branch; no port. |
| 3 | `crates/jackin-tui/tests/console/files.rs` (blob 46e156a7 — byte-identical to branches 1/2 and 4) | (a)/(b) invariant covered in main under a different test file | Identical blob → same evidence (main `crates/jackin-tui/tests/console/sources.rs:563-600` + the Claude twin). | CONTAINED-safe-to-delete | Delete branch; no port. |

**Branch 3 final class: CONTAINED-safe-to-delete.** (6/6 product-code paths are (a)/(b); every branch-3 blob is
byte-identical to, or an earlier revision of, a branch-1/2 blob that is itself superseded.)

---

## Branch 4 — `preserve/w1-fixes-20261109` (tip 78d8b5f02, 2026-11-05)

| branch | absent path | main's disposition | proof | final class | recommended action |
|---|---|---|---|---|---|
| 4 | `crates/jackin-tui/tests/console/files.rs` (blob 46e156a7 — byte-identical to branches 1/2/3) | (a)/(b) invariant covered in main under a different test file | Never in main: `git log --all -- <path>` → 010238a10, 16de1eedf, **78d8b5f02** (this branch's own commit — it is the branch that first added the file). Live in branch (`tests/console/main.rs:9` `mod files;`). Main has no `files` console test, but covers the same pipe/link invariant at `crates/jackin-tui/tests/console/sources.rs:563-600` and `:596+`. | CONTAINED-safe-to-delete | Delete branch; no port. |

**Branch 4 final class: CONTAINED-safe-to-delete.** (1/1 product-code path is (a)/(b); the other 5 absent paths are
usage JSON fixtures, see Limitations.)

---

## Branch 5 — `preserve/p0.6-prototype-20261109` (tip 63b2976a5, 2026-11-02) — 20/20 paths live prototype code

| branch | absent path | main's disposition | proof | final class | recommended action |
|---|---|---|---|---|---|
| 5 | `prototypes/p0.6/src/panes/app.rs`, `src/panes/keys.rs`, `src/panes/main.rs`, `src/panes/term.rs`, `src/panes/transport.rs`, `src/main.rs` (6 files) | (b) superseded — the prototype's *result* is in main; the prototype source was excluded by design | The branch adds 36 paths; 16 are present in main (identical paths) and 20 are the prototype. `rewrite/evidence/P0.6-panes.md` is **byte-identical** in the branch and in main (both blob `7daaf80a71e82660b4819f181b657c8c9494c0ba`) — the prototype's findings are in main. Main's `rewrite/PLAN.md:207-216` states the scope: "**Scope:** `rewrite/evidence/P0.6-panes.md`, a prototype on a local worktree branch **that you do not merge**", and "[x] P0.6 done — 8e619ca9, evidence/P0.6-panes.md (Codex pane: CreateSshSession + ForwardTcp + russh openshell-main; shell pane: ExecSandboxInteractive; ratatui prototype 47 checks PASS on local branch wt/P0.6)". Main's `rewrite/API.md:1553` and `rewrite/DESIGN.md:448` carry the chosen transports forward, and the shipped code implements them (`crates/jackin-openshell/src/grpc/main_pty.rs`, `src/grpc/shell.rs`). | DEBRIS | Delete branch. The scratch ratatui app is a by-design non-mergeable prototype; its evidence and its decisions are already in main. |
| 5 | `prototypes/p0.6/Cargo.toml`, `Cargo.lock`, `.gitignore`, `drive.py`, `main-script.sh`, `main-tui.sh`, `run-all.sh`, `run-panes.sh` (8 files) | (b) superseded — build/run scaffolding for a by-design non-merged prototype | Same PLAN.md scope rule ("a prototype on a local worktree branch that you do not merge"); `main` has no `prototypes/` directory at all (`git ls-tree -r --name-only origin/main | grep '^prototypes/'` → 0). | DEBRIS | Delete branch. |
| 5 | `prototypes/p0.6/captures/app-a.log`, `app-b.log`, `app-c.log`, `run-a.txt`, `run-b.txt`, `run-c.txt` (6 files) | (b) superseded — raw prototype captures; the distilled proof is in main | The captures are the prototype's own run logs; the distilled evidence (`rewrite/evidence/P0.6-panes.md`, byte-identical to main) is what the program keeps (main `rewrite/API.md:4227,4257` cites `P0.6-panes.md` as the live source of the pane rules). | DEBRIS | Delete branch. |

**Branch 5 final class: DEBRIS** (not CONTAINED: main has no `prototypes/` tree, so the paths are absent — but they
are a throwaway prototype explicitly scoped out of the merge, whose result is in main byte-for-byte. Not VALUABLE:
nothing in main depends on it and main's own PLAN forbids merging it.)

---

## Summary

| branch | absent product-code paths | disposition split | final class |
|---|---|---|---|
| `codex/openshell-integration-checkpoint-20261108-122311` | 12 | (a) 5 · (b) 7 · (c) 0 | **CONTAINED-safe-to-delete** |
| `codex/openshell-verified-checkpoint-20261108` | 12 | (a) 5 · (b) 7 · (c) 0 | **CONTAINED-safe-to-delete** |
| `preserve/session-boundaries-20261109` | 6 | (a) 3 · (b) 3 · (c) 0 | **CONTAINED-safe-to-delete** |
| `preserve/w1-fixes-20261109` | 1 | (a) 1 · (b) 0 · (c) 0 | **CONTAINED-safe-to-delete** |
| `preserve/p0.6-prototype-20261109` | 20 | (a) 0 · (b) 20 · (c) 0 | **DEBRIS** |

Nothing was deleted, merged or pushed. Only a read-only clone was made.

## Limitations / open items (not part of the assigned absent-product-code scope, flagged for the coordinator)

1. **5 usage fixtures are branch-added and absent from main** (not counted as product code by the prior audit):
   `crates/usage/jackin-usage/tests/usage/fixtures/kimi/usages_hours.json`, `kimi/usages_no_window.json`,
   `opencode/auth.json`, `opencode/auth_go_oauth.json`, `opencode/auth_no_go.json`
   (present in branches 1, 2, 3, 4 — absent from main in all of them).
   These fixtures encode a **different usage-API shape** than main ships: the branch's rewrite-lineage provider
   (`crates/usage/jackin-usage-provider-kimi/src/lib.rs` @ branch tip, lines 25/83/112-117) parses Kimi's
   `limits[].window.timeUnit` (`TIME_UNIT_SECOND|MINUTE|HOUR|DAY|WEEK`) and the branch's opencode provider expects
   an `opencode-go` entry (`{"type":"api","key":…}`), while main's shipped providers parse Kimi's older
   `usages.limit_5h/limit_7d/limit_month_total/limit_month_code` map (`crates/usage/jackin-usage-provider-kimi/src/lib.rs:28-39`)
   and OpenCode's `access`/`expires`/`metadata.orgID` (`crates/usage/jackin-usage-provider-opencode/src/lib.rs:34-52`).
   Main has **no** `limits`/`timeUnit` handling anywhere in `crates/usage/**`.
   → Disposition for the fixtures themselves: **DEBRIS relative to main** (unusable without the branch's provider
   code, which is present at the same path in main but with different content). **Possible VALUABLE port** if the
   newer Kimi/OpenCode API shape is the wanted one: it is small — 2 provider files (~120 LOC changed), 5 fixture
   files, and ~5 test functions in `crates/usage/jackin-usage/tests/usage/providers/{kimi,opencode}.rs`
   (branch: `kimi.rs` uses `usages_hours.json`/`usages_no_window.json`; `opencode.rs` uses `auth.json`/
   `auth_go_oauth.json`/`auth_no_go.json`); main's current `kimi.rs`/`opencode.rs` have no equivalent cases.
   **This is the only place in the 5 branches where I found content that main does not have anywhere.** Decision
   needed from the coordinator: is the Kimi `limits`/`timeUnit` schema live? If yes → port (VALUABLE); if no →
   fixtures are DEBRIS.
2. **8 branch-only rewrite docs** are also absent from main (non-product-code): `rewrite/blockers/T03-grok.md`,
   `T10-omp-kimi.md`, `T11-omp-minimax.md`, `T12-omp-mimo.md` and `rewrite/evidence/2026-11-07-s{4,6}-*.txt`
   (branches 1, 2). Proof of disposition: those four blockers are **not** in main's `rewrite/blockers/` (main has
   only P0.5, P2.4, T02, T04-T08, T22) because the lanes were closed — main carries the closing evidence
   (`rewrite/evidence/A14-grok-final-live.md`, `A14-omp-final-live.md`, `A14-kimi-lanes.md`,
   `A14-mm-mimo-lanes.md`). The `2026-11-07-*` raw dumps are superseded by main's evidence documents.
3. **Scope**: this review covers only paths absent from main. Paths that exist in main at the same path but with
   different content (e.g. the two Kimi/OpenCode providers above, `crates/jackin-session/src/credentials.rs`,
   `crates/jackin-tui/tests/console/sources.rs`) were not diffed exhaustively — only where needed to judge an
   absent path's disposition.
4. **Patch-id containment is unusable for this repo** (squash-merge); all proofs above are path/blob/functional,
   and each absent path was checked with `git log --all -- <path>`, `git log --diff-filter=D -- <path>` (no deleting
   commit exists: the paths were never in main), `git log --all --find-object=<blob>`, main-tree lookups and
   `git grep` in main.
