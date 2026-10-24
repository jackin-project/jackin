# Late main overlay: immutable preview and native tap tooling

Recorded **2026-10-24T08:49:28.303211+00:00**. This is an independent source/configuration comparison of [`c98f5f5fdd83`](https://github.com/jackin-project/jackin/commit/c98f5f5fdd831e6075e38fde93b9713575406140) to [`e737f4b5eb7c`](https://github.com/jackin-project/jackin/commit/e737f4b5eb7c1a0d15ba6595fc260407ad178452), after the coordinator's clean no-commit merge. It does not change the external research cutoff of **2026-10-24T01:15:00Z**. Full paths, byte hashes, source URLs and parsed comparisons are in [late-main-overlay.json](./late-main-overlay.json).

## Source continuity and the one changed audit file

**25 of the original 26 audited files remain byte-identical**, and **all 13 files in the separate c98 main-overlay audit remain byte-identical**. The exception is `Cargo.toml`: its sole change is `velnor-actions-contract` from **0.1.48** at `9191feacdb538a8df8299b73eaf2ece63f2645d7` to **0.1.52** at `1bd31f8203ecf40d141c331e5195eb422e0904d3`. Every other parsed field in the root Cargo manifest is identical. The previous source-audit records retain their original snapshots.

The entire c98-to-e737 diff contains **26 files**, **no `.rs` file**, and **no `docs/content/research/context/` path**. It changes generated CI/release tooling, native Homebrew tap configuration, the Velnor contract dependency, and release/install documentation. The full-package multiset comparison of `Cargo.lock` finds only the Velnor package version/source replacement; its dependency list is unchanged.

| Integration constraint | Previous main and late main | Result |
| --- | --- | --- |
| `rusqlite` | 0.40.2 | Complete locked package record unchanged |
| `libsqlite3-sys` | 0.38.2 | Complete locked package record unchanged |
| `tokio` | 1.53.3 | Complete locked package record unchanged |
| `rust-toolchain.toml` | Exact file bytes | Unchanged |
| Velnor generator/contract | 0.1.48 → 0.1.52 | Tooling verification must use the merged tree |

The source basis for the researched launch/hook/usage boundaries and the existing SQLite linkage findings therefore remains intact. This does **not** turn earlier compiler or CI results into proof of the changed Velnor toolchain.

## CI coverage after the update

The PR CI graph is unchanged: **40 jobs**, including **32 Rust crate jobs**, and the same **191 Rust task IDs**. `Required verdict` still depends on **37 jobs**. The separate **Required** job runs with `always()` and fails unless the verdict succeeds. The PR, main-push and merge-group triggers, concurrency policy, dependency graph and gate definition are identical. The generator and acquired binary identity change to Velnor 0.1.52. Every task changes its `report_helper_version` and `execution_digest`; argv, environment and every other task field are identical.

There is **no Bun/Next/Fumadocs site-build, site-typecheck or site-test lane** in this configuration. Discovery still excludes `docs/**`. Rust documentation tasks invoke `cargo doc`; the separate `repository-links` check covers repository documentation links. The coordinator's local full documentation gate must be reported separately.

Two new native tap reconciliation workflows run on a five-minute schedule or manual dispatch. The preview production workflow is triggered by completed main CI or manual recovery; stable release remains manually dispatched. These release workflows are not additional PR CI jobs, and this inspection does not certify their security or publication behavior.

## Merge and verification boundary

All 26 inherited main files match their exact e737 bytes in the merge index at inspection. Final-head verification must still confirm that main changes are absent from the research PR diff. The local head at inspection is checkpoint `9971e26249c2a601912419c84aad99980f88d28b`; `MERGE_HEAD` is e737. The coordinator will supply the final research head before the independent CI verdict.

This overlay performs read-only Git, TOML, YAML and SHA-256 comparisons. It runs no compiler, tests, native agent, optimizer, provider call, release publication or tap update. Earlier source, probe, review and build hashes retain their historical snapshot meaning.
