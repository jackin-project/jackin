# Verification record

## Baseline

Baseline target: `origin/main` at `df4671e4d9f2860e90a5c71d8d0bd85b23d23291`, isolated worktree `/private/tmp/jackin-repository-consolidation-20260922`.
Current integration target: `origin/main` at `fd14ac6a7e0642842e66eeeb31d4b589d07148e1` after #1071–#1077.

Authoritative commands are derived from `TESTING.md`, `CONTRIBUTING.md`, `.github/ci/project.toml`, and the generated CI workflows. Baseline command results will be recorded here with tool versions, environment, and whether failures are pre-existing or introduced.

## Integration gates

For each accepted logical change: focused tests plus affected reverse-dependency checks. At integration boundaries: formatting, repository lint/Clippy contract, relevant unit/integration tests, docs/roadmap gates, and CI evidence. Final state also requires clean/synchronized `main`, repeat discovery, recovery integrity, and cleanup-manifest reconciliation.

## Results

### Fresh `origin/main` baseline

- `cargo fmt --all -- --check`: passed.
- `cargo xtask ci --fast`: completed with exit failure because fresh `origin/main` already carries two failing gates:
  - strict lint ratchets/container-path allowlists/test-layout budgets report existing drift across production and test files;
  - docs repo-link validation reports one missing `.github/PULL_REQUEST_TEMPLATE.md` reference and five plain-text `.github/AGENTS.md` references that must use `RepoFile` links.
- No consolidation change was present in this baseline worktree when the command ran. Treat these as carried failures, not evidence against a later patch.

### Recovery proof

- Initial and explicit all-ref bundles verified as complete histories; a final all-ref bundle will be rebuilt after the audit records are committed.
- Explicit all-ref bundle cloned into an isolated bare repository; `git fsck --full --no-progress` reported dangling historical commits only, with no missing-object error.
- `git-admin.tar.gz` passed `gzip -t`, extracted into an isolated directory, and its Git object store passed `git fsck --full --no-progress` with dangling historical objects only.
- Seven dirty live worktrees have exact patch/index/status captures under the recovery root.

Per-batch verification remains pending until candidate changes receive disposition.

### Accepted batches

- #1066: upstream required checks passed at exact head `85a7bd39a0d466b692af39f08ceb2f1fa267b240`; merged as `e50c6e9ba3930a67c93e8b85b3d2233f00dfc5d7`.
- #1067: documentation-only checks passed; merged as `d53b1a517e2752b74109ebe3df817a565c7d5152`.
- #1071: local fmt, 25 focused preview tests, 12 package tests, 364 full `jackin-xtask` tests, clippy, and diff checks passed. Exact head `3ad15ba3f0ade7907effa42952cea59c068de405` passed 43 protected checks and merged as `0f5eff7869751e3f03f77f04cd9956b48e8e127b`.
- #1072: local focused console suite passed (5 tests), fmt/clippy/diff checks passed, then all protected required checks passed at exact head `110ea3a385aecfab49cd438955feeb3833e2745c`; merged as `7f8c7036d5c72231aa7d576f0a2e19cf047a51bf`.
- #1073: local fmt, 578 `jackin-usage` library tests, focused money/status regressions, clippy, and diff checks passed; merged exact head `a9ba4568e853a68a2e8b1a7d10700d7681f006fd` as `113786e5aa58b5357f6fd60d18df5dc1a0c4aea9`.
- #1074: local 19 capsule tests, clippy, fmt, and diff checks passed; exact head `f25a6fa22ee5646b40c507b0c4c83b03249d7195` passed protected checks and merged as `e3415dada382723f4ce116277576208cf8dd5177`.
- #1076: local focused launch tests, clippy, fmt, and diff checks passed; exact head `67579b0685a2e0381c74a132afb07a88d1a02ea8` passed required checks and merged as `de046d3345bb2ec44f9749655cd29164b6e4e4c9`.
- #1075: local capsule/usage tests, clippy, fmt, and diff checks passed. The first artifact upload failed with intermediary `403 Forbidden`; rerunning failed jobs completed successfully, exact head `e90a37980249d4031d4d7155d697d6ebcb2a321b` passed required checks, and merged as `866dd90ae316604078b14f9e547f1deee39bdd0b`.
- #1077: local fmt, strict-lint unsorted-iteration gate, 364 `jackin-xtask` tests, clippy, and diff checks passed. The exact head `d19b0602e0b2e6145720d2d06b54758c01fe5429` passed all protected checks, including the delayed Swift package job, and merged as `fd14ac6a7e0642842e66eeeb31d4b589d07148e1`.

### Final verification notes

- The #1077 strict-lint run cleared all five unsorted filesystem-iteration findings. Remaining strict-lint failures are existing container-path, telemetry, ratchet, and test-layout drift; docs repo-link failures are also pre-existing.
- #1063 was rejected wholesale; its justified capsule, Keychain, and cleanup adaptations are landed separately.

### Rejected dirty launch-security source

- Preserved dirty patch `launch-security-repair` was not applied. Against `origin/main` `0f5eff7869751e3f03f77f04cd9956b48e8e127b`, it failed formatting and runtime checks, broke the current `DockerApi`/`NoOpDocker` contract, retained name-based container resolution, lacked identity/race tests, and mixed unrelated account-identity edits. Docker tests (66) and Docker clippy passed; runtime failures were real. Recovery patch is retained unchanged.
