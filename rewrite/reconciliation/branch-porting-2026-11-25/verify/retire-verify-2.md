# Retire batch 2: verification of RETIRE verdicts

Read-only check of the 65 branches in `retire-batch-2.tsv` against `origin/main` at `0130af3d` (fetched for this run; the plan's snapshot main was `8d9b05927`).

**Counts:** SAFE 63, VALUE 0, UNCERTAIN 2. MOVED: 0 (all 65 expected SHAs match `origin/<branch>`).

## Method

1. SHA check: `git rev-parse origin/<branch>` against the manifest SHA. All 65 match.
2. Unique commits: `git rev-list --no-merges origin/<b> --not origin/main origin/preserve/pr1264-merge`. The `pr1264-merge` line is the #1264 integration history that main squash-merged, so it is used as a second reference. `git cherry` alone misreports this because of the squash. Merge-base diff stats were also computed; for most old-lineage branches they cover ~4,700 files, so the per-commit view was used instead.
3. Each unique commit was checked three ways: (i) patch-id match against main plus pr1264 commits (EQUIV); (ii) for NEW commits, exact added-line presence in current main (code and config paths, rewrite/ excluded); (iii) symbol presence (fn/struct/const/test names) and `git apply --reverse/--check` per file against main's tree.
4. Non-matching changes were classified by hand: superseded by a deliberate later main change, renamed or moved, excluded by plan section 3 (logs, .audit, generated CI, stale pins, removed native hook/capsule/process-directory code, triage/decision trails), or covered by a P-task.

Caveat on EQUIV matches: several patch-equivalent targets sit on the #1264 integration line rather than main's ancestry. For each code-bearing target I checked that its content is present on main, rather than relying on the equivalence alone.

## Results

| branch | expected SHA | verdict | evidence |
|---|---|---|---|
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/3` | `2c88c91d59` | SAFE | Empty checkpoint: tip tree = parent 4691c0089 (on #1264 line; parent content present on main). No unique content. |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/30` | `65f6a40081` | SAFE | Empty checkpoint: tip tree = parent bb46ad26c (on #1264 line; parent content present on main). No unique content. |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/4` | `557429f3be` | SAFE | Empty checkpoint: tip tree = parent 4691c0089 (on #1264 line; parent content present on main). No unique content. |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/5` | `c8ad0d2382` | SAFE | Empty checkpoint: tip tree = parent 4691c0089 (on #1264 line; parent content present on main). No unique content. |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/6` | `8519884113` | SAFE | Empty checkpoint: tip tree = parent 4691c0089 (on #1264 line; parent content present on main). No unique content. |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/7` | `94d8966960` | SAFE | Empty checkpoint: tip tree = parent 4691c0089 (on #1264 line; parent content present on main). No unique content. |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/8` | `674353bbf9` | SAFE | Empty checkpoint: tip tree = parent 4691c0089 (on #1264 line; parent content present on main). No unique content. |
| `preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/9` | `4940d2902c` | SAFE | Empty checkpoint: tip tree = parent 4691c0089 (on #1264 line; parent content present on main). No unique content. |
| `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/0` | `41a002522e` | SAFE | Empty checkpoint: tip tree = parent 573c3a09e (on #1264 line; Git-role observe code present on main). No unique content. |
| `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/1` | `6cd9582027` | SAFE | Empty checkpoint: tip tree = parent 573c3a09e (on #1264 line; Git-role observe code present on main). No unique content. |
| `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/2` | `8bc03fa83b` | SAFE | Empty checkpoint: tip tree = parent 573c3a09e (on #1264 line; Git-role observe code present on main). No unique content. |
| `preserve/t3/YjE5NTdmNGViMzdhNDM3ZTYwZjI1N2UwODdiYzZiMDA/3` | `bc17efa2ba` | SAFE | Empty checkpoint: tip tree = parent 573c3a09e (on #1264 line; Git-role observe code present on main). No unique content. |
| `preserve/w2-impl-strong-e2e-zai` | `b8b0cda499` | SAFE | 546862232 covered: main load.rs:293 has the same T09/T18 test (+1 assert). b8b0cda4 zai fake accounts is the rejected design: main e2e setup.rs is target-aware (CredentialSession/E2eTarget), no zai accounts. |
| `preserve/w6-fix-strong-` | `4aa4a01eab` | SAFE | 4aa4a01 adds .audit logs only (b). Other 21 unique commits touch only rewrite/decisions.tsv and docs/triage trail (b). |
| `preserve/worktree-` | `c09004bba0` | SAFE | c09004bb changes .gitleaksignore 430->413; main already has the 413 entry (a). Other unique commits are rewrite/triage docs and trail (b). |
| `preserve/wt-` | `3f3f6b826c` | SAFE | 3f3f6b82 is the same .gitleaksignore 430->413 change, already on main (a). Other unique commits are rewrite/triage docs and trail (b). |
| `recovery/pr1230-blob-auth-77b399f1-20261027` | `77b399f124` | SAFE | 49 unique commits target the removed native pre-commit owner: crates/tools/jackin-xtask and crates/core/jackin-process-directory are absent on main (0 files); main CONTRIBUTING says pre-commit only checks (b). P04 covers the concept in current code. |
| `recovery/stash-2f8cc42b-20261023` | `e5e6d1fd27` | SAFE | Single snapshot: generated CI workflows, .velnor release manifest and velnor-actions pins (b). No product code. |
| `recovery/stash-2fb075f8-20261023` | `4d9540abf3` | SAFE | Single snapshot: crates/adapters capsule/release code (absent on main) and generated CI, old docs (b). |
| `recovery/stash-88f5643c-20261023` | `ddf8861f45` | SAFE | Single snapshot: generated workflows/actions and velnor-actions-contract pin 0.1.41->0.1.43 (b). |
| `recovery/stash-adoption-6a0d2f03-20261025` | `ade401e79e` | SAFE | Single snapshot: jackin-process-directory secure_install, native hook bootstrap, examples/workflows, jackin-workflow (all absent on main) (b). |
| `recovery/stash-be51ff69-20261023` | `9cdf189b81` | SAFE | 9 unique: 3d057654 already on main (#1171, EQUIV). 84785d605 floor kept: main rustls-webpki 0.103.16 = branch, webpki-root-certs 1.0.11 newer (a). 9cdf189b8 native hook wrappers: hooks-install absent, main hk.pkl already runs betterleaks/gitleaks/kingfisher security groups (b). 518104eef, 5cac46e28, c2fc74fcc: crates absent on main or docs (b). 7d99f6cc7 regenerated CI (b). 7b7b2db16 pins: main pins newer (betterleaks 1.10.0 vs 1.9.0, hk 2.12.0 vs 2.8.0); dropped tools unused on main (a/b). |
| `recovery/stash-d808d094-20261023` | `00f98f3251` | SAFE | Single snapshot: old adapters/release/capsule code (absent on main), CI contract, native hook scripts, pins and docs (b). |
| `recovery/strict-merge-gate-merge-readiness-20261022` | `2f96c6dadd` | SAFE | 7 unique: generated CI gate config and seed/preview lanes (.velnor, mise, ci_contract under crates/tools absent on main). Plan section 3 forbids hand-merging generated gate config (b). |
| `rollout/velnor-3-provider` | `092fe1db79` | UNCERTAIN | No merge base with main; 28-file tree is the jackin-agent-brown role repo (jackin.role.toml, Dockerfile, hooks). Not Jackin product code, so no Jackin value. Owner-repo copy NOT verified from this clone; 223 commits exist only on this ref. |
| `sup019/fix-1264` | `2c54a59747` | SAFE | Shared commits checked: 2c54a597 holds/account_providers present (accounts.rs); f0021a0b OMP revision + reused-inode test present (omp.rs); c1e041f5 gating present (config.rs publishable_tuple); 5bae2c17 Grok scope ambiguity present (credentials.rs only_scope); 84b3afa9 env-default check present (manifest.rs:268); 7af64580 100%. 8e98048352 and 8a5b40e9 sonar CI (b, sonar files blob-identical). d4f718c3 rustfmt-only (b). |
| `sup020/fix-cli-1264` | `aadbd7a40f` | SAFE | Same shared commits as sup019 (covered). CLI commits aadbd7a4, 368b6cda, a8a29de5 present at 100% exact-line presence. |
| `swarm/fanout-call_cd50d53ea77340e08a7d675b-1` | `78855f5ad1` | SAFE | 78855f5a (openshell redaction) present: 100% exact-line presence; parse_authority test input is on main grpc.rs:350. |
| `swarm/fanout-call_e6e20aee930345eb8831374f-0` | `50103fdf09` | SAFE | 14 unique, all shared with sup019/sup020 (covered) or docs/CI (b). 50103fdf 97%, c1e041f5 gating present (config.rs). |
| `swarm/sup009-sonar-source-config` | `f0b2a8a307` | SAFE | f0b2a8a3 sonar scanner config: sonar-project.properties and .sonarcloud.properties blob-identical to main. mise/.velnor/workflow changes are generated or pins (b). |
| `test/provider-eligibility` | `327e45af61` | SAFE | crates/jackin-core/tests/provider_eligibility.rs blob-identical to main. bd0a7d88 96%, 4f858aaa 100%. 1c85b68b regenerated workflows (b). |
| `w3/pr1281-land` | `b2c0da82a8` | SAFE | 31 unique commits touch only rewrite/triage/branch-inventory and rewrite/decisions.tsv (b; plan section 3 excludes triage packs). |
| `w3/pr1281-merge` | `e8d6bcca46` | SAFE | 26 unique commits touch only rewrite/triage and rewrite/decisions.tsv (b). |
| `wt/a11b-account-hardening` | `a935999a42` | SAFE | a935999a: 97% of non-audit code lines present. account_providers(), holds(), forget_locked() and HostPorts::system(home) present in jackin-session accounts.rs and host.rs (a). .audit logs (b). |
| `wt/a14-accounts-integrated` | `ecfe7e9604` | SAFE | EQUIV: patch-equivalent to d9411a2d7 (integration line). Its code is present on main: account_providers/holds in accounts.rs (a). |
| `wt/a14-availability` | `7a20e913e9` | SAFE | 7a20e913 (75% exact-line): unavailable-tuple statuses and tests are superseded by deliberate main promotions (main agent.rs TUPLES marks T01/T02-series Supported; rewrite/evidence/A14-promotion.md on main). EQUIV ebfb26c9->7402852433 (pins) and 8f4a4bfd->1585a65dc (platform host, 16/16 lines on main). |
| `wt/a14-availability-final` | `ebfb26c9c3` | SAFE | 2 EQUIV: 8f4a4bfd->1585a65dc (platform host, 100% on main) and ebfb26c9->7402852433 (interim pins, superseded by main pins). |
| `wt/a14-claude-model` | `4a3d4ee532` | SAFE | 4a3d4ee532: 97% of code lines present. Per-instance model is main's render_claude_config(tuple, model) (jackin-session files.rs:126) (a). |
| `wt/a14-grok-session` | `17aacafcfa` | SAFE | 17aacafc: 98% of non-audit code lines present. Grok scope selection (ACTIVE_SCOPE/INHERITED_SCOPE/only_scope) on main credentials.rs:869 and usage-provider-grok lib.rs. Grok test renamed and promoted on main (agent_table.rs:483) (a). |
| `wt/a14-native-integrated` | `97c7a8ce64` | SAFE | 97c7a8ce: 99% of non-audit code lines present. Missing lines are the pre-promotion Grok test and tuple-state flips superseded on main (a). |
| `wt/a14-oauth-blockers` | `3815766f08` | SAFE | 3 EQUIV, all to docs/blocker records on the integration line (b). No code. |
| `wt/a14-oauth-integrated` | `94d880efef` | SAFE | 94d880ef: 97% present; same superseded pre-promotion test and states as native-integrated (a). |
| `wt/a14-promotion` | `a65c59a3bd` | SAFE | 36ff5aecf 100% present. e2c07b05 96%: the T13 'supervised ... refused api.anthropic.com' reason is a pre-promotion status; main T13 is Supported with no reason (agent.rs). Evidence record on main (A14-promotion.md) (a). |
| `wt/a14-role-review` | `790a9e8910` | SAFE | 4 EQUIV: 1585a65d (100% on main), 7402852433 (interim pins superseded), docs (b). |
| `wt/a14-stable-pins` | `b5d317b080` | SAFE | 3 EQUIV: pins 7402852433/15c8ad7a superseded by main pins; 1585a65d on main (a/b). |
| `wt/a14-subscription-close` | `81cace3ddd` | SAFE | 2 EQUIV: bd577c77 antigravity keyring requirement present on main (100% lines); 12edd9ea live-proof docs (b). |
| `wt/a14-subscription-verify` | `95feab0270` | SAFE | 2 EQUIV to docs/evidence on the integration line (b). No code. |
| `wt/a14-t02-blocker` | `a31a8e7b82` | SAFE | 4 EQUIV: 1585a65d (platform host, 100% on main) and OAuth/T02 docs (b). |
| `wt/a14-t02-live-fixed` | `f41625351c` | SAFE | 3 EQUIV: 1585a65d (100% on main) and live-verification docs (b). |
| `wt/a14-t02-policy` | `8f4a4bfd31` | SAFE | Only unique commit is EQUIV to 1585a65d; main agent spec and YAML carry platform.claude.com with api.anthropic.com (100% lines) (a). |
| `wt/a14-t02-policy-review` | `28f7e0751b` | SAFE | 2 EQUIV: 1585a65d (100% on main) and review docs (b). |
| `wt/a14-t03-promotion` | `fbdb9667be` | SAFE | 72a22cb8 87%: its exact-reason and 22-tuple test is renamed on main (catalog.rs:223 all_22_catalog_rows_match_the_approved_matrix); Grok CLI tests blob-identical (a). fbdb9667 docs (b). 97c7a8ce 99%. |
| `wt/a14-t09-config` | `d6ee3233e0` | SAFE | 78ab4be3 (pins Z.AI anthropic protocol): 5/5 lines present on main. EQUIV 436781af (Z.AI anthropic-messages endpoint, 2/2 lines on main) (a). |
| `wt/a14-t09-live` | `c700d20788` | SAFE | Same commit 78ab4be3 (5/5 on main) and EQUIV 436781af (2/2 on main); live-proof docs (b). |
| `wt/a14-t09-verify` | `f9d6621d24` | SAFE | Same commit 78ab4be3 (5/5 on main) and EQUIV 436781af (2/2 on main); verify docs (b). |
| `wt/a14-t13-auth` | `8c444a97ad` | SAFE | Only unique commit EQUIV to 4a153877 (T13 Kimi auth token). Main uses ANTHROPIC_AUTH_TOKEN for T13 (agent.rs, t13-claude-kimi.yaml). Old render_claude_config(tuple) signature superseded by (tuple, model) (a). |
| `wt/a14-zai-final` | `36537f1dd0` | SAFE | 3 EQUIV: 1585a65d (100% on main), 7402852433 (interim pins superseded), docs b8fd739b (b). |
| `wt/a15-declared-auth-producer` | `dd2008cbf9` | UNCERTAIN | No merge base; 49-file tree is the jackin-sentinel role repo, not Jackin (no crates/). Its own README and probe files are not in this clone, so the owner-repo claim (sentinel main fb17d8b6d) could not be checked here. 189 commits exist only on this ref. |
| `wt/a15-e2e-t01` | `655a00e2f1` | SAFE | 5a0802dd (95%): jackin-start byte-identical to main (empty diff) and undeclared-window judgement present in j17.rs (a). b667957821 (88% code): gpt-5.1-codex allowlist and manifest test superseded by main's explicit rejection (manifest.rs 'rejects_the_removed_t01_model'); validate_tuple_selection present (plan.rs, a). EQUIV dd45b8c1 (81%) and 18578a2b (98%) and 9e231545 (98%) on main. .audit (b). |
| `wt/a15-final-clean` | `8c93c096db` | SAFE | 8c93c096 touches only rewrite/evidence/A15-e2e-review.md, superseded per branch notes (b). b667957821 as wt/a15-e2e-t01 (a, superseded gpt-5.1-codex). .audit (b). |
| `wt/a15-final-review` | `b667957821` | SAFE | b667957821 as wt/a15-e2e-t01 (a, superseded gpt-5.1-codex). .audit (b). |
| `wt/a15-supported-targets` | `c78d1ab385` | SAFE | 41c8c40e 98% (j_supported and target journeys present), 5a0802dd 95% (jackin-start identical), b667957821 as above. EQUIV 9e231545/dd45b8c1/18578a2b (98/81/98%) on main (a). .audit (b). |
| `wt/live-sub` | `961ede2c24` | SAFE | Single EQUIV to 92f1effaf (integration line), evidence document only (b). |
| `wt/live-t02t05` | `a79fa13933` | SAFE | Single EQUIV to 3d1131581, evidence document only (b). |
| `wt/models-research` | `66e47d24c8` | SAFE | Single EQUIV to 06b087a58, vendor research document only (b). |

## Detailed sections for UNCERTAIN branches

### `rollout/velnor-3-provider` (UNCERTAIN)

- Expected SHA `092fe1db7`, matches. Unique commits: 223 (`git log origin/main..origin/rollout/velnor-3-provider` also lists them). There is no merge base with main, so the commits form an unrelated root.
- Changed content: the whole tree is 28 files (`jackin.role.toml`, `Dockerfile`, `hooks/preflight.sh`, `hooks/source.sh`, `config/fleet/velnor-host.env`, `zshrc.d/docker-helpers.zsh`, `scripts/marketplace-audit.sh`, `.github/workflows/*`, `mise.toml`, `renovate.json`). It has no `crates/`. The history is a role repository for `jackin-agent-brown`, not the Jackin application.
- Product value on main: none found. Nothing in this tree is Jackin application code that main lacks.
- Why UNCERTAIN: the verdict depends on the owning role repository holding this history. That repository is not in this clone, and no ref other than this branch contains these commits (`git branch -r --contains 092fe1db7` lists only this ref). Before deleting the ref, confirm the role repository has the 223 commits (or accept losing them).
- Plan wording: "unrelated role-repository history" matches what the tree shows.

### `wt/a15-declared-auth-producer` (UNCERTAIN)

- Expected SHA `dd2008cbf9`, matches. No merge base with main. 189 unique commits, all unrelated to Jackin's crate layout. Only this ref contains them.
- Tree: 49 files, README titled `jackin-sentinel`, probe scripts, `policy.yaml`, `instructions/*.md`, `.github` CI from the sentinel role. There is no `crates/` directory. Commit subjects such as `fix(probe): ...` and `chore(deps): update ... (#1..#157)` belong to the sentinel repository's own history.
- Product value on main: none found. Nothing here is Jackin crate code that main lacks.
- Why UNCERTAIN: the plan says the declared-auth rejection already lives in sentinel `main` at `fb17d8b6d` (probe blob `2ff1f0cb8`). Neither object is in this clone (`git cat-file -t fb17d8b6d` fails), so I could not confirm that claim. Deletion of the ref should wait for that owner-repository check.

## Notes on the remaining 63 SAFE verdicts

- Largest groups: the 12 `preserve/t3/*` empty checkpoints; the `wt/a14-*` and `wt/a15-*` branches, whose code is present on main or superseded; the `sup019`/`sup020`/`swarm` branches, whose shared commits are present on main; and the `recovery/stash-*` snapshots and `strict-merge-gate`, which are generated CI or removed architecture.
- Superseded by a deliberate main change (not a regression): the `Unavailable` tuple flips in `wt/a14-availability*` and the T13 and MiniMax statuses; the `gpt-5.1-codex` T01 model allowlist in `wt/a15-*` (main's `manifest.rs` test rejects it on purpose); the Grok `grok_row_*` test in `wt/a14-*` (main renamed it `..._as_a_supported_tuple`).
- Tool pins in `recovery/stash-be51ff69` are older than main's (betterleaks 1.9.0 vs 1.10.0, hk 2.8.0 vs 2.12.0). Tools dropped from main's `mise.toml` (cargo-audit, cargo-fuzz, cargo-hack, cosign, weaver, boltffi) are not referenced anywhere on main, so the removal is deliberate.
- `preserve/w2-impl-strong-e2e-zai` `b8b0cda4` is the rejected fake-zai-account e2e setup. Main's `crates/xtask/src/e2e/setup.rs` is target-aware and has no zai accounts.
- Old crate trees referenced by the stash and recovery branches (`crates/adapters`, `crates/services`, `crates/tools/jackin-xtask`, `crates/core/jackin-process-directory`) have 0 files on main, which confirms their removal.

## Scratch artifacts

Working files: `<scratchpad>/manifest/v2/` (`sha-check.tsv`, `unique-commits.tsv`, `applycheck.tsv`, `commit-presence.tsv`, `symbols-missing.tsv`, `build_report.py`).
