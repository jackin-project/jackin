# R14 private recovery

- Target: `jackin-project/jackin`
- Baseline: `main` at `310e644832193232344ca99838ccf599f28faf53`
- Source: `/private/tmp/jackin-ci-audit.wOxRJU/jackin`
- Source filesystem identity: `16777233:708869982`
- Source Git directory identity: `16777233:708869983`
- Source state: no `HEAD`, no `packed-refs`, no loose refs, no nested Git store, and no regular worktree files. Two dangling symlinks pointed to the absent `AGENTS.md` (`CLAUDE.md` and `.github/CLAUDE.md`). The source index and config are preserved below.
- Source object store: 50 pack files, 48 promisor markers. The source was a partial clone; only the audited private range is retained here.

## Audited range

Range `d6ab0f90af5a1e744ae845b8c7172cac5180a09f..b516991d914d23f6de170c8fb6bb3111d2e9315e` contains exactly 3 commits, 11 trees, and 7 blobs. The thin bundle records `d6ab0f90af5a1e744ae845b8c7172cac5180a09f` as its prerequisite. The prerequisite is present in the keeper.

Commits:

- `0e089acd5d0974b3b04b7983e0200da07da24ad6` — `ci: disable release mise auto-install`
- `ae142400397fa4df0a1f9d6252c81680e930fd44` — `docs: record current CI reliability evidence`
- `b516991d914d23f6de170c8fb6bb3111d2e9315e` — `chore(ci): regenerate release policy state on current main`

The complete object list and keeper presence check are in `range-object-manifest.tsv`. Objects already present in the keeper are marked `ALREADY_PRESENT_IN_KEEPER_OBJECT_STORE`; the 3 commits, 4 trees, and 1 blob absent from the keeper are retained in the bundle.

## Recovery artifacts

All data artifacts are private, outside the worktree, and mode `0600`. The executable verifier `verify-r14.sh` is mode `0700`; this directory is mode `0700`.

| Artifact | SHA-256 | Purpose |
| --- | --- | --- |
| `r14-private.bundle` | `c3fd319d9e98e20cac7cf114ccd0f972166e76cee15ff4c151b120c20d3f9dfe` | Thin Git bundle for the exact three-commit range. |
| `r14-range.patch` | `bb6a4e03c62c4a8d1d0386e88d015c5c9af7930a0275964693b94f791cc671f0` | Binary-capable patch for the exact three commits. |
| `source-index` | `e68e2f898f43d038e0c6dd694ca4e45beb484587ec054b2c83cbb6cd2904a351` | Exact source index bytes. |
| `source-config` | `6f54ac1361e08a6e0c360cebd1fa63fef66c49ebb23d1be5c4906c30ad32bb2e` | Exact source Git config; contains no embedded credential. |
| `range-object-manifest.tsv` | `b3148899da26503941520cfde3ad81b86e8956a1fadde31bdb45c91688f82eae` | Object types, paths, keeper presence, and retention disposition. |
| `source-state.tsv` | `2a55d881ca0d3fe3abb5cac5eeb8269c0b0386f2bc5bdb0f5040889aa5b54b6a` | Source identities, modes, hashes, and missing-admin-state evidence. |
| `parent-siblings-before.tsv` | `a7705e7bc4edf18970b60e0bd9797bd7eb3d06f5db68810a9802a8ae8a51105e` | Pre-delete identity, Git head, and status digest for sibling directories. |
| `verify-r14.sh` | `311af5a7f64f4e79c002c3bc645d7f95da4bc6bd35bcf21bb09ffb5bf0b8fc9e` | Re-runs bundle, object, patch, tree, and connectivity verification. |

## Verification

The bundle passed `git bundle verify` against the keeper. A fresh temporary Git repository fetched the prerequisite from the keeper, fetched `r14-private.bundle`, resolved the recovered ref at `b516991d914d23f6de170c8fb6bb3111d2e9315e`, and passed strict connectivity verification. Applying `r14-range.patch` to the prerequisite succeeded; the resulting tree exactly matched the bundle tip tree `63d37fc4f806ba210a6ce7c48a728488e0481c01`.

No remote refs were changed. Deletion authorization is limited to the exact source leaf after the final identity, lock, process, nested-store, dependency, and sibling-preservation checks.
