# Reference index

Snapshot: 13 October 2026, Asia/Singapore. **79 server refs / 50 distinct tips; local and full semantic coverage remain gated.**

[Main plan](../GOAL.md) · [Standalone /goal prompt](../GOAL_PROMPT.md) · [Bundle guide](../README.md)

## Primary references

- [00 repository baseline](00-repository-baseline.md)
- [01 branch inventory](01-branch-inventory.md)
- [02 change coverage matrix](02-change-coverage-matrix.md)
- [03 overlaps and conflicts](03-overlaps-and-conflicts.md)
- [04 target architecture](04-target-architecture.md)
- [05 execution tasks](05-execution-tasks.md)
- [06 validation and test plan](06-validation-and-test-plan.md)
- [07 risks and rollback](07-risks-and-rollback.md)
- [08 review and decision log](08-review-and-decision-log.md)
- [09 evidence and limitations](09-evidence-and-limitations.md)
- [10 artifact integrity](10-artifact-integrity.md)

## Tools and normalized evidence

[Tools guide](tools/README.md) · [Read-only collector](tools/collect_git_evidence.py) · [Fixture tests](tools/test_collector.py) · [Artifact validator](tools/validate_bundle.py) · [Branch JSON](evidence/remote-branches.json) · [Observations](evidence/observations.json) · [Decisions](evidence/decisions.json) · [Tasks](evidence/tasks.json) · [Synthetic results](evidence/collector-test-results.txt) · [Artifact report](evidence/artifact-validation.json)

## Complete exact-ref dossier index

Every row is in scope; no exclusions. Same-tip refs share evidence, not independent review. A dossier is not evidence that its full ancestry has been analyzed.

| ID / dossier | Exact server ref | Pinned tip |
| --- | --- | --- |
| [B001](branches/b001-archive-docs-437a9e81ba65.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/docs/build-split-c72-record/tip-3cb98f2a634c1e660e501489d7383329569db7a3` | `3cb98f2a634c1e660e501489d7383329569db7a3` |
| [B002](branches/b002-archive-docs-33af45993b41.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/docs/build-split-evidence-41265/tip-b1e08b880223594cceccf3ea44322d44e3cdf984` | `b1e08b880223594cceccf3ea44322d44e3cdf984` |
| [B003](branches/b003-archive-recovery-583438c051c5.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/cleanup-20261006/jackin-unreferenced-tree-abc5b11ca1b2/tip-fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` | `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` |
| [B004](branches/b004-archive-keeper-5aacaad28a02.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-0a898cd246fbbf62f3b463f0097ffd6a368b1e07/tip-0a898cd246fbbf62f3b463f0097ffd6a368b1e07` | `0a898cd246fbbf62f3b463f0097ffd6a368b1e07` |
| [B005](branches/b005-archive-keeper-efa220a4ecda.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d/tip-1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d` | `1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d` |
| [B006](branches/b006-archive-keeper-856b4d85181f.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d/tip-1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d` | `1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d` |
| [B007](branches/b007-archive-keeper-74cf60fb6d52.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e/tip-5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e` | `5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e` |
| [B008](branches/b008-archive-keeper-77baf007acb6.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-5a8264746286a981debb099646a6354d9b10e0cb/tip-5a8264746286a981debb099646a6354d9b10e0cb` | `5a8264746286a981debb099646a6354d9b10e0cb` |
| [B009](branches/b009-archive-keeper-dd596b9a057a.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-5c4aa5eba5aaea2f2bfa2339f91066567a8e9979/tip-5c4aa5eba5aaea2f2bfa2339f91066567a8e9979` | `5c4aa5eba5aaea2f2bfa2339f91066567a8e9979` |
| [B010](branches/b010-archive-keeper-8fd347cb734e.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-6d65e649855f98f133f58f605727b098b5d4eabb/tip-6d65e649855f98f133f58f605727b098b5d4eabb` | `6d65e649855f98f133f58f605727b098b5d4eabb` |
| [B011](branches/b011-archive-keeper-64de0538eaa5.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-07beb1c705ee8f813d2c8f5a438291cc031c9d1a/tip-07beb1c705ee8f813d2c8f5a438291cc031c9d1a` | `07beb1c705ee8f813d2c8f5a438291cc031c9d1a` |
| [B012](branches/b012-archive-keeper-27c1d7ad2d95.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-14bd1471616056e07c81c59d59eafc0963b16865/tip-14bd1471616056e07c81c59d59eafc0963b16865` | `14bd1471616056e07c81c59d59eafc0963b16865` |
| [B013](branches/b013-archive-keeper-e118ca686ade.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-256b88d1a7e85b4855d1e64399e2af4482026e17/tip-256b88d1a7e85b4855d1e64399e2af4482026e17` | `256b88d1a7e85b4855d1e64399e2af4482026e17` |
| [B014](branches/b014-archive-keeper-4d364008016b.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-631d734b99ee9b413230ddb210698a5c80f41d70/tip-631d734b99ee9b413230ddb210698a5c80f41d70` | `631d734b99ee9b413230ddb210698a5c80f41d70` |
| [B015](branches/b015-archive-keeper-dc5b26803593.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-7646c61bd4261a5908693bdaf0a6ecd9e9643448/tip-7646c61bd4261a5908693bdaf0a6ecd9e9643448` | `7646c61bd4261a5908693bdaf0a6ecd9e9643448` |
| [B016](branches/b016-archive-keeper-f164586efdc9.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-66227a0033a6f8b917e684de1776753e33978590/tip-66227a0033a6f8b917e684de1776753e33978590` | `66227a0033a6f8b917e684de1776753e33978590` |
| [B017](branches/b017-archive-keeper-b3531a0bac69.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-77508d454bfe9a903043586db2c8c5faa1f9e960/tip-77508d454bfe9a903043586db2c8c5faa1f9e960` | `77508d454bfe9a903043586db2c8c5faa1f9e960` |
| [B018](branches/b018-archive-keeper-6176f4f904dc.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-a28ab0a27eccc892decf8a4df55244b70096f631/tip-a28ab0a27eccc892decf8a4df55244b70096f631` | `a28ab0a27eccc892decf8a4df55244b70096f631` |
| [B019](branches/b019-archive-keeper-7cddcc8364f3.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-b930e3658c671ca21ab7c25ea1a40349bb69284a/tip-b930e3658c671ca21ab7c25ea1a40349bb69284a` | `b930e3658c671ca21ab7c25ea1a40349bb69284a` |
| [B020](branches/b020-archive-keeper-83afb29df592.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-c7b8a95856cce6f5e8442f0e6fafcf025a089132/tip-c7b8a95856cce6f5e8442f0e6fafcf025a089132` | `c7b8a95856cce6f5e8442f0e6fafcf025a089132` |
| [B021](branches/b021-archive-keeper-45846051b2ee.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78/tip-d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78` | `d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78` |
| [B022](branches/b022-archive-keeper-0058bbcc86ff.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb/tip-df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb` | `df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb` |
| [B023](branches/b023-archive-keeper-a760f85ce5ae.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-e978dda9643079998f342380ccd123562139858a/tip-e978dda9643079998f342380ccd123562139858a` | `e978dda9643079998f342380ccd123562139858a` |
| [B024](branches/b024-archive-keeper-d898a35778ef.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/root-ffe6d6767bf2be30f54df7d245574b5980c05eac/tip-ffe6d6767bf2be30f54df7d245574b5980c05eac` | `ffe6d6767bf2be30f54df7d245574b5980c05eac` |
| [B025](branches/b025-archive-staged-67aa054ffa3d.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/keeper/staged-index-jackin-1044-current-0437497d7f22/tip-0437497d7f22fbdb7c8aad986be932704d707fe9` | `0437497d7f22fbdb7c8aad986be932704d707fe9` |
| [B026](branches/b026-archive-sanitized-0ced81950334.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/source3-sanitized/8e14f13a810898b2a54aebccb336d95fc6686ca5/tip-2d97bba390864fbdc79aa6566e3394df938408af` | `2d97bba390864fbdc79aa6566e3394df938408af` |
| [B027](branches/b027-archive-sanitized-0739580d021c.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/source3-sanitized/8f2d72ac88d4f15b1435aae1f508fcb2d7ca5eab/tip-502e0627d3940e2cd68ae0078910f21452d9ccc1` | `502e0627d3940e2cd68ae0078910f21452d9ccc1` |
| [B028](branches/b028-archive-src3-95287c049e3e.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-6ce20ce13cb8/tip-6ce20ce13cb87c5ab89b44d72698fa0d68aacf39` | `6ce20ce13cb87c5ab89b44d72698fa0d68aacf39` |
| [B029](branches/b029-archive-src3-21f83e144de4.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-8fb49688a640/tip-8fb49688a6408627e22118f92d24f47785564979` | `8fb49688a6408627e22118f92d24f47785564979` |
| [B030](branches/b030-archive-src3-841f56f9a292.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-12dea6c5fd0f/tip-12dea6c5fd0f9d2c594d5ad3159e6493d03352f4` | `12dea6c5fd0f9d2c594d5ad3159e6493d03352f4` |
| [B031](branches/b031-archive-src3-75be4f6a7848.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-432e0108ce90/tip-432e0108ce90b64b428c69c27d4a2ac23e559379` | `432e0108ce90b64b428c69c27d4a2ac23e559379` |
| [B032](branches/b032-archive-src3-70770d79115b.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-b1dc105acffb/tip-b1dc105acffb68ba71fff86f9c18dc5ff77de924` | `b1dc105acffb68ba71fff86f9c18dc5ff77de924` |
| [B033](branches/b033-archive-src3-9264ed8676fb.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/branch-d16cd795d4a5/tip-d16cd795d4a539b84ba5f44290663b2363d49350` | `d16cd795d4a539b84ba5f44290663b2363d49350` |
| [B034](branches/b034-archive-src3-ac5215e0b69e.md) | `refs/heads/archive/cleanup-20261006/jackin-origin/recovery/jackin-20261004T211752Z-e002bd55/src3-8fe2b0e6dc1c/unreachable-33d88b1cf764/tip-33d88b1cf764f614d3935c11c48decd53386083e` | `33d88b1cf764f614d3935c11c48decd53386083e` |
| [B035](branches/b035-modernize-9949620d3954.md) | `refs/heads/chore/rust-mise-mbx-modernize` | `9ef617d014df15c6d967501bcbbdcb3689ed453c` |
| [B036](branches/b036-policy-0b27f2016591.md) | `refs/heads/ci-tooling/pr1121-ledger-454-20261010` | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| [B037](branches/b037-usage-1c388dc4de71.md) | `refs/heads/fix/claude-usage-live-repair` | `2e4a62da4bc4406764162a267163389a5fb513a7` |
| [B038](branches/b038-baseline-f921bd05e68b.md) | `refs/heads/main` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| [B039](branches/b039-misrouted-e096ae35b5a8.md) | `refs/heads/recovery/cleanup-20261006/misrouted-jackin/src-2e29a881/dangling-sccache-successor-41dd148e514d` | `41dd148e514d1bf3f84401f0ecbf8e87b58bb261` |
| [B040](branches/b040-usage-followup-bf245b09ab75.md) | `refs/heads/rescue/local/937c9c5f-a870-43c7-aa58-36a3f5c1b9cf/ed5b0cf1d7a4fdfe4e62eed7/usage-r37-followup` | `2abed4ad67d8438ab9eb124b564be6ad5b545847` |
| [B041](branches/b041-policy-rescue-bb88995b68e3.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/bdb98a128593444a2c9f2482/rust-policy-rollout-ef238a7e` | `ef238a7e4720fe1be8e8e295e1c502f2ba529e0b` |
| [B042](branches/b042-policy-rescue-4dc1add6a9f0.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/c4ea76082d0e4a93c792f46d/rust-policy-rollout-bff77099` | `bff77099f8273a4d480a0e430cba66afc4197543` |
| [B043](branches/b043-policy-rescue-cf95d97cce7c.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/d5c53271a9c5551fb1b8813b/rust-policy-rollout-3ddbc056` | `3ddbc0563ed95225b0004c393b3928fb599ee066` |
| [B044](branches/b044-dirty-3eaee4abbc92.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/dirty/jackin-s7-usage-d5c53271a9` | `40823a76a3e20b901d5bc410979cee0931fd5c48` |
| [B045](branches/b045-dirty-868886b2213b.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/dirty/jackin-s61-c4ea76082d` | `45e834683c8d5ca0fea1c7d3676cb136efa4acb1` |
| [B046](branches/b046-dirty-fc03ff5ffa1d.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/dirty/jackin-v011-validate-a8e16751f8` | `b72046bbc9314e93cddf4d8252570dc375935f56` |
| [B047](branches/b047-stash-alias-53dd4c6affcc.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/796109848951-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B048](branches/b048-stash-alias-b7b19a9f3034.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/b60d2272bc89-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B049](branches/b049-stash-alias-371a2456f4ff.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/bbb65f5a2a52-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B050](branches/b050-stash-alias-bfd7103e838f.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c0ab9196b766-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B051](branches/b051-stash-alias-7512abdb59f0.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c3cf9a0eb43a-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B052](branches/b052-stash-alias-01522f00a01b.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c46962aa8530-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B053](branches/b053-stash-alias-4c4fe5556b72.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/c39770351b8f-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B054](branches/b054-stash-alias-68ff05fe0f69.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/d2404462eaed-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B055](branches/b055-stash-alias-f8acf47b8695.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/e03ad68a1423-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B056](branches/b056-stash-alias-cbbb9b2a9c6e.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/e721fc3aea40-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B057](branches/b057-stash-alias-09031c14e05c.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/e348490bce40-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B058](branches/b058-stash-alias-373bb2fc05d3.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/f349f6398aa7-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B059](branches/b059-stash-alias-ebc2e2faed24.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/3db8bf2ee028-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B060](branches/b060-stash-alias-15ca898cb2c6.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/4ac430a548ca-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B061](branches/b061-stash-alias-cf15510f83af.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/4e8bc3e16a54-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B062](branches/b062-stash-alias-dca0343dfb05.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/07ad558976dc-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B063](branches/b063-stash-alias-54f51006e189.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/7b2538888f01-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B064](branches/b064-stash-alias-abb1dffe6447.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/8e79df941bcc-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B065](branches/b065-stash-alias-0e6388966c3a.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/18a078e0fd97-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B066](branches/b066-stash-alias-b1eced4af625.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/31ad21a631b3-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B067](branches/b067-stash-alias-31cbf60dff61.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/41aec636bd17-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B068](branches/b068-stash-alias-929b5577bb5f.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/51e08660dd2e-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B069](branches/b069-stash-alias-8836a20a65fb.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/596d8e54fd97-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B070](branches/b070-stash-alias-f574e4820623.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/4456e9b83763-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B071](branches/b071-stash-alias-42c120365146.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/24255e91bdc8-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B072](branches/b072-stash-alias-64003e4d826d.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/187535a508ae-80ab98038e` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B073](branches/b073-stash-alias-2cc727bc102c.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/650748ab814e-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B074](branches/b074-stash-alias-bc491d591458.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/stash/jackin-project/jackin/75703094d1b6-b1b9078173` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B075](branches/b075-stash-alias-770a8c6e1ecf.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/90e8831cbed7f0348cb01ab4/rust-policy-rollout-stash-80ab980` | `80ab98038e945ec48c82dad241ebf33aa0efa82d` |
| [B076](branches/b076-stash-alias-8e34450f0fb7.md) | `refs/heads/rescue/local/20261011-110605-42b7822b/90e8831cbed7f0348cb01ab4/rust-policy-rollout-stash-b1b9078` | `b1b90781739333ba7c8829c092a845f6ea82b11c` |
| [B077](branches/b077-policy-517d17ddf7fc.md) | `refs/heads/rust-policy-rollout` | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| [B078](branches/b078-wip-6b8347c05938.md) | `refs/heads/wip/mise-lock-snapshot-20261007-ef40fea11a54` | `ef40fea11a54ade7911de7622e6e011707241dd4` |
| [B079](branches/b079-wip-ebf572949c76.md) | `refs/heads/wip/reflog-snapshot-20261007-d04467f05f11` | `d04467f05f1183008322047b1828c201e551e947` |
