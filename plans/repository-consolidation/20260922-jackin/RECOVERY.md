# Recovery record

Recovery root: `/Users/donbeave/.codex-chainargos2/jackin-recovery-20260922-rzwau1`.

Captured before cleanup. The initial snapshot predates the non-pruning fetch; the landed snapshot also captures the final product state after #1077.

- `refs.bundle`: initial bundle snapshot. It is complete history, but Git's default bundle ref selection omitted many remote/PR refs; do not use it alone as the complete ref backup.
- `refs-complete.bundle`: explicit bundle of every ref returned by `git for-each-ref` after fetch and consolidation-worktree creation.
- `refs-final-20260922.bundle`: final explicit all-ref bundle, rebuilt after the audit commit and before cleanup.
- `git-admin.tar.gz`: self-contained Git administrative directory/object store snapshot.
- `show-ref.txt`, `show-ref-after-fetch.txt`, `all-refs-after-fetch.tsv`, `reflog.txt`, `fsck-unreachable.txt`, `worktrees-before.txt`, and `canonical-status.txt`.
- `reflog-commits.txt`: unique reflog commit IDs. The Git version lacks `git bundle create --stdin`; reflog-only object preservation therefore relies on the self-contained Git admin archive plus the explicit ID list.
- `dirty-worktrees/`: exact base, index, status, staged/unstaged binary patches, and file manifests for all seven dirty live worktrees.
- `standalone-clones/*-final.bundle`: exact current refs for both independent Jackin clones; each was bundle-verified and cloned into an isolated bare repository for `git fsck`.
- `*-precleanup-landed-20260922.*`: refreshed refs, remote heads, PR list, worktree states, and operation snapshots after #1077 landed at `fd14ac6a7e0642842e66eeeb31d4b589d07148e1`.

Verification status:

- `git bundle verify refs.bundle`: passed for the initial bundle; incomplete ref-name coverage is recorded above.
- `git bundle verify refs-complete.bundle`: passed; explicit all-ref bundle, SHA-1 object format.
- `git fsck --full --unreachable --no-reflogs`: completed; 69 unreachable commits, 393 trees, and 248 blobs recorded.
- `git-admin.tar.gz` and bundle SHA-256 values are recorded in the recovery directory's checksum capture.
- The complete bundle cloned into an isolated bare repository and passed `git fsck --full`; dangling objects are expected from preserved historical tips.
- The final bundle is verified with `git bundle verify`, cloned into an isolated bare repository, and checked with `git fsck --full`. Final hashes are recorded in the recovery-root checksum file; the repository copy records the stable artifact names and prior hashes without embedding a self-referential final-bundle hash.

Recovery must be retained after cleanup. Restoration starts by extracting `git-admin.tar.gz` into an isolated directory and using the extracted `.git` as the repository administrative directory; verify with `git fsck`, `git bundle verify`, and the recorded ref/object IDs before reconnecting a checkout.
