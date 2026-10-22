# Native Velnor preview release and Homebrew promotion design

Date: 2026-10-18. Status: researched proposal, not implemented configuration.

Security review: three independent reviews completed. Accepted findings and remaining implementation proof are recorded in [the security review](velnor-native-preview-security-review.md). The requirements below incorporate their design corrections; no runtime implementation has been security-qualified yet.

This design refines [the original implementation plan](automatic-homebrew-preview-plan.md). The requested solution is a standard Velnor feature. Handwritten or preserved workflows are outside scope.

## Recommendation

Extend the existing Rust `binary_release` feature with an independently enabled `preview` policy and typed multi-product package outputs. Normalize stable and preview publication into shared artifact, identity, provenance, and immutable-release machinery. Keep their trigger, version, and environment authorization policies distinct.

Add a separate optional distribution feature for updating a Homebrew tap through a PR. It references the verified binary preview release; Homebrew repository and formula details do not belong in Cargo analysis or release-plz configuration.

Use existing named Mise checks for tap validation. Do not create a third release engine, arbitrary privileged workflow task, new general validation framework, or Velnor-wide automated code-review system as part of the release feature.

## Comparison with current release features

Current source and latest release match at Velnor `f1b044a606b6bc469f9718644f5c0ed88bf965b6`, version 0.1.12.

| Concern | Rust `release` today | Rust `binary_release` today | Proposed preview extension |
| --- | --- | --- | --- |
| Product | Cargo crates published through release-plz | One selected executable | Declared executable packages and companion assets |
| Selection | Explicit packages or publishable workspace | One package and one bin | Existing primary identity plus explicit package/output inventory |
| Version | Cargo package version, coordinated by release-plz | Cargo package version | Cargo base version plus exact-source commit count and SHA suffix |
| Trigger | Default-branch push and plan-bound manual dispatch | Manual dispatch only | Default-branch push; optional bounded retry by manual dispatch |
| Version PR | Same-repository release-plz preparation PR | None | No source version bump PR; downstream formula update PR |
| Destination | crates.io | Immutable GitHub release | Immutable GitHub prerelease, then optional tap PR |
| Target set | Cargo registry package selection | Apple Silicon macOS only | Explicit multi-target payload inventory |
| Authorization | Protected publishing environment and OIDC/bootstrap modes | Protected environment with required human reviewer | Separate protected default-branch preview environment allowing automatic publication |
| Shared proof | Source selection, validation, pinned tools | Exact-SHA Required CI, split build/attest/publish, immutability | Reuse these proofs; add preview version, package set, retry and downstream freshness rules |

The `release_pr` field is specifically release-plz's same-repository version preparation. It must not be repurposed to mean a Homebrew PR. Crates.io OIDC authentication also does not replace the GitHub App installation credentials needed to update a second repository.

Some configuration fields exceed current implementation: for example, `version_groups` exists in the crate-release schema but `release_emit.rs` currently rejects nonempty values. Configuration shape alone is not proof of working release behavior.

## Alternatives considered

1. **Add preview to `[stacks.rust.release]`.** Reject. Preview binaries do not require a crates.io upload or a Cargo version-bump PR. This mixes registry semantics with executable distribution.
2. **Add a sibling `binary_preview` or `binary_package_release` engine.** Reject as the default design. It duplicates source selection, package validation, receipt generation, attestation, and immutable publishing. A separate generated preview workflow is appropriate; a duplicate publication implementation is not.
3. **Extend `binary_release` with typed preview policy and common publication machinery.** Recommend. Preserve stable behavior while sharing the implementation and output contracts.
4. **Create an unrestricted workflow or promotion DSL.** Reject. The demonstrated requirement needs a typed producer, closed output inventory, immutable publisher, and bounded PR destination adapter, not arbitrary YAML, commands, permissions, or GitHub events.

## Proposed producer configuration

All fields introduced below are a proposed API. Velnor 0.1.12 will reject the new sections. The existing parent fields remain meaningful for the current stable/manual binary release; `preview.enabled` is independent of its `enabled` field. Generation must check both policies independently, rather than returning early when stable is disabled.

```toml
[stacks.rust.binary_release]
enabled = false
manifest_path = "Cargo.toml"
package = "jackin"
bin = "jackin"

[stacks.rust.binary_release.preview]
enabled = true
versioning = "package-preview-commit-count-sha7"
version_env = "JACKIN_VERSION_OVERRIDE"
environment = "preview-publish"

[stacks.rust.binary_release.preview.producer]
task = "release:preview-package"
source = { mise_config = "mise.toml", working_directory = "." }
output_directory = "dist/preview"
manifest = "dist/preview/package-output.json"

[[stacks.rust.binary_release.preview.products]]
id = "jackin"
package = "jackin"
bins = ["jackin", "jackin-role", "jackin-usage-broker"]
targets = [
  "aarch64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
]
archive = "tar.gz"

[[stacks.rust.binary_release.preview.products]]
id = "jackin-capsule"
package = "jackin-capsule"
bins = ["jackin-capsule"]
targets = ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"]
archive = "tar.gz"

[stacks.rust.binary_release.preview.evidence]
profile = "signed-sbom-v1"
```

The TOML demonstrates the policy and package declarations, not a complete frozen schema. Implementation must also define exact companion-asset declarations, producer tool/input closure, bounded runner/build placement, and timeout/resource limits by reusing the existing typed task concepts. These cannot be inferred from an untrusted output manifest or delegated to an arbitrary publishing script.

The producer is a repository-owned Mise task, analogous to Velnor's existing named task integrations. Velnor owns its source qualification, tool/input identities, placement, resource bounds, output validation, and publication. The task owns domain-specific archive composition and generation of application metadata. It runs without publication credentials or OIDC.

The expected output set must be derivable from the declared products plus explicit companion metadata and evidence policy. For this project it is exactly four CLI payloads, two capsule payloads, the capsule manifest, and the existing required supporting assets. Velnor rejects extra/missing assets, duplicate target identities, symlinks, path escapes, invalid executable layouts, and mismatched source/version data. The producer's `package-output.json` is untrusted build inventory, not publication authority. Velnor independently hashes and validates it before generating the canonical public release receipt.

`version_env` is a validated literal application build variable name; Velnor supplies its value. For this initial API, require `^[A-Z][A-Z0-9_]*_VERSION_OVERRIDE$` and reject runtime/control namespaces including `GITHUB_`, `ACTIONS_`, `RUNNER_`, `VELNOR_`, `MISE_`, `CARGO_`, `RUST_`, `BASH_`, `LD_`, and `DYLD_`. Never evaluate its value as shell or accept a config-supplied value. It is not arbitrary environment injection, an executable version expression, or a secret reference. The producer receives the exact source SHA and computed preview version through a fixed handoff contract. Default branch, version suffix syntax, immutable behavior, tag `v<version>`, prerelease status, CI workflow identity, and token permissions should be native rules, not user-provided shell/templates or safety-disable switches.

The closed `signed-sbom-v1` evidence profile requires checksums, SBOMs, Sigstore signatures, and GitHub artifact attestations. Required identity/integrity proofs cannot be disabled through individual booleans, missing fields, or an unknown profile. Add negative configuration tests for attempts to disable or substitute these proofs.

Normalize the stable publisher's one selected bin/target into the same internal output model. Preview can initially be the multi-product surface without forcing an unnecessary stable configuration migration. If implementation chooses to replace parent `package`/`bin` with a common inventory, migrate all consumers and tests completely; do not retain compatibility aliases or parallel renderers.

## Proposed distribution configuration

Keep this outside `stacks.rust`:

```toml
[distribution.homebrew_tap]
enabled = true
source = "rust.binary_release.preview"
repository = "jackin-project/homebrew-tap"
formula = "Formula/jackin-preview.rb"
update_task = "package:update-preview"
environment = "tap-update"
app_id_variable = "HOMEBREW_TAP_APP_ID"
app_private_key_secret = "HOMEBREW_TAP_APP_PRIVATE_KEY"
merge = "squash-after-gates"
```

These fields are also a proposal. The update task must be declared in trusted tap-main Mise configuration, qualified with its tracked source, inputs, pinned tools, and allowed output paths. Implement that typed updater binding as part of the distribution contract; a task name alone is not sufficient authorization to modify arbitrary files. Execute producer/updater tasks through the existing static task-projection isolation model: disable Mise hooks, ambient config/environment loading, and automatic tool installation during task execution, with tools provisioned through the qualified setup path. Do not load tap or producer Mise configuration in a signer, publisher, writer, or merge job. The example deliberately omits configurable permission strings or arbitrary merge conditions: the adapter supplies fixed minimum permissions and validates its closed merge mode.

The generated workflow checks out a captured tap-main SHA without persisted credentials, downloads and validates the immutable release, and invokes the trusted updater without a write token. It records the prepared patch against that SHA. Validate that only the configured formula path changes and that the formula's version, URLs, resources, hashes, and source identity match the verified release. Preserve existing install logic; do not use blind Ruby regex replacement.

Path restriction and matching release values are insufficient: Ruby formulae are executable code. The isolated writer must independently construct the canonical expected formula from the captured trusted base using a non-evaluating parser and an exact allowlist of literal metadata replacements. All other bytes must remain identical to the trusted base, including install/test logic. Reject ambiguous syntax, extra expressions or statements, unexpected hunks, other paths, renames/copies, mode changes, symlinks, submodules, and Git metadata. Compare the prepared result with that canonical result before obtaining write credentials; revalidate the exact blob/tree identity before commit. Never evaluate the Ruby formula to validate it in the privileged boundary. If safe deterministic rewriting cannot be proven, stop promotion rather than accept a path-only patch.

A subsequent isolated writer job obtains a short-lived App installation token scoped to the tap with `contents:write` and `pull_requests:write`, applies only the validated patch, signs off the commit, and opens or reconciles the update PR. It does not execute the updater, package binaries, or PR scripts. If tap main advanced incompatibly, regenerate the patch from a new trusted base rather than applying an unchecked stale diff.

Signer, publisher, writer, and merge jobs use clean ephemeral execution environments and qualified tools, without producer-controlled caches, hooks, executable configuration, environment-file handoffs, or persisted checkout credentials. The writer should use Git object/tree APIs for the validated blob or an equally constrained Git invocation that cannot run repository hooks or smudge/clean filters. Resolve API destinations and GitHub-host release URLs from validated repository/tag identity; do not let build inventory select arbitrary download or API hosts.

App credentials are references to repository/environment configuration, never values in TOML. The App must be installed only on the intended target repository and have no ruleset bypass. Use the App ID variable and private-key secret to mint an installation token at runtime; do not configure a long-lived installation-token secret.

Create separate main-only GitHub environments in the source repository: `preview-publish` contains the immutable-release status read credential; `tap-update` contains the tap App private key and App ID variable. Only the isolated PR writer/merge boundary references `tap-update`. The producer, updater, signer, and release publisher must not reference that environment. Automatic preview/promotion environments have no human-review requirement; exact-source CI and the separate PR feedback gates remain mandatory. Stable publication retains its required reviewer environment. Environment names/policies are runtime-validated prerequisites, not resources that workflow generation silently creates.

## Direct promotion versus tap workflow dispatch

Two native implementations are possible:

- **Producer-side PR promotion, recommended initially:** generated preview workflow adds isolated tap preparation and PR-writer jobs after immutable publication. It requires only tap contents/PR capabilities and naturally binds promotion to the verified release receipt.
- **Tap-side generated update workflow:** producer dispatches the tap's standard update workflow using `workflow_dispatch`; tap independently verifies the immutable receipt and opens the PR. This gives the tap ownership of scheduling/retries but needs an additional dispatch credential boundary and Actions-write permission on the target repository.

Both must be Velnor-generated. Choose the first for the initial consumer unless tap-owned retries or additional producers demonstrate the second is necessary. Do not use `repository_dispatch`, `workflow_run`, or PR-target triggers: these do not fit Velnor's current generated-event policy.

## Generated workflow structure

Generate a separate preview workflow alongside existing CI and optional stable binary-release workflow. Internally its jobs are:

1. Capture exact default-branch source and workflow authority; require that SHA's successful push CI run and Required job in the same run attempt.
2. Build/package with the qualified producer, complete history for the version counter, and no release/tap/OIDC credentials.
3. Validate the closed output inventory, generate/validate SBOM evidence, sign/attest without executing repository producer code under the signer identity.
4. Recheck CI/current source, verify preview environment and immutable-release prerequisites, then publish the complete immutable prerelease.
5. Prepare the tap formula patch from trusted tap main without write credentials.
6. Create/reconcile its DCO-signed-off PR with the restricted App token.
7. Request/perform squash merge only after required checks and final-head feedback gates pass.

Reuse existing `[[checks]]` in the tap to execute updater tests and formula/capsule installation tests through Required. No new tap validation framework is needed. A separate merge-readiness workflow versus a job/controller remains an implementation choice; do not assume Velnor already provides one.

The required feedback gate must not treat a quiet period or an empty list as proof that expected reviewers have finished. If configured automated reviewers are expected, integrate a trusted exact-head completion signal. PRs with unaddressed findings remain open for evidence-backed disposition. This is a merge prerequisite distinct from building/publishing preview artifacts; do not add arbitrary AI review infrastructure to the release engine.

Bind final validation and merge to the exact PR head SHA, target repository/base branch, expected automation branch/author, canonical formula blob, and source receipt. Re-fetch all checks and feedback immediately before a merge request that includes the expected head SHA; a changed head invalidates the decision. Do not enable an asynchronous merge operation that can merge a replacement head without revalidating the canonical patch and feedback. Feedback can still arrive after a completed review; the guarantee is a final-head snapshot after configured review completion, not a claim to prevent future comments.

## Signer identity and immutable retry rules

Existing Jackin package verification accepts a broad repository-level cosign identity, and runtime capsule verification accepts release/preview workflow prefixes. These checks must migrate with the new workflow. For previews, require the exact generated preview signer workflow, authorized ref, expected OIDC issuer, authenticated workflow authority commit/digest, source SHA, and artifact subject digest. The expected values come from the trusted workflow/source policy, not self-declared producer manifest fields. Verify the canonical public receipt itself as well as each required artifact. Reject signatures from another workflow or ref in the same repository and reject an authentic signature over a different source/asset set. Preserve a separate exact stable-channel signer policy where needed; do not widen identities to make migration pass.

Publication uses an explicit non-overwriting state machine:

| Observed identity | Permitted retry behavior |
| --- | --- |
| No tag or release | Recheck eligibility, create tag at exact candidate SHA, then create draft. |
| Tag only | Resume draft creation only if the tag resolves exactly to candidate SHA and all currently verified evidence/receipt and eligibility checks pass; never move the tag. |
| Matching draft | Verify draft tag/source identity, the original authenticated receipt/run identity where already present, and every existing asset against retained authenticated original artifact digests. Upload only missing declared assets whose exact bytes/evidence remain available. Never replace existing assets. |
| Matching immutable published release | Verify immutable status, exact authenticated receipt, complete asset set, hashes, and provenance; resume only downstream PR reconciliation. |
| Conflict, unknown draft origin, unavailable original evidence, or inconsistent partial state | Stop without deleting/moving a tag, clobbering an asset, or publishing the draft. Require explicit recovery after inspection. |

The canonical receipt binds release channel, source and workflow identity, package set, version, artifact names/digests, and signing/build run identity where required. A retry cannot adopt a different build's changed bytes merely because its source/version matches. Cover interruption after tag creation, draft creation, each upload, publication, and PR creation with executable tests. Stale-source policy still governs unfinished publication; a matching completed immutable release may be inspected without authorizing an obsolete formula downgrade.

## Concrete implementation changes

### Velnor

- `velnor-actions-contract`: add preview/product/producer/evidence/distribution types with closed serde validation; no arbitrary jobs, scripts, event names, permissions, or shell version expressions.
- `velnor-actions-rust`: resolve package/bin/feature selection, workspace membership, package-version identity, and expected executable products. Do not require crates.io publishability for binary products; Jackin correctly uses `publish = false`.
- `velnor-actions-mise`: resolve exact producer/updater task sources, lockfiles, tool closures, and fixed invocation handoff without exposing secrets to repository tasks.
- `velnor-actions-orchestrator`: independently select stable/preview policies, derive a shared typed release plan, compose product placement, environment/identity gates, and optional distribution plan. Generation should not query live release or remote tap contents; runtime performs those checks.
- `velnor-actions-workflow-renderer`: factor current immutable publisher, source eligibility, and evidence helpers; emit the preview and distribution DAG with typed channel-specific rules.
- CLI/plan reporting: describe selected products, channel, computed runtime version policy, required setup, generated workflows, and destination. Existing `generate` remains the public entry point; no mandatory new CLI command is needed.
- Replace the incompatible receipt contracts with one Velnor-owned typed artifact-set receipt, provisionally `velnor.package-release.v2`. Current Velnor `release.json` describes one binary, whereas Jackin's strict private `velnor.package-release.v1` describes six payloads and supporting assets. The latter name is not evidence of an already implemented Velnor package-release API. Migrate receipt writers, strict verifiers, tap updater, and fixtures together; do not publish old/new compatibility receipts or retain a fallback parser. Preserve stable authorization semantics while updating any stable consumers affected by the common receipt migration.
- Add generation, executable publisher, permissions, partial-retry, concurrency, stale-source, invalid-output, and PR lifecycle tests. Stable render/output parity must be demonstrated where stable behavior is unchanged.

### Jackin

- Add a real preview producer task and fixed output contract around existing archive tools. There is no current standalone preview producer ready to wire into the new API.
- Refactor `release_archive.rs`: it currently calls `cosign sign-blob` in its build/archive loop. Split build/package from signing/attestation so building repository code never receives signer OIDC permissions. SBOM generation can remain unprivileged where practical, with its output independently verified before signing.
- Pin/verify the macOS cross-compilation SDK or qualify an appropriate native runner route. Add complete producer tool pins, including Zig/cargo-zigbuild where used.
- Keep `JACKIN_VERSION_OVERRIDE` as the runtime version binding; the existing build helper already accepts it. No new Jackin version environment variable is needed.
- Update capsule, manifest, sidecar, signature, help-link and documentation resolution to immutable `v<preview-version>` releases, with no rolling fallback.
- Update generator pin/config and regenerate only after upstream support is released.

### Homebrew tap

- Expose the existing preview updater as a qualified Mise task accepting verified release identity.
- Replace literal rolling-tag validation with canonical immutable-version-tag validation and preserve full source provenance checks.
- Add generated named checks for updater behavior and actual formula/capsule installation, keeping stable formula/Cask coverage.
- Keep the existing `jackin@preview` alias and formula installation interface; do not add compatibility paths.
- Configure required checks and exact-head merge/review completion policy before enabling automatic merges.

## Mandatory validation rules

- Preview defaults off; stable reviewer requirements cannot be disabled through preview configuration. Consumer-only policy checks and emitters must evaluate stable/preview independently; test all four enablement combinations and reject distribution references to missing or disabled channels.
- Default-branch source is exact, append-only, full-history, and successfully checked; deterministic reruns cannot mint a different version.
- Cargo packages/bins/targets must exist and be supported. All selected products must agree with the release version contract.
- Producer/updater sources are tracked and path-safe; outputs are regular files with explicit allowed names and inventory.
- Missing evidence, invalid signer/workflow identity, mismatched versions/hashes/architectures, or extra assets fail closed.
- Publishing uses immutable version tags and permanent assets; no replacement or rolling mutation.
- Repeated releases only reuse exact verified existing identities; interrupted drafts have defined non-overwriting recovery behavior.
- Distribution requires a configured enabled source channel and verifies its receipt before PR mutation.
- Older or unrelated-source formula updates cannot overwrite a newer tap formula; identical identities reconcile idempotently.
- App tokens are minted only in the final writer/merge boundary; producer, signer, updater, and PR tests never receive them.
- Merge requires final-head checks and feedback disposition, without bypassing existing protections.

## Source anchors

- [Current Rust crate release configuration](https://github.com/tailrocks/velnor-new/blob/v0.1.12/crates/velnor-actions-contract/src/config/release.rs)
- [Current crate-release orchestration](https://github.com/tailrocks/velnor-new/blob/v0.1.12/crates/velnor-actions-orchestrator/src/release_emit.rs)
- [Current binary-release configuration](https://github.com/tailrocks/velnor-new/blob/v0.1.12/crates/velnor-actions-contract/src/config/binary_release.rs)
- [Current binary-release orchestration](https://github.com/tailrocks/velnor-new/blob/v0.1.12/crates/velnor-actions-orchestrator/src/binary_release_emit.rs)
- [Current binary-release renderer](https://github.com/tailrocks/velnor-new/blob/v0.1.12/crates/velnor-actions-workflow-renderer/src/schema2_consumer_binary_release.rs)
- [Current binary-release authorization and publisher](https://github.com/tailrocks/velnor-new/blob/v0.1.12/crates/velnor-actions-workflow-renderer/src/schema2_consumer_binary_release_scripts.rs)
- [Current native binary-release contract](https://github.com/tailrocks/velnor-new/blob/v0.1.12/docs/proposed/binary-release-contract.md)
- [Existing named Mise checks](https://github.com/tailrocks/velnor-new/blob/v0.1.12/docs/implemented/named-mise-checks.md)
- [Current workflow event policy](https://github.com/tailrocks/velnor-new/blob/v0.1.12/crates/velnor-actions-contract/src/workflow/workflow_policy.rs)
- [Velnor architecture boundaries](https://github.com/tailrocks/velnor-new/blob/v0.1.12/docs/proposed/architecture.md)
