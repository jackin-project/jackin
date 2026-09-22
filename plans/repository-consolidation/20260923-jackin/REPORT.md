# Repository provenance repair

Recorded 2026-09-23 against the PR's historical `main` base at
`55e21f05bdee37a1424464ce69b86fc5e550307a`.

## Result

This is a documentation-only provenance record. Published merge history is not
rewritten. The companion `PROVENANCE.json` restores auditable attribution,
trailers, pull-request identity, scope, and verification for the seven landed
squash merges covered here.

Each source commit is the immutable GitHub PR head recorded for a landed PR.
Scope is the GitHub PR base-to-source diff. The squash parent is the actual
first parent of the published merge commit, which can differ from the PR base
when another PR landed first.

Verification below is anchored to immutable source-series commits, not to the
mutable head of this PR. `PROVENANCE.json` records the audit target, its parent,
and the historical PR base so later documentation commits cannot change what
was tested.

## Explicit mappings

| PR | Source head | Squash merge | Published merge parent | Scope |
|---|---|---|---|---|
| [#1077](https://github.com/jackin-project/jackin/pull/1077) | `d19b0602e0b2e6145720d2d06b54758c01fe5429` | `fd14ac6a7e0642842e66eeeb31d4b589d07148e1` | `866dd90ae316604078b14f9e547f1deee39bdd0b` | 2 files, +11/-12 |
| [#1078](https://github.com/jackin-project/jackin/pull/1078) | `69087d5aaa99d8b10d401ea3896fd2321a544d18` | `d6ab0f90af5a1e744ae845b8c7172cac5180a09f` | `fd14ac6a7e0642842e66eeeb31d4b589d07148e1` | 13 files, +33876/-1 |
| [#1080](https://github.com/jackin-project/jackin/pull/1080) | `b516991d914d23f6de170c8fb6bb3111d2e9315e` | `b44205ed39b1e40069f79fb9262d8c1647931766` | `d6ab0f90af5a1e744ae845b8c7172cac5180a09f` | 6 files, +121/-6 |
| [#1081](https://github.com/jackin-project/jackin/pull/1081) | `d976e16be5f5402d55215baab3a9b47a047f0c54` | `acad03ea1de2c3120a5b11f065f158376df82294` | `b44205ed39b1e40069f79fb9262d8c1647931766` | 5 files, +26/-4 |
| [#1085](https://github.com/jackin-project/jackin/pull/1085) | `98a63809922696454a816f9a36b83a00518ac22c` | `5ba333ee04b8545fbafb08ac658f281f7b94547f` | `acad03ea1de2c3120a5b11f065f158376df82294` | 1 file, +43/-107 |
| [#1087](https://github.com/jackin-project/jackin/pull/1087) | `15df337ef1e16ca1415df950587b6a34ebf9804a` | `7e5624322d2cfb9ad9cdf0772b8f48418721cc98` | `5ba333ee04b8545fbafb08ac658f281f7b94547f` | 16 files, +848/-280 |
| [#1086](https://github.com/jackin-project/jackin/pull/1086) | `f51c68d8bff51dd9a778bc4f4416af7dce71d14a` | `55e21f05bdee37a1424464ce69b86fc5e550307a` | `7e5624322d2cfb9ad9cdf0772b8f48418721cc98` | 7 files, +573/-47 |

The two source-to-squash repairs are therefore explicit:

- #1087: `15df337ef1e16ca1415df950587b6a34ebf9804a` → squash
  `7e5624322d2cfb9ad9cdf0772b8f48418721cc98`, parent
  `5ba333ee04b8545fbafb08ac658f281f7b94547f`.
- #1086: `f51c68d8bff51dd9a778bc4f4416af7dce71d14a` → squash
  `55e21f05bdee37a1424464ce69b86fc5e550307a`, parent
  `7e5624322d2cfb9ad9cdf0772b8f48418721cc98`.

For #1087 and #1086, GitHub reports the PR base as
`acad03ea1de2c3120a5b11f065f158376df82294`; the later published merge
parents are `5ba333ee...` and `7e562432...` because #1085 and #1087 landed in
between. The JSON records both facts instead of conflating PR base with merge
parent.

## Attribution and evidence

The JSON lists every source commit reported by GitHub, its subject, and its
source trailers. The source commits carry `Co-authored-by: Codex
<codex@openai.com>` and the contributor's `Signed-off-by` trailer. The
published squash commits remain untouched; the mapping is the durable record
that reconnects those source trailers and authorship to each PR and merge.

Repository evidence is the object identity, source-base diff, and exact
one-parent relationship for each squash commit. GitHub evidence is the PR URL,
title, source branch/head, merged state, merge OID, merge time, and passing
required checks (`DCO`, `Policy`, `ci-required`) recorded in the JSON.

The first-parent sequence covered by this report is:

```text
866dd90a… → fd14ac6a… (#1077)
→ d6ab0f90… (#1078)
→ b44205ed… (#1080)
→ acad03ea… (#1081)
→ 5ba333ee… (#1085)
→ 7e562432… (#1087)
→ 55e21f05… (#1086)
```

## Repair verification

The provenance and generated-state checks passed in an isolated worktree. The
exact pinned Velnor generator computes and accepts scan input
`ee4d10ecde1b6a03` for the audited source-series tree; the policy result is 11
rules, 0 failed. The repository link audit remains a carried pre-existing
limitation; this report does not claim that all gates passed:

- JSON parse: `jq empty plans/repository-consolidation/20260923-jackin/PROVENANCE.json` — passed.
- Provenance gates: `cargo xtask roadmap audit` and `cargo xtask research check` — passed.
- Documentation link audit: `cargo xtask docs repo-links` — failed with exit 1
  on the immutable audit target `146720b90b7d0c0e22dcef83eabfd64acbfdea58`,
  its parent `3b65588dad6c976e0a8fa1eca344908373f746ae`, and historical PR base
  `55e21f05bdee37a1424464ce69b86fc5e550307a`, producing the same six
  pre-existing references:
  - `docs/content/reference/getting-oriented/xtasks.mdx:45` — missing `.github/PULL_REQUEST_TEMPLATE.md`.
  - `docs/content/research/context/techniques/02-baseline-audit.mdx:114` — `.github/AGENTS.md` reference is not a verifiable `RepoFile` link.
  - `docs/content/research/context/techniques/06-context-architecture.mdx:52` — `.github/AGENTS.md` reference is not a verifiable `RepoFile` link.
  - `docs/content/research/engineering/ci/performance/ci-performance-analysis.mdx:27` — `.github/AGENTS.md` reference is not a verifiable `RepoFile` link.
  - `docs/content/research/engineering/ci/rust-tooling/index.mdx:171` — `.github/AGENTS.md` reference is not a verifiable `RepoFile` link.
  - `docs/content/research/engineering/ci/rust-tooling/index.mdx:23` — `.github/AGENTS.md` reference is not a verifiable `RepoFile` link.
  These references are unchanged by this PR and are deliberately carried;
  repairing them is outside this generated-state/provenance correction.
- Format sanity: `cargo fmt --all -- --check` — passed.
- Diff sanity: `git diff --check` and a changed-path audit proving the PR is
  limited to the two provenance files and this generated-state file, with no
  source-code changes — passed.
