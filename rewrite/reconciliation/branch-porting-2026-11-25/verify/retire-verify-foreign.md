# Foreign-history refs (resolved by the parent orchestrator, Opus 5.5 session)

| Branch | Expected SHA | Verdict | Evidence |
|---|---|---|---|
| rollout/velnor-3-provider | 092fe1db796367479029d4e94379530f7f5965c0 | SAFE | The owner repo is ChainArgos/jackin-agent-brown (private). Comparing its main...092fe1db gives diverged, ahead=2 behind=1, so 221 of 223 commits are in the owner repo's main (base 3e128bd21 exists there). The 2 unique commits are 67aa16b01 "ci: roll out Velnor 3-provider workflow" and 092fe1db7 "fix(ci): route github-hosted selectors to velnor-scale-set": generated CI for another repo, which plan section 3 excludes. |
| wt/a15-declared-auth-producer | dd2008cbf9e7d3e3af830905e588e3abe663c9dc | SAFE | The owner repo is jackin-project/jackin-sentinel. The tip commit is in sentinel PR #158 (feature/openshell-native, merged 2026-11-12). The plan cites sentinel main fb17d8b6d, and comparing fb17d8b6d...main gives identical. This is not Jackin product code. |
