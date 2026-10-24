# Post-cutoff Jackin integration-base review

This is a source inspection overlay, not a change to the external-product release cutoff of **2026-10-24T01:15:00Z**. The original audit used Jackin `6e266406b49da2ab3c55355f3b7d33d1643ee6cd`. While the research PR was in progress, origin/main advanced to **`c98f5f5fdd831e6075e38fde93b9713575406140`**; the coordinator requested a compatibility review before a normal merge. No production source was edited by this review.

## Scope and exact provenance

[main-overlay-source-register.json](main-overlay-source-register.json) contains hashes for thirteen selected source files from the new commit. [main-overlay-comparison.json](main-overlay-comparison.json) compares all twenty-six files from the original source audit through `git show` at the exact commits: **26 unchanged, 0 changed**. Consequently, the twelve-agent registry, six entrypoint selection cases, status-hook lists, UID/GID SessionIdentity, current MCP behavior and token-monitor conclusions retain their original source basis. This is a byte comparison, not a repeated host or capsule launch.

The review read the new `secure_install.rs` public API and implementation, absolute-root and Linux ACL modules; the Darwin ACL FFI/getter/enumeration path and permission-mask policy; container-root preparation changes; marker-pair acquisition/descriptor operations; the transaction publication, Prepared/Committed recovery and strict-classifier paths; transaction schema validation; and the precommit receipt/checked-install composition and CLI. It did not claim a line-by-line audit of every new test or all unrelated console/UI changes. No Rust build, platform ACL runtime, transaction fault injection, native host or paid call ran in this overlay review.

## Decision-relevant changes

| Owner | New source behavior | Implication and limit |
|---|---|---|
| Process-directory checked installation | Retains directory/file descriptors, rejects traversal and symlinks, validates type/owner/mode/link count, checks native ACL policy and rechecks name/inode identity | Future artifact installation can reuse a checked-read primitive. It does not authenticate the source, build or executable, and it does not retain every ancestor name forever. |
| Absolute-root acquisition | Walks each component from the OS root through descriptors; final root is effective-user owned with exact mode 0700 | More precise than canonicalizing one string. Linux has a documented default-ACL exception for root-owned ancestors when the effective user is non-root; unsupported inspection fails. |
| Instance role preparation | Unix preparation opens a non-symlink root, checks effective ownership and no group/other write, retains device/inode and rechecks around preparation/auth work | Preserve the root guard when adding worker state. Its non-Unix branch only creates directories; do not generalize Unix protections to all platforms. |
| Account marker pair | Nonblocking cooperative shared/exclusive flock on independently opened root descriptors, root identity checks and descriptor-relative operations | Readers and publishers participate in one trusted-root protocol. It assumes cooperating writers and does not defeat a hostile owner of the same UID namespace. |
| Marker transaction | Durable Prepared/Committed records, staged file identities, exact byte/digest checks, generation fence and bounded lock handoff | Prepared recovery accepts only classified expected states; foreign inodes, third values or inconsistent residues fail. Commit rename/sync uncertainty does not trigger blind rollback or overwrite newer/foreign state. |
| Precommit receipt inspector | Strict format3 receipt parsing, bounded descriptor hashing, checked fixed artifact names and repeated consistency checks | A non-activating inspection result carries no execution authority. The CLI explicitly excludes source closure, build authority, running-image identity and execution trust. |

Linux's checked-install policy rejects extended access ACLs and applicable default ACLs rather than trying to interpret every allow/deny combination. Darwin inspects a bounded extended ACL and rejects unknown or mutating permission grants. Neither policy was executed on the relevant platform in this review. See the pinned module sources below for their exact scope and error behavior.

The account-marker code is a useful existing example of acknowledged state transitions and conservative recovery, but it is not an observation archive and cannot be reused as a model-exposure ledger merely because both use persistence. New observation recovery still needs its own admitted consumer/worktree/context-epoch identity, exact-original contract, retention leases and host delivery evidence.

## Required preservation during integration

1. Preserve new root/descriptor/ACL checks and cooperative marker transaction ownership. Do not add a worker installer that rewrites account markers or bypasses an existing preparation error.
2. Treat foreign or uncertain transaction state as a failure requiring the established recovery protocol; do not restore saved bytes indiscriminately over current state.
3. Do not convert a format3 hash-consistency result into an executable trust or supply-chain attestation claim.
4. Keep the optimizer's transform-failure fallback limited to the already-captured authorized observation. Installation, authorization, integrity or mandatory redaction failures never gain a raw bypass.
5. Keep post-cutoff Jackin integration facts distinct from frozen external-product releases and from unexecuted optimizer qualification.

## Immutable source anchors

- [Checked installation API and implementation](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/core/jackin-process-directory/src/secure_install.rs)
- [Absolute root acquisition](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/core/jackin-process-directory/src/secure_install/absolute_root.rs)
- [Linux ACL inspection](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/core/jackin-process-directory/src/secure_install/linux_acl.rs)
- [Darwin ACL inspection](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/core/jackin-process-directory/src/secure_install/darwin_acl.rs)
- [Instance preparation](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/services/jackin-instance-roles/src/prepare.rs)
- [Account marker lease](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/services/jackin-runtime-launch-account-identity/src/account_identity/marker_pair.rs)
- [Account marker transaction](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/services/jackin-runtime-launch-account-identity/src/account_identity/marker_transaction.rs)
- [Transaction record validation](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/services/jackin-runtime-launch-account-identity/src/account_identity/marker_transaction/record.rs)
- [Non-activating checked receipt composition](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/tools/jackin-xtask/src/precommit_owner_security/checked_install.rs)
- [Receipt inspector CLI](https://github.com/jackin-project/jackin/blob/c98f5f5fdd831e6075e38fde93b9713575406140/crates/tools/jackin-xtask/src/precommit_owner_main.rs)
