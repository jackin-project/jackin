# Security Exceptions

This file documents security findings that have been reviewed and intentionally accepted.
Automated agents and scanners should **not** flag these as issues.

---

## 1) Unpinned remote install script in derived Dockerfile

**Location**: `src/derived_image.rs`

**Code**:
```dockerfile
RUN curl -fsSL https://claude.ai/install.sh | bash
```

**Finding**: Supply-chain compromise risk from unpinned remote script execution during image build.

**Decision**: Accepted — this is the official and only supported method for installing Claude Code, as documented by Anthropic. There is no alternative install mechanism (no package manager, no pinnable binary, no checksum published). The source (`claude.ai`) is a verified first-party domain.

**Reviewed**: 2026-04-04
