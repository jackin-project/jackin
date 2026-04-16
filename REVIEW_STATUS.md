# Review Status

Last verified: 2026-04-17

This file is the single source of truth for review follow-up in this repository.
It consolidates the former `PROJECT_REVIEW.md`, `RUST_REVIEW_FINDINGS.md`,
`SECURITY_REVIEW_FINDINGS.md`, and `SECURITY_EXCEPTIONS.md` files.

Keep only unresolved findings and accepted exceptions here. When an item is
fixed, remove it from this file instead of keeping a stale "resolved" entry.
Git history remains the long-term record for what was reviewed and when.

## Active Findings

### High

1. Launcher preview omits selector-scoped global mounts.

   The interactive launcher preview still shows workspace mounts plus only
   unscoped global mounts. The real launch path resolves selector-scoped global
   mounts too, so the operator can approve a launch from an incomplete access
   picture.

2. Documentation drift remains significant.

   Core docs still disagree with the implementation in important ways,
   including version requirements, docs stack references, build-caching
   behavior, config schema coverage, runtime behavior, and some security-model
   details.

3. Runtime startup still triggers `gh auth login` automatically.

   The runtime entrypoint starts a GitHub CLI login flow when `gh` is present
   but unauthenticated. Public docs mostly frame GitHub authentication as a
   persistence behavior after the operator chooses to authenticate, not as a
   default startup interaction.

### Medium

4. The orchestration core is still too centralized.

   `runtime.rs`, `lib.rs`, `config.rs`, `workspace.rs`, and `launch.rs` remain
   large multi-concern modules. Routine changes still require tracing several
   responsibilities through the same functions.

5. Docker command construction still hides policy intent.

   Image-build, DinD-launch, and agent-launch commands are still assembled as
   large positional argument vectors. That makes mount, env, label, and network
   policy harder to audit than it should be.

6. Build-caching docs still overstate what is skipped.

   Loads still create a derived build context and run `docker build` each time.
   Docker layer cache helps, but the build step is not actually skipped on
   subsequent loads.

7. Mount config can still persist invalid or ambiguous state.

   Global and scoped mounts still share one key space, and write-time config
   updates do not validate the full persisted mount shape before saving.

8. Claude-specific implementation still conflicts with runtime-agnostic
   roadmap language.

   The current implementation is explicitly Claude-focused across manifest
   schema, image build, entrypoint behavior, and version probing, while some
   roadmap language still presents the architecture as runtime-agnostic today.

9. CI still uses `cargo test --locked` while contributor guidance requires
   `cargo nextest run`.

   Local guidance, AGENTS guidance, and CI are still not aligned on the test
   runner policy.

10. Cleanup tolerance still string-matches Docker CLI stderr.

    Missing-resource cleanup is still detected by checking stderr text such as
    `No such container`, `No such volume`, and `No such network`.

11. Command execution timeouts are currently absent.

    Timeout handling that earlier security notes marked as resolved is no
    longer present in the current `ShellRunner` implementation. Long-running or
    stalled subprocesses currently rely on normal process completion.

12. Reproducibility and provenance are still branch-moving by default.

    Agent repos still default to moving git branches rather than pinned commits,
    and the current operator experience does not expose strong provenance or
    update controls.

## Accepted Exceptions

### Unpinned remote install script in derived Dockerfile

Location: `src/derived_image.rs`

```dockerfile
RUN curl -fsSL https://claude.ai/install.sh | bash
```

Accepted because this is currently the official and only supported Claude Code
installation path. There is no pinned package artifact or published checksum to
verify instead, and the installer is fetched from Anthropic's first-party
domain.

Reviewed: 2026-04-04

## Resolved Review Cleanup

The previously documented findings around `ShellRunner::capture()` pipe
deadlocks, `jackin-validate` drift, agent-vs-DinD role labels, `last_agent`
persistence on failure, prompt I/O error flattening, trust-on-first-use,
symlink boundary checks, config file permissions, and sensitive mount
confirmation were re-verified against the current codebase on 2026-04-17 and
removed from this file.
