# Immutable preview publication and migration

Status: implementation and live publication proof remain in progress. This document records the selected Jackin contract and rollout acceptance; it does not claim that a preview has been published or installed.

This contract supersedes earlier preview tag and retry proposals in [the automatic Homebrew preview plan](automatic-homebrew-preview-plan.md) and [the native Velnor preview design](velnor-native-preview-design.md). Keep those documents as design history. Their proposed `v<preview-version>` release tag and any rolling-tag replacement behavior do not apply to the selected implementation.

## Identity and publication rules

Each eligible `main` source commit gets one immutable prerelease, located by the exact tag `preview-<full40-lowercase-source-sha>`. The existing Homebrew version remains `<package-version>-preview.<main-commit-count>+<source-sha7>`. The shorter version suffix remains useful for package managers; the full source SHA is the release identity.

The tag is an annotated Git tag bound to the signed canonical release receipt envelope. The receipt binds the same full source commit, workflow and run identity, version, product inventory, asset names, sizes, and SHA-256 digests. Preview assets retain the existing unversioned filenames because each tag has its own immutable release. Never move a tag, overwrite or delete a published asset, or adopt bytes from another run because source or version text matches.

Retries may resume only the exact source and authenticated release identity. They must verify the existing annotated tag, signed receipt envelope, immutable prerelease state, and complete asset bytes. A conflicting tag, receipt, run, or asset is a hard failure requiring explicit recovery. Stable publication remains manual, keeps `v<version>` tags, and does not change as part of this preview migration.

The historical rolling `preview` release and its assets are preserved as legacy published data. Migration does not move or overwrite that tag, remove its assets, or rewrite an existing formula to URLs that do not exist. New native binaries and the tap formula move to immutable source-bound releases only after the first new release is independently verified.

## Runtime and tap binding

Native capsule downloads and formula resources use the same full-SHA release tag. Native verification checks the signed receipt envelope, tag binding, selected payload and companion, signatures, SBOMs, hashes, provenance, and full declared asset inventory before installing or executing bytes. A version override alone does not authorize remote downloads. Stable download behavior stays unchanged.

The preview formula retains version `P-preview.N+sha7`, and its six archive URLs use the corresponding `preview-<full40-source-sha>` tag. Formula update tooling accepts only a receipt whose full source SHA, tag, version, authenticated assets, and formula metadata agree. It preserves the Ruby install logic and rejects the old rolling `/download/preview/` URL.

## Workflow lint tool migration

The released SDK owns [`.github/jactionlint.yaml`](../../.github/jactionlint.yaml). Local CI and staged hooks use Jactionlint 2.3.0 and pinned ShellCheck with the SDK's strict offline flags. They check workflows, composite actions, and Dependabot YAML through an explicit sorted file inventory. The previous Actionlint compatibility-ignore bridge is removed. Tool pins, lock records, generated configuration, hooks, and CI assertions change together; a Git revert of the migration restores the previous paired configuration without altering published releases.

## CI eligibility and dependency seeding

Dependency seeding uses the independent `workflow.dependency_seed_timeout_minutes = 30` budget. Crate jobs keep their existing ten-minute limit. The seed job runs serial whole-workspace test, Clippy, and documentation lanes, so it does not inherit a single-crate timeout. The longer seed budget does not weaken release admission: preview publication still requires the latest exact-source main CI run and its Required verdict to succeed.

## Rollout acceptance

Complete and retain evidence for each item before calling publication or installation verified:

1. Generate and inspect the Velnor workflows from the pinned released SDK. Confirm exact successful `main` CI source selection, qualified binary production for supported targets, separated build/sign/publish permissions, and the immutable tag and receipt-envelope contract.
2. Publish a real preview from an eligible `main` commit. Verify the annotated tag resolves to that full source SHA, its signed receipt envelope names the same tag and source, all declared assets match authenticated digests and provenance, and the GitHub release is immutable.
3. Exercise an exact retry and a conflicting retry. The exact retry must preserve tag and asset bytes; the conflict must stop without mutation. Verify the historical rolling `preview` release and its assets remain intact.
4. Update the tap formula only from the verified new release. Check the full source tag in every URL, package version, payload hashes, formula source identity, and unchanged stable formula. Complete the required tap checks and exact-head feedback gates.
5. On macOS, install `jackin-preview` from the tap and prove the installed command and bundled capsule came from the published binaries. Record `brew info`, installed version, binary architecture, and source/release identity. Do not use `cargo build` or `cargo install` for this proof.

Until these checks pass against actual generated workflows, a live signed release, the updated tap, and the macOS installation, configuration, unit tests, and generated workflow files are not proof of end-to-end publication.
