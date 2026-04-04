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

### 2) Git source trust is broad for first clone
`resolve_agent_source()` auto-constructs GitHub URL from namespace/name and clones directly.

**Risk**
- Typosquatting / untrusted repo execution in build context.

**Recommendation**
- Add optional allowlist for namespaces/orgs.
- Require explicit confirmation for first-time non-builtin agents.
- Support commit/tag pinning in config (`git + rev`).

---

### 3) Potential symlink escape in manifest Dockerfile path resolution
`resolve_manifest_dockerfile_path()` blocks `..` components but does not canonicalize the final joined path before file check.

**Risk**
- A symlinked path component could resolve outside repo boundaries.

**Recommendation**
- Canonicalize joined path and enforce `canonical.starts_with(repo_canonical)`.
- Reject symlink components for Dockerfile path resolution.

---

## Medium-priority issues

### 4) Config file permissions for secrets-adjacent state
`config.save()` relies on default file permissions.Make

**Risk**
- Config may be too permissive under certain umask setups.

**Recommendation**
- On Unix, write with `0o600` using atomic write/rename.
- Consider same policy for sensitive persisted runtime state.

---

### 5) Cleanup logic relies on string-matched Docker errors
`is_missing_cleanup_error()` checks text like `"No such container"` / `"No such network"`.

**Risk**
- Brittle across Docker versions/locales and message changes.

**Recommendation**
- Prefer status-code-based cleanup tolerance.
- Reduce dependence on exact stderr text matching.

---

### 6) No timeout control for command execution
`ShellRunner` uses blocking process calls (`status`/`output`) without timeout.

**Risk**
- CLI can hang indefinitely on stalled git/docker/network commands.

**Recommendation**
- Add command timeouts (global default + per-operation override).

---

### 7) Silent config-save failures on last-agent persistence
After load, `last_agent` persistence ignores save errors (`let _ = config.save(...)`).

**Risk**
- Hidden state inconsistency / degraded UX with no signal.

**Recommendation**
- Emit warning on persistence failure.
- Optionally make this behavior explicit under debug mode.

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

1. Fix Dockerfile path canonicalization/symlink boundary checks.
2. Pin/verify remote installer script.
3. Add git trust controls + optional commit pinning.
4. Harden file permissions and atomic writes for config/state.
5. Add command timeout support and robust cleanup handling.
