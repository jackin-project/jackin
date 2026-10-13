# 01 — Branch inventory

> **Planning status: PARTIAL EVIDENCE / IMPLEMENTATION BLOCKED.** All 79 discovered server branches have dossiers and all 50 distinct tips received some inspection. Complete unique-history/semantic closure, the operator's local inventory, Jackin test execution and independent review are not complete. These are hard execution gates, not silently waived work.

## Scope and counting

**79 server-side `refs/heads/*` references; 50 distinct tip objects; zero exclusions.** Inventory was obtained using the GitHub branches API with 20 results per page: pages 1–3 each returned 20, page 4 returned 19 and page 5 returned an empty array. Search-branches independently exposed the names. [Normalized snapshot](evidence/remote-branches.json) records exact names, full tips, IDs and dossier mappings.

These are remote server branch heads, not proof of a local `origin/*` mapping. There is no operator-checkout local-branch inventory; TASK-001 must add it. `rescue/local` in a server ref name does not close that gap. Branch owner/upstream relationship is unknown unless later confirmed; a commit author or encoded ref name is not ownership proof.

Thirty stash-named refs point to two distinct modern stash objects; `ci-tooling/pr1121-ledger-454-20261010` and `rust-policy-rollout` share P. Exact-ref dossiers remain separate to prevent coverage loss. Repeated tips share findings, not fictitious independent reviews.

## Key observed relationships

Ahead/behind numbers below are **head relative to the stated base**, not always relative to main.

| Base | Head | Head ahead | Head behind | Merge base |
| --- | --- | --- | --- | --- |
| M | P | 366 | 0 | 1c3dd1be4a3b1ed518133ec3899c084c792370ff |
| M | U | 25 | 0 | 1c3dd1be4a3b1ed518133ec3899c084c792370ff |
| M | T | 2 | 4 | 9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b |
| U | P | 366 | 25 | 1c3dd1be4a3b1ed518133ec3899c084c792370ff |
| P | T | 2 | 370 | 9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b |
| P | ef238a7e | 1 | 362 | 32d78964698cdd1d97eb9fcb180e410348e46d70 |
| P | bff77099 | 2 | 169 | 95629842235f0d19a89b8e2d09c4e4813867fb5b |
| 3ddbc056 | 40823a76 | 10 | 1 | d32e85563648931f80ffab141bb2650f478c2a60 |

M=`1c3dd1be4a3b1ed518133ec3899c084c792370ff`; P=`fba69d60c2373156dc0dcecee8e085c83776966f`; U=`2e4a62da4bc4406764162a267163389a5fb513a7`; T=`9ef617d014df15c6d967501bcbbdcb3689ed453c`. Other main-relative values are **not computed**, not zero. Full pairwise structural comparison is 1,225 unordered pairs for the original 50 tips; semantic overlap families need additional review.

## Exact-reference inventory and dossier index

| ID / dossier | Exact server ref | Tip SHA | Group | Inspection / closure |
| --- | --- | --- | --- | --- |
| [B001](branches/b001-archive-docs-437a9e81ba65.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/docs/build-split-c72-record/tip-3cb98f2a634c1e660e501489d7383329569db7a3` | `3cb98f2a634c1e660e501489d7383329569db7a3` | archive-docs | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B002](branches/b002-archive-docs-33af45993b41.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/docs/build-split-evidence-41265/tip-b1e08b880223594cceccf3ea44322d44e3cdf984` | `b1e08b880223594cceccf3ea44322d44e3cdf984` | archive-docs | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B003](branches/b003-archive-recovery-583438c051c5.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/cleanup-20261006/jackin-unreferenced-tree-abc5b11ca1b2/tip-fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` | `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` | archive-recovery | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B004](branches/b004-archive-keeper-5aacaad28a02.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-0a898cd246fbbf62f3b463f0097ffd6a368b1e07/tip-0a898cd246fbbf62f3b463f0097ffd6a368b1e07` | `0a898cd246fbbf62f3b463f0097ffd6a368b1e07` | archive-keeper | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B005](branches/b005-archive-keeper-efa220a4ecda.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d/tip-1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d` | `1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d` | archive-keeper | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B006](branches/b006-archive-keeper-856b4d85181f.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d/tip-1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d` | `1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B007](branches/b007-archive-keeper-74cf60fb6d52.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e/tip-5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e` | `5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B008](branches/b008-archive-keeper-77baf007acb6.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-5a8264746286a981debb099646a6354d9b10e0cb/tip-5a8264746286a981debb099646a6354d9b10e0cb` | `5a8264746286a981debb099646a6354d9b10e0cb` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B009](branches/b009-archive-keeper-dd596b9a057a.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-5c4aa5eba5aaea2f2bfa2339f91066567a8e9979/tip-5c4aa5eba5aaea2f2bfa2339f91066567a8e9979` | `5c4aa5eba5aaea2f2bfa2339f91066567a8e9979` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B010](branches/b010-archive-keeper-8fd347cb734e.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-6d65e649855f98f133f58f605727b098b5d4eabb/tip-6d65e649855f98f133f58f605727b098b5d4eabb` | `6d65e649855f98f133f58f605727b098b5d4eabb` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B011](branches/b011-archive-keeper-64de0538eaa5.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-07beb1c705ee8f813d2c8f5a438291cc031c9d1a/tip-07beb1c705ee8f813d2c8f5a438291cc031c9d1a` | `07beb1c705ee8f813d2c8f5a438291cc031c9d1a` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B012](branches/b012-archive-keeper-27c1d7ad2d95.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-14bd1471616056e07c81c59d59eafc0963b16865/tip-14bd1471616056e07c81c59d59eafc0963b16865` | `14bd1471616056e07c81c59d59eafc0963b16865` | archive-keeper | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B013](branches/b013-archive-keeper-e118ca686ade.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-256b88d1a7e85b4855d1e64399e2af4482026e17/tip-256b88d1a7e85b4855d1e64399e2af4482026e17` | `256b88d1a7e85b4855d1e64399e2af4482026e17` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B014](branches/b014-archive-keeper-4d364008016b.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-631d734b99ee9b413230ddb210698a5c80f41d70/tip-631d734b99ee9b413230ddb210698a5c80f41d70` | `631d734b99ee9b413230ddb210698a5c80f41d70` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B015](branches/b015-archive-keeper-dc5b26803593.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-7646c61bd4261a5908693bdaf0a6ecd9e9643448/tip-7646c61bd4261a5908693bdaf0a6ecd9e9643448` | `7646c61bd4261a5908693bdaf0a6ecd9e9643448` | archive-keeper | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B016](branches/b016-archive-keeper-f164586efdc9.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-66227a0033a6f8b917e684de1776753e33978590/tip-66227a0033a6f8b917e684de1776753e33978590` | `66227a0033a6f8b917e684de1776753e33978590` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B017](branches/b017-archive-keeper-b3531a0bac69.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-77508d454bfe9a903043586db2c8c5faa1f9e960/tip-77508d454bfe9a903043586db2c8c5faa1f9e960` | `77508d454bfe9a903043586db2c8c5faa1f9e960` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B018](branches/b018-archive-keeper-6176f4f904dc.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-a28ab0a27eccc892decf8a4df55244b70096f631/tip-a28ab0a27eccc892decf8a4df55244b70096f631` | `a28ab0a27eccc892decf8a4df55244b70096f631` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B019](branches/b019-archive-keeper-7cddcc8364f3.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-b930e3658c671ca21ab7c25ea1a40349bb69284a/tip-b930e3658c671ca21ab7c25ea1a40349bb69284a` | `b930e3658c671ca21ab7c25ea1a40349bb69284a` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B020](branches/b020-archive-keeper-83afb29df592.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-c7b8a95856cce6f5e8442f0e6fafcf025a089132/tip-c7b8a95856cce6f5e8442f0e6fafcf025a089132` | `c7b8a95856cce6f5e8442f0e6fafcf025a089132` | archive-keeper | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B021](branches/b021-archive-keeper-45846051b2ee.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78/tip-d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78` | `d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B022](branches/b022-archive-keeper-0058bbcc86ff.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb/tip-df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb` | `df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B023](branches/b023-archive-keeper-a760f85ce5ae.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-e978dda9643079998f342380ccd123562139858a/tip-e978dda9643079998f342380ccd123562139858a` | `e978dda9643079998f342380ccd123562139858a` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B024](branches/b024-archive-keeper-d898a35778ef.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-ffe6d6767bf2be30f54df7d245574b5980c05eac/tip-ffe6d6767bf2be30f54df7d245574b5980c05eac` | `ffe6d6767bf2be30f54df7d245574b5980c05eac` | archive-keeper | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B025](branches/b025-archive-staged-67aa054ffa3d.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/staged-index-jackin-1044-current-0437497d7f22/tip-0437497d7f22fbdb7c8aad986be932704d707fe9` | `0437497d7f22fbdb7c8aad986be932704d707fe9` | archive-staged | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B026](branches/b026-archive-sanitized-0ced81950334.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/source3-sanitized/8e14f13a810898b2a54aebccb336d95fc6686ca5/tip-2d97bba390864fbdc79aa6566e3394df938408af` | `2d97bba390864fbdc79aa6566e3394df938408af` | archive-sanitized | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B027](branches/b027-archive-sanitized-0739580d021c.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/source3-sanitized/8f2d72ac88d4f15b1435aae1f508fcb2d7ca5eab/tip-502e0627d3940e2cd68ae0078910f21452d9ccc1` | `502e0627d3940e2cd68ae0078910f21452d9ccc1` | archive-sanitized | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B028](branches/b028-archive-src3-95287c049e3e.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-6ce20ce13cb8/tip-6ce20ce13cb87c5ab89b44d72698fa0d68aacf39` | `6ce20ce13cb87c5ab89b44d72698fa0d68aacf39` | archive-src3 | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B029](branches/b029-archive-src3-21f83e144de4.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-8fb49688a640/tip-8fb49688a6408627e22118f92d24f47785564979` | `8fb49688a6408627e22118f92d24f47785564979` | archive-src3 | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B030](branches/b030-archive-src3-841f56f9a292.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-12dea6c5fd0f/tip-12dea6c5fd0f9d2c594d5ad3159e6493d03352f4` | `12dea6c5fd0f9d2c594d5ad3159e6493d03352f4` | archive-src3 | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B031](branches/b031-archive-src3-75be4f6a7848.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-432e0108ce90/tip-432e0108ce90b64b428c69c27d4a2ac23e559379` | `432e0108ce90b64b428c69c27d4a2ac23e559379` | archive-src3 | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B032](branches/b032-archive-src3-70770d79115b.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-b1dc105acffb/tip-b1dc105acffb68ba71fff86f9c18dc5ff77de924` | `b1dc105acffb68ba71fff86f9c18dc5ff77de924` | archive-src3 | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B033](branches/b033-archive-src3-9264ed8676fb.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-d16cd795d4a5/tip-d16cd795d4a539b84ba5f44290663b2363d49350` | `d16cd795d4a539b84ba5f44290663b2363d49350` | archive-src3 | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B034](branches/b034-archive-src3-ac5215e0b69e.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/unreachable-33d88b1cf764/tip-33d88b1cf764f614d3935c11c48decd53386083e` | `33d88b1cf764f614d3935c11c48decd53386083e` | archive-src3 | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B035](branches/b035-modernize-9949620d3954.md) | `refs/heads/chore/rust-mise-mbx-modernize` | `9ef617d014df15c6d967501bcbbdcb3689ed453c` | modernize | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B036](branches/b036-policy-0b27f2016591.md) | `refs/heads/ci-tooling/pr1121-ledger-454-20261010` | `fba69d60c2373156dc0dcecee8e085c83776966f` | policy | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B037](branches/b037-usage-1c388dc4de71.md) | `refs/heads/fix/claude-usage-live-repair` | `2e4a62da4bc4406764162a267163389a5fb513a7` | usage | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B038](branches/b038-baseline-f921bd05e68b.md) | `refs/heads/main` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | baseline | BASELINE_INSPECTED_NOT_EXECUTED; history OPEN; independent review NOT_RUN |
| [B039](branches/b039-misrouted-e096ae35b5a8.md) | `refs/heads/recovery/cleanup-20261006/misrouted-jackin/src-2e29a881/dangling-sccache-successor-41dd148e514d` | `41dd148e514d1bf3f84401f0ecbf8e87b58bb261` | misrouted | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B040](branches/b040-usage-followup-bf245b09ab75.md) | `refs/heads/rescue/local/937c9c5f-a870-43c7-aa58-36a3f5c1b9cf/ed5b0cf1d7a4fdfe4e62eed7/usage-r37-followup` | `2abed4ad67d8438ab9eb124b564be6ad5b545847` | usage-followup | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B041](branches/b041-policy-rescue-bb88995b68e3.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/bdb98a128593444a2c9f2482/rust-policy-rollout-ef238a7e` | `ef238a7e4720fe1be8e8e295e1c502f2ba529e0b` | policy-rescue | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B042](branches/b042-policy-rescue-4dc1add6a9f0.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/c4ea76082d0e4a93c792f46d/rust-policy-rollout-bff77099` | `bff77099f8273a4d480a0e430cba66afc4197543` | policy-rescue | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B043](branches/b043-policy-rescue-cf95d97cce7c.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/d5c53271a9c5551fb1b8813b/rust-policy-rollout-3ddbc056` | `3ddbc0563ed95225b0004c393b3928fb599ee066` | policy-rescue | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B044](branches/b044-dirty-3eaee4abbc92.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/dirty/jackin-s7-usage-d5c53271a9` | `40823a76a3e20b901d5bc410979cee0931fd5c48` | dirty | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B045](branches/b045-dirty-868886b2213b.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/dirty/jackin-s61-c4ea76082d` | `45e834683c8d5ca0fea1c7d3676cb136efa4acb1` | dirty | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B046](branches/b046-dirty-fc03ff5ffa1d.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/dirty/jackin-v011-validate-a8e16751f8` | `b72046bbc9314e93cddf4d8252570dc375935f56` | dirty | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B047](branches/b047-stash-alias-53dd4c6affcc.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/796109848951-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B048](branches/b048-stash-alias-b7b19a9f3034.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/b60d2272bc89-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B049](branches/b049-stash-alias-371a2456f4ff.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/bbb65f5a2a52-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B050](branches/b050-stash-alias-bfd7103e838f.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c0ab9196b766-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B051](branches/b051-stash-alias-7512abdb59f0.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c3cf9a0eb43a-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B052](branches/b052-stash-alias-01522f00a01b.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c46962aa8530-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B053](branches/b053-stash-alias-4c4fe5556b72.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c39770351b8f-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B054](branches/b054-stash-alias-68ff05fe0f69.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/d2404462eaed-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B055](branches/b055-stash-alias-f8acf47b8695.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/e03ad68a1423-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B056](branches/b056-stash-alias-cbbb9b2a9c6e.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/e721fc3aea40-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B057](branches/b057-stash-alias-09031c14e05c.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/e348490bce40-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B058](branches/b058-stash-alias-373bb2fc05d3.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/f349f6398aa7-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B059](branches/b059-stash-alias-ebc2e2faed24.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/3db8bf2ee028-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B060](branches/b060-stash-alias-15ca898cb2c6.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/4ac430a548ca-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B061](branches/b061-stash-alias-cf15510f83af.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/4e8bc3e16a54-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B062](branches/b062-stash-alias-dca0343dfb05.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/07ad558976dc-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B063](branches/b063-stash-alias-54f51006e189.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/7b2538888f01-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B064](branches/b064-stash-alias-abb1dffe6447.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/8e79df941bcc-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B065](branches/b065-stash-alias-0e6388966c3a.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/18a078e0fd97-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B066](branches/b066-stash-alias-b1eced4af625.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/31ad21a631b3-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B067](branches/b067-stash-alias-31cbf60dff61.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/41aec636bd17-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B068](branches/b068-stash-alias-929b5577bb5f.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/51e08660dd2e-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B069](branches/b069-stash-alias-8836a20a65fb.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/596d8e54fd97-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B070](branches/b070-stash-alias-f574e4820623.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/4456e9b83763-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B071](branches/b071-stash-alias-42c120365146.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/24255e91bdc8-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B072](branches/b072-stash-alias-64003e4d826d.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/187535a508ae-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B073](branches/b073-stash-alias-2cc727bc102c.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/650748ab814e-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B074](branches/b074-stash-alias-bc491d591458.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/75703094d1b6-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B075](branches/b075-stash-alias-770a8c6e1ecf.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/90e8831cbed7f0348cb01ab4/rust-policy-rollout-stash-80ab980` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B076](branches/b076-stash-alias-8e34450f0fb7.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/90e8831cbed7f0348cb01ab4/rust-policy-rollout-stash-b1b9078` | `b1b90781739333ba7c8829c092a845f6ea82b11c` | stash-alias | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B077](branches/b077-policy-517d17ddf7fc.md) | `refs/heads/rust-policy-rollout` | `fba69d60c2373156dc0dcecee8e085c83776966f` | policy | PARTIAL_SEMANTIC_REVIEW; history OPEN; independent review NOT_RUN |
| [B078](branches/b078-wip-6b8347c05938.md) | `refs/heads/wip/mise-lock-snapshot-20261007-ef40fea11a54` | `ef40fea11a54ade7911de7622e6e011707241dd4` | wip | TIP_PATCH_REVIEW; history OPEN; independent review NOT_RUN |
| [B079](branches/b079-wip-ebf572949c76.md) | `refs/heads/wip/reflog-snapshot-20261007-d04467f05f11` | `d04467f05f1183008322047b1828c201e551e947` | wip | TIP_PARTIAL_PATCH_REVIEW; history OPEN; independent review NOT_RUN |

## Group counts

| Group | Refs |
| --- | --- |
| archive-docs | 2 |
| archive-keeper | 21 |
| archive-recovery | 1 |
| archive-sanitized | 2 |
| archive-src3 | 7 |
| archive-staged | 1 |
| baseline | 1 |
| dirty | 3 |
| misrouted | 1 |
| modernize | 1 |
| policy | 2 |
| policy-rescue | 3 |
| stash-alias | 30 |
| usage | 1 |
| usage-followup | 1 |
| wip | 2 |

## Coverage closure rule

Each dossier states observed evidence, source-tip changes, overlaps and unresolved history. A dossier existing does not mean all reachable changes have been analyzed. Before implementation, TASK-002 must reconcile the unique-commit set and every relevant parent/tree/file against the change matrix. The prior October 6 consolidation's claims are useful leads, not a substitute for current semantic evidence.

## Local reconciliation (executed 2026-10-13)

The planning directory `/Users/donbeave/Projects/merge/jackin` is not a Git checkout and was not initialized as one. No pre-existing clone of `github.com/jackin-project/jackin` was found. A full, non-shallow clone was created at `/Users/donbeave/Projects/jackin-project/jackin` after the absence was recorded.

| Fact | Value |
| --- | --- |
| `BASE_SHA` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` (`refs/heads/main` and `refs/remotes/origin/main`; unchanged from the planning pin) |
| Local heads | `refs/heads/main` only, upstream `origin/main`, clean worktree, no untracked paths |
| Worktrees at inventory time | one: the clone above |
| Configured remotes | `origin` = `https://github.com/jackin-project/jackin.git` |
| Configured fetch refspec | `+refs/heads/*:refs/remotes/origin/*` (force flag is in config; it was not used for the audit fetch) |
| Pre-fetch snapshot | `/Users/donbeave/Projects/merge/jackin-evidence-prefetch-20261013/` |
| Audit namespace | `refs/audit/20261013T2203Z/origin/*` (79 heads, non-forcing refspec, no prune). Zero SHA differences versus `refs/remotes/origin/*` |
| Advertised versus pins | 79 names, 79 identical tips, 50 distinct objects, 0 moved, 0 added, 0 missing |
| Collector | `/Users/donbeave/Projects/jackin-project/jackin-evidence-20261013` exit 0: 80 branch refs, 50 tips, 1225 pairwise rows, 1787 patch-id groups, 1888 reachable commits, not shallow, refs and worktree unchanged. Exit 0 is structural collection only |
| Ruleset 14746904 | active `protect-main`; bypass actors 0; deletion, non-fast-forward, linear history; squash-only PRs; thread resolution; required checks `DCO` and `Required`; approving review count 0. Remote writes remain disabled |
| Primary graph reconfirmed | P 366 ahead / 0 behind M; U 25 / 0; T 2 ahead / 4 behind, merge base `9b6e1d92608fed65579b0ab5bb4b04d2a8dd602b` |

Discovered non-checkout artifacts, retained with a disposition rather than dropped:

| ID | Location | Disposition |
| --- | --- | --- |
| L-BUNDLE-20260923 | `/Users/donbeave/.codex/recovery/chainargos-java-monorepo-reconciliation-20260923T/jackin/jackin-all.bundle` (inspected at `/Users/donbeave/Projects/jackin-project/jackin-bundle-20260923`) | Out of scope. Zero objects exist in the Jackin clone. HEAD `092fe1db796367479029d4e94379530f7f5965c0` is the ChainArgos Agent Brown role repo, not `jackin-project/jackin`. |
| L-TREE-20260921 | `/Users/donbeave/Projects/work/jackin-index-hesbn6n4` | Closed as an ancestor tree. Report `task-002-snapshot-20260921.md` identifies commit `fce94cea8a15de0c2db3bb4ff880d741baf5c00a` (`fix(security): isolate Apple usage relay supervisor (#1012)`), tree `e12910141bf9062c34a487f3203543dbafbe4d6b`. Coordinator recheck: that object is a commit, that tree OID matches, and the commit is an ancestor of both M and P, with 69 commits on M after it. No unique tip behavior to preserve. |

No server pin disappeared or moved, so no disappeared-ref dossier was required. History and behavior rows stay OPEN; this section does not close TASK-002.

Independent inventory review: ACCEPT. Report: `/Users/donbeave/Projects/merge/jackin-evidence-prefetch-20261013/task-001-independent-review.md`. Reviewer was not the clone or collector operator. Checks 1–7 passed: HEAD/upstream/clean primary worktree, 0 pin mismatches against origin and `refs/audit/20261013T2203Z/origin/`, 50 tips, 1225 pairwise rows, 0 missing objects, P/U/T ahead-behind reconfirmed, and the September bundle shares 0 commits with this clone. The four detached baseline worktrees were created after collection, add no local heads, and were clean at review. TASK-001 inventory gate is accepted. Semantic history remains TASK-002.
