# Security & Code Review Findings

## Scope
Rust codebase review with security focus, including runtime orchestration, repo validation, workspace/mount handling, and persistence behavior.

---

## High-priority risks

### 1) Unpinned remote install script in build
In `src/derived_image.rs`, the derived Dockerfile includes:

- `RUN curl -fsSL https://claude.ai/install.sh | bash`

**Risk**
- Supply-chain compromise risk.
- Non-reproducible builds due to remote script drift.

**Recommendation**
- Pin by checksum/signature, or vendor the installer.
- Download to a file and verify SHA256 before execution.
- Prefer explicit version pinning in config/manifest.

---

### 2) ~~Git source trust is broad for first clone~~ ✅ Resolved
Trust-on-first-use model implemented. Untrusted third-party agents now require explicit operator confirmation before building. Built-in agents are always trusted. Trust state is persisted in config.

---

### 3) ~~Potential symlink escape in manifest Dockerfile path resolution~~ ✅ Resolved
`resolve_manifest_dockerfile_path()` now canonicalizes the joined path and enforces `canonical.starts_with(repo_canonical)`. Symlink components are rejected with a dedicated test.

---

## Medium-priority issues

### 4) ~~Config file permissions for secrets-adjacent state~~ ✅ Resolved
`config.save()` now uses `0o600` permissions on Unix with atomic write (temp file + `sync_all()` + rename).

---

### 5) Cleanup logic relies on string-matched Docker errors
`is_missing_cleanup_error()` checks text like `"No such container"` / `"No such network"`.

**Risk**
- Brittle across Docker versions/locales and message changes.

**Recommendation**
- Prefer status-code-based cleanup tolerance.
- Reduce dependence on exact stderr text matching.

---

### 6) ~~No timeout control for command execution~~ ✅ Resolved
`ShellRunner` now has `capture_timeout: Option<Duration>` with a 120s default and `wait_with_timeout()` that kills stalled processes.

---

### 7) ~~Silent config-save failures on last-agent persistence~~ ✅ Resolved
Both `last_agent` persistence sites now emit `eprintln!("warning: ...")` on save failure.

---

## Lower-priority hardening opportunities

### 8) Privileged DinD sidecar
Current design uses privileged `docker:dind` sidecar (already documented clearly).

**Recommendation**
- Roadmap toward rootless DinD or alternative build/runtime isolation.
- Add optional stricter network policy modes.

---

### 9) Mount policy guardrails
Mount validation is structurally good.

**Recommendation**
- Add optional deny/warn rules for sensitive host paths:
  - `~/.ssh`
  - `~/.aws`
  - `~/.gnupg`
  - etc.

---

### 10) Reproducibility/provenance improvements
Current repo flow tracks moving branches by default.

**Recommendation**
- Support lockfile-like pinning to commit SHAs.
- Display selected commit SHA in runtime config output.
- Introduce explicit `--update` behavior.

---

## Positive observations

- Strong selector validation and namespacing behavior.
- Good mount/workspace conflict checks.
- Rejection of symlinks in derived build context copy is a strong defensive choice.
- Security model documentation is realistic and avoids over-claiming isolation.
- Good test coverage around runtime orchestration paths.

---

## Suggested implementation order

1. ~~Fix Dockerfile path canonicalization/symlink boundary checks.~~ ✅
2. Pin/verify remote installer script. (see `SECURITY_EXCEPTIONS.md`)
3. ~~Add git trust controls~~ ✅ + optional commit pinning. (see `todo/reproducibility-pinning.md`)
4. ~~Harden file permissions and atomic writes for config/state.~~ ✅
5. ~~Add command timeout support~~ ✅ and robust cleanup handling. (see `todo/bollard-migration.md`)
