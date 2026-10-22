# Native preview design security review

Date: 2026-10-18. Scope: proposed configuration and design, plus selected existing Velnor, Jackin, and tap verification paths. Three independent security subagents reviewed configuration, publication/provenance, and tap PR/merge boundaries. This is a design review, not a penetration test or completed implementation audit.

Reviewed documents: [native design](velnor-native-preview-design.md) and [original plan](automatic-homebrew-preview-plan.md). Source baseline: Velnor v0.1.12 / `f1b044a606b6bc469f9718644f5c0ed88bf965b6` and the repository commits recorded in the plan.

## Findings and dispositions

| ID | Priority | Finding | Disposition |
| --- | --- | --- | --- |
| SEC-1 | High design blocker | A formula-only patch can still add executable Ruby while preserving the expected version, URL, hash, and source fields. | Accepted. Require independent canonical literal-only rewriting against captured trusted tap main, preservation of all other bytes, exact blob/tree validation, and rejection of unexpected paths/modes/symlinks/hunks. |
| SEC-2 | High contract inconsistency | Proposed evidence booleans could disable proofs that mandatory validation otherwise requires. | Accepted. Replace booleans with a closed mandatory `signed-sbom-v1` profile; disable/substitution attempts fail configuration validation. |
| SEC-3 | Medium, existing verifier weakness | Jackin's cosign verification allows a repository-level identity rather than the exact authorized signer workflow/ref and source/authority identity. | Accepted. Migrate package/runtime/tap verification to exact channel-specific signer and authenticated receipt/subject identity. |
| SEC-4 | Medium availability/correctness gap | Interruption after creating a tag or draft has no explicit safe recovery rules; naive retries become stuck or could adopt inconsistent assets. | Accepted. Specify a non-overwriting state machine, original authenticated evidence binding, and fail-closed handling of unknown/mismatched states. |
| SEC-5 | Medium hardening | `version_env` only required a literal identifier, allowing dangerous runtime/control environment names if interpreted broadly. | Accepted. Restrict the initial API to application `*_VERSION_OVERRIDE` names, reject control namespaces, and accept only Velnor-computed values. Protected-branch-owned config and generated version values limit the exploit; this alone is not a demonstrated external escalation. |
| SEC-6 | Medium boundary clarification | Qualified updater task identity did not explicitly require the existing static task-projection isolation or exclude hooks/config execution in privileged jobs. | Accepted. Require static projection, disabled hooks/ambient loading/automatic install during execution, clean isolated privileged jobs, and no task/config execution with write credentials. |
| SEC-7 | Required implementation gate | Current Velnor consumer-policy checks and early-return emission inspect stable enablement; preview-only configuration needs explicit policy/dependency validation. | Accepted. Test stable/preview combinations and distribution references before generation. This is a required extension to existing code, not a demonstrated preview vulnerability, because preview does not exist yet. |

The updated native design contains these corrections. “Accepted” means the design requirement was fixed; it does not mean an implementation or its tests have passed.

## Evidence and attack scenarios

### SEC-1: formula semantics

Before correction, the design constrained the formula path and required release metadata to match, but did not require the remaining Ruby to stay unchanged. A compromised updater or crafted patch could add a command to `install` or top-level Ruby while retaining every checked metadata field. Homebrew formula evaluation/installation executes that code. A path-only writer and green superficial tests are insufficient.

The existing tap updater writes the full formula through a heredoc at `scripts/package-update.sh` around lines 168–226. Therefore the writer must compare the complete result to an independently derived canonical formula rather than trust the updater's output. The patch producer is not publication authority.

### SEC-2 and SEC-5: configuration admission

The original example used independently disableable `checksums`, `sbom`, `sigstore`, and `github_attestation` booleans, contradicting the required missing-evidence failure rule. The revised profile defines the complete required proof set. `version_env` likewise now has a narrow explicit grammar and denied control namespaces instead of mere lexical validity.

### SEC-3: authenticated signer policy

`crates/tools/jackin-xtask/src/release_verify.rs` defines `CERTIFICATE_IDENTITY_REGEXP` as `https://github.com/jackin-project/jackin/` and supplies it to `cosign verify-blob`. Its GitHub attestation verification also names the repository without an exact signer restriction. A signature/attestation originating from another workflow in that repository can satisfy those broad checks.

Runtime `crates/adapters/jackin-image/src/capsule_binary.rs::is_allowed_signer_san` accepts release/preview workflow prefixes, without the proposed channel's exact ref and authority binding. All consumers must adopt the same precise policy; tightening only the publisher leaves a weak acceptance path.

The GitHub CLI supports signer workflow/digest and source identity constraints in [attestation verification](https://cli.github.com/manual/gh_attestation_verify). Verify the selected pinned tool version and authenticated claims in executable tests; do not trust receipt fields merely because they contain expected-looking values.

### SEC-4: partial publication

Existing Velnor publication creates a tag before creating a draft. The old single-binary publisher rejects an existing identity. The new idempotent preview feature must deliberately handle tag-only and partial-draft states without moving tags, replacing files, or adopting new bytes under an old receipt. A completed immutable release can safely resume downstream reconciliation only after identity, inventory, hash, and attestation checks.

### SEC-6 and SEC-7: execution and policy admission

Use the existing [named Mise check isolation contract](https://github.com/tailrocks/velnor-new/blob/v0.1.12/docs/implemented/named-mise-checks.md) as the baseline for source-bound task projection. Merely naming a trusted task does not guarantee that unrelated Mise hooks or config-side behavior cannot execute.

Current consumer-only binary-release policy validation is in `crates/velnor-actions-contract/src/config/mod.rs`; current opt-in selection is in `crates/velnor-actions-orchestrator/src/binary_release_emit.rs`. Add preview and distribution checks explicitly; stable-disabled/preview-enabled must work only under the authorized consumer policy.

## Controls already present in the proposal

- Exact-source push CI and workflow authority checks.
- Immutable version tags and permanent release assets.
- Closed product/asset inventories and independent hashing.
- Separation of build, signing, publishing, updater, and PR writer jobs.
- Short-lived target-scoped App tokens without bypass/admin permissions.
- No repository producer/updater code running in the credential-bearing writer.
- SDK/tool qualification requirements.
- Required checks, DCO, thread resolution, and final-head feedback handling.
- Automatic merge blocked until configured review completion and feedback disposition can be proven.

Existing Jackin archive-member verification rejects unexpected/duplicate entries and non-regular members. Preserve that strictness in the native output contract; do not replace it with unrestricted extraction in privileged jobs.

## Required implementation proof

Tests must demonstrate rejection of Ruby code injection inside an otherwise correct formula; unexpected diff/tree metadata; missing/disabled evidence; forbidden version environment names; alternate-workflow/ref signatures; forged source/authority claims; wrong artifact digests; unsafe archive entries; and changed PR heads.

Exercise producer/updater hooks and environment-loading attempts without exposing secrets. Verify privileged jobs use clean environments and cannot invoke Git hooks, filters, repository configuration, or producer-controlled caches.

Exercise interruption at every publication/PR transition, especially tag-only, partial draft, mismatched receipt, unavailable original bytes, stale source, newer tap version, and already-published exact identity. Verify final merge includes the expected head SHA and that a changed head invalidates prior patch/check/review decisions. GitHub's [merge API](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request) documents the expected-head SHA parameter.

No implementation tests, live publication, token mutation, repository settings changes, or PR merge were performed during this security review.
