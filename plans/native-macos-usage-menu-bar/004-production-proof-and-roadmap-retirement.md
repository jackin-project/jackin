# Plan 004: Prove the first production install and retire the completed roadmap item

> **Executor instructions**: This is an evidence and documentation closure plan, not permission to repair release code ad hoc. Run it only after Plan 003 is merged, a real stable release exists, Plan 003's first cask PR passes its independent checks, and the operator explicitly approves and merges that cask. If any proof fails, mark this plan BLOCKED with the exact failure and return to Plan 003 on a normal follow-up commit/new release; never mutate published assets.
>
> **Drift check (run first)**: compare the live roadmap, operator guide, ADR, release-verification guide, native README, GitHub Release assets, and tap cask with Plan 003's contracts. Any mismatch is a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: Plan 003, one real immutable stable release, and the operator-approved merged first cask
- **Category**: tests / docs
- **Planned at**: commit `7a52a273`, 2026-07-21

## Why this matters

Workflow code can be syntactically green while Apple rejects notarization, Gatekeeper rejects the downloaded ZIP, Homebrew installs the wrong bundle, or reruns duplicate publication. The roadmap should be retired only after the public bytes, cask, clean install, both architectures, and recovery behavior are proven. This preserves roadmap truth and turns durable behavior over to the operator guide and ADR.

## Current state

- `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx` explicitly leaves “Notarized CI artifact + Homebrew cask” unchecked.
- `PULL_REQUESTS.md:210-222` requires fully resolved roadmap pages to be deleted in the same PR, with durable operator details moved to guides, architecture to references, overview/sidebar links updated, and docs audits passing.
- The current roadmap has inbound links from the operator guide, roadmap overview, and operator-surface `meta.json`; all must be repointed or removed during retirement.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Download | `gh release download v<VERSION> --repo jackin-project/jackin --pattern 'jackin-usage-menu-bar-<VERSION>-universal-apple-darwin.zip*' --dir <clean-dir>` | ZIP and sidecars downloaded |
| Supply chain | `cargo xtask release-verify <clean-dir>/jackin-usage-menu-bar-<VERSION>-universal-apple-darwin.zip` | all checks pass |
| Cask | `brew install --cask jackin-project/tap/jackin-usage-menu-bar` | installs public release app |
| Docs | `cargo xtask roadmap audit && cargo xtask docs repo-links && cargo xtask research check` | exit 0 |
| Docs site | from `docs/`: `bun run build && bunx tsc --noEmit && bun test` | all pass |

## Scope

**In scope:** production evidence collection in GitHub/tap checks; `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx`; `docs/content/docs/(public)/getting-started/verifying-releases.mdx` only if evidence reveals a gap; `docs/content/docs/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx`; roadmap page, overview, and operator-surface metadata; `native/README.md`; plan index/status.

**Out of scope:** changing release bytes, moving/deleting tags, force-pushing, bypassing tap checks, source implementation fixes inside this evidence PR, Sparkle, DMG/PKG, or deleting user credential/state directories on uninstall.

## Steps

### Step 1: Verify public release bytes after upload

From a clean directory, download the ZIP and sidecars from the stable GitHub Release. Run `cargo xtask release-verify`; extract and run the shared release-mode app verifier; independently require Developer ID fingerprint/Team ID, secure timestamp, hardened runtime, no forbidden entitlements, accepted staple, Gatekeeper `Notarized Developer ID`, both architectures, macOS 14 floor, exact bundle/version metadata, static linkage, and no packaged FFI framework/library.

Record links to the release, successful workflow run, attestation, and notary diagnostic artifact in a sanitized `production-proof.txt` uploaded as a short-retention artifact by the evidence run and summarized in the closure PR. The file must contain exact release/tag/SHA, runner architecture and macOS version, asset ID/digest, cask commit, each command's exit status, and public run URLs; redact usernames, home paths, signing subject details beyond approved Team ID/fingerprint, and every credential/provider response. Do not put transient run links in long-lived operator docs.

**Verify**: every command passes against downloaded public bytes. A checksum computed locally equals the release sidecar and cask SHA.

### Step 2: Verify clean Homebrew lifecycle and runtime coverage

On a clean supported macOS account/machine with no provider credentials, install through the public cask, confirm the installed app is the same signed/notarized/versioned bundle, launch and verify it remains alive while presenting an honest unavailable state rather than crashing, then quit and uninstall the cask. Verify the app is removed and no root daemon/LaunchDaemon was installed. Provider-login behavior is already owned by shipped host-runtime fixtures and is deliberately excluded from this distribution proof so no operator credential enters CI, transcripts, or a clean test account.

Run the launch proof on both Intel and Apple Silicon macOS 14+ if the operator selected that acceptance policy. Architecture inspection alone does not satisfy a required runtime proof.

**Verify**: tap required checks pass for every approved architecture; the sanitized evidence artifact records exact commands/statuses and contains no output matching the repository/tap credential-scan patterns; `brew uninstall --cask` removes the app.

### Step 3: Prove release reconciliation without changing immutable bytes

Rerun the release workflow for the completed version through the approved manual path. It must detect complete release/formula/cask state, make no release upload or tap commit, and exit successfully. Exercise tap-only repair using a safe fixture/dry-run or a deliberately prepared non-production test state; do not damage the real cask. Confirm conflicting-asset fixtures still fail closed.

**Verify**: the production rerun reports no writes; release asset IDs/checksums and tap commit remain unchanged before versus after.

### Step 4: Move durable truth and retire the roadmap page

Re-read the roadmap top to bottom and confirm no residual remains. Ensure the operator guide owns installation/use/uninstall/privacy behavior, ADR-011 owns static XCFramework/signing/notarization architecture, release-verification docs own sidecar verification, and `native/README.md` owns contributor build/release details.

Delete `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`; remove its entry from the operator-surface `meta.json`; replace the roadmap overview's planned/partial bullet with one Completed bullet linking the canonical operator guide and ADR; remove/repoint the guide's roadmap link. Run `rg "roadmap/native-macos-usage-menu-bar" docs/` and require no matches.

**Verify**: all xtask/docs-site commands in the command table pass. The roadmap sidebar/overview audits report no orphan or stale entry.

### Step 5: Close the planning program

Mark Plans 001–004 DONE, then follow the root `plans/README.md` convention: after source/docs audit confirms no unfinished residual, remove this shipped plan directory and its active-program row in the same or a dedicated planning-cleanup commit as the operator prefers. Code, docs, public release, tap history, and git history become the source of truth.

**Verify**: `git status --short` contains only intentional docs/plan changes; no production source or published artifact changed during closure.

## Done criteria

- [ ] Public downloaded ZIP passes supply-chain, Developer ID, notarization, staple, Gatekeeper, architecture, deployment, plist, and static-linkage verification.
- [ ] Homebrew clean install/launch/uninstall succeeds on every operator-required architecture with no root daemon.
- [ ] Completed-version release rerun makes no writes; partial/conflict fixtures retain correct repair/fail-closed behavior.
- [ ] Durable operator/contributor/architecture/verification docs contain all shipped truth.
- [ ] Roadmap page and sidebar entry are removed; overview has one Completed canonical-doc bullet; no stale inbound link remains.
- [ ] Full docs and roadmap audits pass.

## Execution status (honest)

- **BLOCKED** — depends on plan 003 shipping a real notarized release ZIP + operator-approved first cask merge. No public notarized menu-bar asset or stable cask exists yet.

## STOP conditions

- Any public checksum, signature identity, notarization/staple, architecture, version, or cask value disagrees.
- One required hardware architecture has not executed successfully.
- Release rerun changes or attempts to clobber an existing asset.
- The roadmap still contains a genuine unfinished item after production proof.
- Fixing evidence requires source/workflow changes. Stop and stay on an existing in-scope open PR branch; if none exists, return to `main`, propose a new policy-compliant `fix/` or `chore/` branch, and wait for operator confirmation. Publish a new version if bytes must change, then rerun this plan; never revive or push a merged/deleted branch implicitly.

## Maintenance notes

Future releases should make Plan 004's checks routine release evidence, not a repeated roadmap project. Sparkle remains optional; cask updates are the accepted update channel. Reviewers should guard the invariant that notarized public bytes are immutable and that roadmap retirement follows real production proof, not merely merged YAML.
