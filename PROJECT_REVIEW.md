# Full Project Review

Review date: 2026-04-14

Scope: end-to-end engineering review of the `jackin` project, with primary focus on the Rust crate in this repository (`jackin/`) and cross-checks against project documentation, UX, testing, and contributor experience.

## 1. Executive Summary

### Overall quality

The project has a clear and worthwhile core idea: run AI coding agents with full autonomy inside an operator-controlled Docker boundary, while separating environment definition (agent class) from file visibility (workspace). That product model is coherent, and the implementation mostly matches it for the current Claude-first proof-of-concept scope.

### Top strengths

- Clean product abstraction: agent class vs workspace split is the right design
- Thorough input validation (path traversal, symlinks, repo boundaries, env var cycles)
- Strong security posture (`unsafe_code = "forbid"`, `serde(deny_unknown_fields)`, TLS on DinD)
- Excellent test discipline: 317 tests, all passing, with good behavioral coverage
- phosphor-themed UX is polished and cohesive
- Honest security documentation in the well-maintained parts

### Top risks

- Documentation drift is severe enough to mislead contributors and users
- The orchestration core (`runtime.rs` at 3163 lines) is becoming a maintenance bottleneck
- Claude-specific assumptions are deeply embedded despite roadmap claims of runtime-agnosticism
- Launcher preview shows incomplete mount sets (missing scoped mounts)
- `gh auth login` runs automatically at container startup, contradicting docs that present it as optional
- Global mount configuration can persist invalid or ambiguous state

### Does implementation match project goals?

Yes, for the current Claude-focused proof-of-concept.
No, for the broader runtime-agnostic narrative:

- The code is not meaningfully runtime-agnostic yet
- The docs overstate build-skip behavior
- Some operator-facing docs no longer match the implementation

## 2. Major Findings

### Finding 1: Launcher preview omits scoped global mounts

- Area: UX / Security model
- Severity: High
- What is wrong:
  The launcher preview only includes workspace mounts plus unscoped global mounts.
  Code: `src/launch.rs:140-152`, `src/launch.rs:491-497`

  Actual launch resolution includes selector-scoped mounts as well.
  Code: `src/workspace.rs:402-404`, `src/config.rs:161-193`

- Why it matters:
  This undermines the project's central value proposition of explicit operator control over file visibility. The operator can approve a launch based on an incomplete access picture.

- Recommendation:
  Resolve the preview through the same `resolve_load_workspace` path, using the currently highlighted agent selector. Add a regression test asserting that the launcher preview mount set equals the final resolved mount set for the selected workspace and agent.

### Finding 2: Documentation drift is pervasive

- Area: Documentation consistency
- Severity: High
- What is wrong:
  Several core docs are stale in important ways:

  - `PROJECT_STRUCTURE.md:9` says MSRV 1.87 — `Cargo.toml:5` requires 1.94
  - `PROJECT_STRUCTURE.md:53` says `jackin.toml` — code uses `jackin.agent.toml`
  - `PROJECT_STRUCTURE.md:65-111` references Astro/Starlight docs engine — docs use Vocs
  - Installation docs say Rust 1.87 and show version `0.1.0`
  - Config command docs omit trust management entirely
  - Configuration reference omits `[agents]` section with `git`/`trusted` fields
  - Comparison docs still describe DinD as unauthenticated TCP — TLS was added in PR #44

- Why it matters:
  Documentation cannot be treated as a source of truth. This hurts new contributors, security reviewers, and users equally.

- Recommendation:
  Treat doc drift as a release-quality issue. Sweep all docs against current implementation. Add CI checks for Rust version, docs engine references, command reference completeness, and config schema coverage.

### Finding 3: Automatic GitHub auth at startup contradicts docs

- Area: UX / Security
- Severity: High
- What is wrong:
  `docker/runtime/entrypoint.sh:27-29` runs `gh auth login` if `gh` is installed and unauthenticated. Docs (`concepts.mdx`, `security-model.mdx`) present this as opt-in persistence.

- Why it matters:
  First-run UX becomes an unexpected auth flow. Non-GitHub or offline tasks pay startup friction. The security model shifts from "credentials persist if you log in" to "the runtime will ask you to log in unless already authenticated."

- Recommendation:
  Only run `gh auth setup-git` when already authenticated, or gate `gh auth login` behind explicit operator opt-in. Document the final policy clearly.

### Finding 4: The orchestration core is too centralized

- Area: Architecture / Readability
- Severity: High
- What is wrong:
  The operational core is concentrated in a few oversized modules/functions:

  - `src/runtime.rs` is 3163 lines
  - `src/lib.rs` is 1412 lines
  - `src/config.rs` is 1260 lines
  - `src/workspace.rs` is 1015 lines
  - `src/launch.rs` is 855 lines

  `load_agent_with` requires tracking: GC, git identity, host identity, intro animation, source resolution, trust gate, repo clone, container naming, state preparation, logo display, config summary, env var resolution, build policy, image construction, network creation, DinD launch, TLS verification, agent launch, and cleanup — all in one function flow.

- Why it matters:
  Maintenance cost is growing faster than feature surface. Every change requires reasoning about multiple concerns at once. This is exactly how subtle regressions and documentation drift accumulate.

- Recommendation:
  Decompose `runtime.rs` into: `build.rs` (image building), `orchestrate.rs` (network/DinD/launch), `gc.rs` (cleanup), and keep `runtime.rs` as a thin coordinator. Split `lib.rs` `run()` into per-command handler functions.

### Finding 5: Docker command construction hides intent

- Area: Readability
- Severity: Medium
- What is wrong:
  `runtime.rs:536-555` and `581-659` construct Docker commands as large `Vec<&str>` sequences interleaved with conditional logic. Understanding what's being launched requires mentally reconstructing the full `docker run` invocation.

- Why it matters:
  Policy decisions (what gets mounted, what env vars are set) are obscured by shell-level syntax.

- Recommendation:
  Introduce typed structs (`DindLaunchSpec`, `AgentLaunchSpec`) that render to CLI args in one place.

### Finding 6: Build caching narrative is inaccurate

- Area: Performance / Docs consistency
- Severity: Medium
- What is wrong:
  Docs claim subsequent loads skip the build step. Implementation (`runtime.rs:780-800`) always creates a derived build context and runs `docker build`. The `JACKIN_CACHE_BUST` arg is only added with `--rebuild`, so Docker layer cache helps, but the orchestration cost is always paid.

- Why it matters:
  Misleading mental model for operators.

- Recommendation:
  Either implement real build fingerprinting or rewrite docs to say "builds rerun but benefit from Docker cache."

### Finding 7: Mount config model is ambiguous

- Area: Architecture / Correctness
- Severity: Medium
- What is wrong:
  `DockerMounts(BTreeMap<String, MountEntry>)` at `config.rs:42` uses one key space for global mount names, wildcard scope keys, and exact selector scope keys. The distinction is only visible through the enum variant. `config mount add` doesn't validate the full mount shape before persisting.

- Why it matters:
  Invalid configuration can be saved and only fail at launch time. The shared keyspace makes the data model harder to reason about.

- Recommendation:
  Separate unscoped and scoped mounts in the data model. Validate at write time.

### Finding 8: Claude-specific code embedded throughout

- Area: Architecture
- Severity: Medium
- What is wrong:
  The roadmap states the architecture is runtime-agnostic. The implementation is deeply Claude-specific:

  - Reserved env names: `src/manifest.rs:6-17`
  - Derived image installs Claude: `src/derived_image.rs:41-48`
  - Image version tracking shells out to `claude --version`: `src/runtime.rs:455-468`
  - Entrypoint installs plugins and launches Claude: `docker/runtime/entrypoint.sh:34-52`

- Why it matters:
  Adding a second runtime (Codex, Amp) would require refactoring several core layers. It is not additive.

- Recommendation:
  Be explicit in docs that the current implementation is Claude-specialized. Before adding support for other runtimes, introduce a `RuntimeKind` abstraction that owns install logic, runtime bootstrap, version probing, and manifest schema extensions.

### Finding 9: CI disagrees with contributor guidance on test runner

- Area: Project hygiene
- Severity: Medium
- What is wrong:
  `TESTING.md` and `AGENTS.md` say "always use `cargo nextest run`" and "do not use `cargo test`." CI workflows still run `cargo test --locked`.

- Why it matters:
  Undermines contributor confidence. If nextest is the standard, CI should enforce it.

- Recommendation:
  Pick one test runner policy and apply consistently across docs, local guidance, CI, and release workflows.

### Finding 10: Review debt documents not folded into main docs

- Area: Project hygiene / Documentation consistency
- Severity: Medium
- What is wrong:
  The repo contains `RUST_REVIEW_FINDINGS.md`, `SECURITY_REVIEW_FINDINGS.md`, and `SECURITY_EXCEPTIONS.md`. These documents correctly note areas of risk and technical debt, and several items have been resolved. But the mainstream docs have not been updated to reflect those resolutions — the review files have become the only accurate record of current behavior in some areas.

- Why it matters:
  Review artifacts are useful as engineering records, but if they become the only accurate source of truth, the normal docs become misleading. A contributor reading the main docs won't know that DinD TLS, config permissions, or symlink boundary checks have been implemented unless they also find and cross-reference the review files.

- Recommendation:
  Treat review files as transient engineering records. Update the main docs first when resolving findings, then keep the review files as rationale/history.

### Finding 11: Architecture docs don't match actual repo cache layout

- Area: Documentation consistency
- Severity: Low
- What is wrong:
  The architecture docs describe a repo cache layout that does not match the actual path structure. The implementation uses `~/.jackin/agents/<namespace>/<name>` (see `src/repo.rs:14-18`), but this layout is not accurately reflected in the architecture reference docs.

- Why it matters:
  Contributors or operators trying to understand or debug the cache will look in the wrong places.

- Recommendation:
  Update the architecture reference to document the actual `~/.jackin/agents/` layout including the namespace subdirectory structure.

### Finding 12: Manifest docs incomplete on reserved env vars

- Area: Documentation consistency
- Severity: Low
- What is wrong:
  The manifest documentation lists `JACKIN_CLAUDE_ENV` and `JACKIN_DIND_HOSTNAME` as reserved runtime env vars. The implementation also reserves `DOCKER_HOST`, `DOCKER_TLS_VERIFY`, and `DOCKER_CERT_PATH` (see `src/manifest.rs:10-17` and `src/runtime.rs:35`). These are silently skipped if declared in a manifest but not documented as reserved.

- Why it matters:
  An agent author declaring `DOCKER_HOST` in their manifest would see it silently ignored at runtime with no explanation.

- Recommendation:
  Document all reserved env vars in the manifest reference, including the Docker TLS variables added with TLS support.

## 3. Readability Review

### What reads well

- **Validation code**: `repo.rs`, `repo_contract.rs`, `selector.rs`, manifest validation logic is clear, well-named, and easy to follow.
- **Test names are descriptive**: `rejects_symlink_escaping_repo_boundary`, `trust_gate_rejects_untrusted_agent_in_non_interactive_context` — tests communicate intent.
- **`docker.rs` is clean**: The `CommandRunner` trait and `ShellRunner` are straightforward. The trait enables good testability.
- **`workspace.rs` validation**: Mount validation, sensitive mount detection, and workspace config validation all read clearly.
- **Small modules**: `paths.rs`, `instance.rs`, `repo_contract.rs`, `terminal_prompter.rs` are focused and easy to understand.

### Worst readability issues

#### `runtime.rs` is the primary offender

Not because the code is incorrect — because too many concerns live in one file. `load_agent_with` and `launch_agent_runtime` mix: cleanup, progress display, network startup, env injection, state mounts, trust gates, build policy, and version probing. The reader must hold too many concepts simultaneously.

**Rewrite target**: Each orchestration step should be a function that takes typed input and returns typed output. The coordinator should be a thin pipeline connecting them.

#### `lib.rs:232-760` `run()` function

Mixes CLI dispatch with business logic. Each match arm contains its own `use` imports, inline struct definitions, and formatting logic. This is a 500-line function that should be decomposed into per-command handlers.

#### `tui.rs:120-329` digital rain animation

200+ lines of inline animation code with magic numbers (`0xDEAD_BEEF_CAFE_1337`, frame counts, color tuples). Fun, but expensive to maintain.

#### `launch.rs:370-573` workspace screen drawing

Another oversized drawing function. Building UI widgets inline with style definitions, layout constraints, and data formatting all interleaved.

#### `#[allow(clippy::too_many_lines)]` as a decomposition signal

This suppression appears 5 times in the codebase. Each instance marks a function that should be decomposed. The linter is right; the suppressions should be eliminated by refactoring, not silencing.

#### Large inline test modules

`config.rs`, `workspace.rs`, `manifest.rs`, and `runtime.rs` have 300-700 lines of tests inline. This makes the production code harder to scan. Move large suites into sibling test modules once those files are split.

## 4. Documentation vs Implementation Mismatches

### Outdated docs

| Doc | Claim | Reality |
|---|---|---|
| `PROJECT_STRUCTURE.md:9` | MSRV 1.87 | `Cargo.toml:5` requires 1.94 |
| `PROJECT_STRUCTURE.md:53` | `jackin.toml` manifest | Code uses `jackin.agent.toml` |
| `PROJECT_STRUCTURE.md:65-111` | Astro/Starlight docs engine | Docs use Vocs |
| Installation docs | Rust 1.87, version 0.1.0 | Rust 1.94, version 0.6.0-dev |
| Comparison docs | DinD is unauthenticated TCP | TLS with auto-generated certs (PR #44) |

### Implemented but undocumented

- `jackin config trust grant/revoke/list` commands
- `[agents]` config section with `git` and `trusted` fields
- Scoped mount resolution precedence (global < wildcard < exact)
- Reserved env vars `DOCKER_HOST`, `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`
- Automatic `gh auth login` at container startup
- Version check against npm for Claude Code updates
- `jackin-validate` binary for CI validation of agent repos
- `version_check.rs` Claude Code update detection and auto-rebuild

### Documented but not implemented or overstated

- "Subsequent loads skip the build step" — builds always run `docker build`
- "Runtime-agnostic architecture" — deeply Claude-specific

### Which side is more reliable?

For current behavior, the implementation is more reliable than the documentation. The most reliable docs are `security-model.mdx` and `commands/load.mdx`. The least reliable are `PROJECT_STRUCTURE.md`, `installation.mdx`, `commands/config.mdx`, and the comparison/workspace auto-detection sections.

## 5. Architecture Review

### What the architecture gets right

- **Product model**: `agent class = environment`, `workspace = visible files`. This is the right split.
- **Validation layering**: Selector -> Repo -> Dockerfile contract -> Manifest -> Workspace. Each layer has clear responsibility and tests.
- **`CommandRunner` trait**: Clean abstraction for shell execution with `FakeRunner` for testing. This is the right pattern.
- **Agent state isolation**: `instance.rs` and `paths.rs` provide clean state management.
- **Config persistence**: Atomic writes with `0o600` permissions on Unix. Proper temp file + rename pattern.

### Overengineering

- Branding/theming polish (Matrix animations, banner art, shimmer effects) is further along than the internal modularity. The presentation layer is ahead of the platform extensibility.

### Underengineering

- No runtime backend abstraction
- No build fingerprinting or image reuse model
- No typed Docker command construction
- Launcher preview doesn't use the same resolution path as actual launch
- Config model doesn't distinguish mount names from scope keys by construction
- Mount validation at write time is incomplete

### Concrete architectural improvements

1. Introduce a typed runtime backend abstraction before adding a second runtime.
2. Move shell-command generation behind typed launch/build specs.
3. Separate "resolve intent" from "execute side effects" in the orchestration layer.
4. Centralize mount resolution into one authoritative path used by both launcher preview and launch execution.
5. Add a build fingerprint model so image reuse is explicit and testable.

## 6. Rust-Specific Review

### Strengths

- `unsafe_code = "forbid"` is enforced and upheld
- `serde(deny_unknown_fields)` on all manifest types prevents silent configuration errors
- Pedantic clippy with thoughtful overrides for CLI-appropriate leniency
- Path canonicalization and symlink rejection in repo validation
- Good use of `thiserror` for `SelectorError`
- `let-else` and `let-chains` used idiomatically (Rust 2024 edition)
- `const fn` used appropriately for compile-time evaluation

### Weaknesses

#### Weak type modeling for domain concepts

Container names, image names, mount scopes, label keys, and env var ownership are all plain strings. Policy is encoded in naming conventions rather than types. This makes the architecture more brittle because the compiler cannot help prevent mixing things up.

#### Heavy `anyhow` everywhere

Acceptable for a CLI, but internal domain paths would benefit from typed errors. When `resolve_load_workspace` fails, the caller cannot distinguish "workspace not found" from "mount validation failed" without parsing error messages.

Candidates for typed errors: selector resolution, workspace resolution, trust/repo source errors, runtime launch/cleanup errors.

#### Duplicated dependency-graph logic

Cycle detection (Kahn's algorithm) exists in both `manifest.rs:234-281` and `env_resolver.rs:163-209`. Factor into a shared utility before adding more graph-dependent features.

#### `parse_mount_spec` returns `anyhow::Result` but never errors

`workspace.rs:82-84` wraps an infallible inner function in `Ok()`. The return type is misleading — it suggests parsing can fail, but it cannot.

#### `version_check.rs:98` takes `&PathBuf` instead of `&Path`

Should take `&Path` per Rust conventions. Minor, but symptomatic of a pattern where the codebase converts to `String`/`PathBuf` earlier than necessary, losing `Path` semantics.

## 7. Testing Review

### Coverage assessment

317 tests, all passing. Coverage is strong for:

- Selector parsing and validation
- Manifest schema validation and env var lifecycle
- Repo validation (path traversal, symlinks, boundary enforcement)
- Workspace resolution and mount conflict detection
- Runtime command orchestration (via `FakeRunner`)
- CLI help text and parsing
- Plugin bootstrap script behavior (integration tests)

### What is missing

- No test that launcher preview equals actual resolved mounts (Finding 1 gap)
- No real-Docker integration tests — all Docker interaction is through `FakeRunner`
- No docs-consistency tests (version/MSRV/engine mismatches aren't caught)
- No test for automatic `gh auth login` behavior
- No test for invalid `config mount add` input rejection at write time
- No test for `parse_mount_spec` never returning an error

### Test quality

Tests are well-structured with descriptive names. The `FakeRunner` pattern is effective and well-maintained. The `for_load_agent` helper that pre-fills preamble captures is a good pattern for reducing test noise.

### Verification status

These commands passed during review:

```sh
cargo fmt -- --check
cargo clippy
cargo nextest run
```

All 317 tests passed with zero failures and zero warnings.

## 8. Actionable Improvement Plan

### Immediate fixes (before next feature)

1. Fix launcher preview so it includes scoped mounts resolved for the selected agent.
2. Remove or gate the automatic `gh auth login` startup behavior and document the final policy clearly.
3. Validate mount configuration at write time in `config mount add`.
4. Update the stale docs:
   - `PROJECT_STRUCTURE.md` (MSRV, manifest name, docs engine)
   - Installation/version references
   - Config command reference (add trust management docs)
   - Configuration schema docs (add `[agents]` section)
   - DinD TLS wording in comparison docs
   - Manifest reserved env var docs
   - GitHub auth behavior docs
5. Correct the build-caching narrative in the docs.

### Short-term improvements (next 2-3 PRs)

1. Split `runtime.rs` into build, orchestrate, and gc modules.
2. Split `lib.rs` `run()` into per-command handler functions.
3. Factor out dependency-graph logic from `manifest.rs` and `env_resolver.rs`.
4. Align CI and release verification with the documented `nextest` workflow.
5. Add CI checks for doc drift on Rust version, docs engine, command reference completeness, and config schema coverage.

### Larger refactors (before adding second runtime)

1. Introduce a `RuntimeKind` abstraction for Claude/Codex/Amp support.
2. Implement true build reuse via content fingerprinting.
3. Introduce typed Docker command builders to replace `Vec<&str>` assembly.
4. Replace mount config model with separate unscoped/scoped maps.
5. Move the heaviest inline test suites into separate test modules once files are decomposed.

## 9. Final Verdict

### Is the project healthy?

Yes, for a proof-of-concept. The core product model is sound, validation is thorough, and test discipline is strong.

### Is it maintainable?

Moderately. Validation-layer code is easy to maintain. The orchestration core is approaching a complexity threshold where changes become risky without decomposition.

### Is it readable?

Mixed. Small modules (`selector.rs`, `paths.rs`, `instance.rs`, `repo_contract.rs`) are clear. Large modules (`runtime.rs`, `lib.rs`, `launch.rs`) require too much mental state to navigate.

### Is the documentation trustworthy?

Not fully. Enough core pages are stale that a new contributor should verify doc claims against code before acting on them.

### What should change first before further development continues?

1. Fix the launcher preview mount-set mismatch (security-relevant UX bug).
2. Repair the stale documentation (trust erosion).
3. Decompose `runtime.rs` before it grows further (maintenance ceiling).
