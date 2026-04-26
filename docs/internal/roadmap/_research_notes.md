# Research Notes — Readability & Modernization Roadmap

Scratchpad for sources, links, and snippets gathered during analysis.
Append new entries at the bottom; include retrieval date.

---

## 2026-04-26 — Error handling ecosystem (Rust)

**Sources:**
- https://dev.to/leapcell/rust-error-handling-compared-anyhow-vs-thiserror-vs-snafu-2003
- https://users.rust-lang.org/t/opinions-on-error-stack/98213
- https://markaicode.com/rust-error-handling-2025-guide/

**Summary:**
- `anyhow` (application-level) + `thiserror` (library-level) remains the 2025–2026 consensus for CLIs. No strong pressure to switch.
- `error-stack` (Hasura/notion-of-stacks): richer context chains, heavier API surface; community reception divided. Good for long-running services, overkill for a single-binary CLI.
- `miette`: fancy diagnostic printing (source spans, labels, help text). Significant gain if `jackin` errors need to point users to config file lines. Not a replacement for anyhow/thiserror — it layers on top.
- `snafu`: fine-grained context typing. Good when you need many distinct error variants. More verbose than thiserror.
- **jackin verdict (iteration 1):** `anyhow` + `thiserror 2.0` is correct for this project's size and single-binary nature. `miette` is worth evaluating only for the config-file validation path (`manifest/validate.rs`, `config/editor.rs`) where pointing to the offending TOML line would reduce operator friction. See §7 Error Handling.

---

## 2026-04-26 — Ratatui TUI snapshot testing

**Sources:**
- https://ratatui.rs/recipes/testing/snapshots/
- https://crates.io/crates/ratatui-testlib
- https://github.com/ratatui/awesome-ratatui

**Summary:**
- Official ratatui approach: `Terminal::new(TestBackend::new(w, h))` + `insta::assert_snapshot!` on `.buffer()`.
  Limitations: terminal-size-sensitive (must fix dimensions), no ANSI colour in snapshots by default.
- `ratatui-testlib` (raibid-labs, 2025): runs TUI in real PTY, captures via terminal emulator, integrates with insta. More faithful rendering but heavier setup; mainly useful for colour-sensitive tests.
- **jackin verdict (iteration 1):** `insta` + `TestBackend` is the right first step. `ratatui-testlib` is deferred until colour fidelity becomes a real need. See §7 Testing.

---

## 2026-04-26 — Spec-driven AI agent development (2026 landscape)

**Sources:**
- https://github.blog/ai-and-ml/generative-ai/spec-driven-development-with-ai-get-started-with-a-new-open-source-toolkit/
- https://github.com/github/spec-kit (GitHub Spec Kit — open source)
- https://www.morphllm.com/spec-driven-development (Kiro overview)
- https://github.com/gotalab/cc-sdd (cc-sdd harness — Claude Code SDD)
- https://www.augmentcode.com/tools/best-kiro-alternatives (Kiro alternatives survey)
- https://www.augmentcode.com/tools/best-spec-driven-development-tools (tool comparison)

**Summary:**
- **Kiro (AWS)**: spec-first IDE — requirements → design → tasks phases, each phase as a durable `.kiro/specs/` artifact. IDE-centric; requires VS Code extension. Not compatible with Claude Code CLI `/loop` patterns.
- **GitHub Spec Kit**: open-source toolkit (REQUIREMENTS.md, DESIGN.md, TASKS.md template trio), works with any agent via CLAUDE.md/copilot-instructions.md/AGENTS.md. Lightweight. No enforcement mechanism — relies on prompt discipline.
- **cc-sdd (gotalab)**: minimal SDD harness for Claude Code; spec → plan → execute cycle using `.claude/commands/`. Agent-invocable. Compatible with `/loop`. Active in 2025–2026.
- **BMad-Method**: structured "method files" + persona files. More heavyweight, primarily CLAUDE.md based.
- **Tessl/intent-as-source-of-truth**: spec is the canonical source; code is generated output. Too radical a shift for a mature Rust codebase.
- **Plain CLAUDE.md + AGENTS.md + .claude/commands/**: already what jackin does today. The gap is formalising the spec lifecycle (draft → in-progress → merged) and ensuring specs are committed artifacts, not ephemeral chat.
- **jackin verdict (iteration 1):** GitHub Spec Kit's three-file template + cc-sdd's `/loop`-compatible command structure is the most pragmatic fit. See §8.1.

---

## 2026-04-26 — Superpowers alternatives for Claude Code

**Sources:**
- https://github.com/obra/superpowers (superpowers source)
- https://wiki.yowu.dev/en/dev/ai-agent/superpowers-vs-omc (OMC comparison)
- https://dev.to/chand1012/the-best-way-to-do-agentic-development-in-2026-14mn
- https://www.scriptbyai.com/claude-code-resource-list/

**Summary:**
- **Oh My ClaudeCode (OMC)**: throughput/parallelisation focus; less about structured discipline. Not a 1:1 superpowers replacement.
- **Shipyard**: extends superpowers philosophy with IaC validation/security auditing focus. Heavy for a single-maintainer Rust CLI project.
- **Local-Review**: parallel diff reviews by multiple agents. Complementary, not a discipline framework.
- **Hand-rolled `.claude/commands/*.md`**: Claude Code's native skill mechanism. Already partially in use (`docs/superpowers/specs/` pattern exists but lives outside source control's CI gates). Requires operator to write and maintain the skill files.
- **jackin verdict (iteration 1):** A minimal hand-rolled approach — `docs/internal/agent-skills/` (committed, reviewed, versioned) + spec lifecycle in `docs/internal/specs/` — delivers the essential outcomes without the `obra/superpowers` dependency or framework lock-in. See §8.2.

---

## 2026-04-26 — Cargo workspace vs single crate

**Sources:**
- https://matklad.github.io/2021/08/22/large-rust-workspaces.html (matklad — still authoritative)
- https://users.rust-lang.org/t/large-private-single-project-workspace-or-not/67185
- https://mmapped.blog/posts/03-rust-packages-crates-modules (Rust at scale)
- https://nickb.dev/blog/cargo-workspace-and-the-feature-unification-pitfall/ (feature unification pitfall)

**Summary:**
- Matklad rule of thumb: "for projects 10k–1M lines, flat workspace layout makes most sense; rust-analyzer at ~200k LOC is a good example."
- jackin is ~40k LOC — below the threshold where workspace splitting compels itself.
- Key workspace benefit: parallel compilation across crates on multi-core. Key cost: `feature unification` can create surprising dependency variants, more `Cargo.toml` maintenance.
- Single-crate benefits: simpler `use` paths, no inter-crate API compatibility discipline needed, one lint pass, simpler CI.
- **jackin verdict (iteration 1):** Stay single-crate for now. Workspace becomes worth considering if `operator_env` or `config/editor` reach a point where they'd benefit from independent versioning or if a `jackin-daemon` or `jackin-library` use case emerges. See §4.

---

## 2026-04-26 — Structured logging: log vs tracing for Rust CLI

**Sources (retrieved 2026-04-26):**
- https://blog.logrocket.com/comparing-logging-tracing-rust/ (LogRocket — comparing log vs tracing)
- https://www.shuttle.dev/blog/2023/09/20/logging-in-rust (Shuttle — logging in Rust 2025)
- https://docs.rs/tracing (tracing crate docs)
- https://tokio.rs/tokio/topics/tracing (tokio tracing guide)

**Summary:**
- `log` crate: stable de-facto standard, "lowest common denominator", works with env_logger for RUST_LOG filtering. Appropriate for synchronous CLIs.
- `tracing` crate: span-based, async-aware, structured fields. The right choice for async servers and services. Overkill for synchronous CLIs.
- For `jackin`: defer. Operator output (TUI step helpers) is intentionally styled, not logging. Developer debug traces (--debug mode) expose Docker commands intentionally. A RUST_LOG=debug path using `log` + `env_logger` would be a developer convenience improvement, not a readability fix.

---

## 2026-04-26 — astro-og-canvas 0.11.1 exactOptionalPropertyTypes conflict

**Source:** Direct reading of `docs/src/pages/og/[...slug].png.ts` and `docs/package.json` (iteration 3).

**Summary:**
- `astro-og-canvas ^0.11.1` is the pinned version.
- User-code conflict: `logo: undefined` on the `getImageOptions` return value. Under `exactOptionalPropertyTypes` this is a type error — must omit the property instead.
- Fix: delete `logo: undefined,` from the options object (~line 35 of the OG card generator).
- Possibly more conflicts in `astro-og-canvas` internals — needs `bunx tsc --noEmit` to confirm.

---

## 2026-04-26 — OpenSpec (Fission-AI/OpenSpec)

**Sources (retrieved 2026-04-26):**
- https://github.com/Fission-AI/OpenSpec
- https://openspec.dev/
- https://openspec.pro/
- https://www.augmentcode.com/tools/best-spec-driven-development-tools (tool comparison, 2026)
- https://medium.com/@richardhightower/agentic-coding-gsd-vs-spec-kit-vs-openspec-vs-taskmaster-ai-where-sdd-tools-diverge-0414dcb97e46

**Summary:**
- **What it is:** Lightweight spec-driven development framework. "Brownfield-first" philosophy — designed for iterating on mature codebases, not greenfield starts. Published to npm; `npm install -g @fission-ai/openspec@latest` (requires Node.js 20.19.0+).
- **Core workflow — three-phase state machine:**
  1. `/opsx:propose` — creates `openspec/changes/<feature>/` with `proposal.md`, `design.md`, `tasks.md`, `specs/`
  2. `/opsx:apply` — task-based implementation with progress tracking
  3. `/opsx:archive` — completed change moves to `openspec/changes/archive/[date]-[feature]/`; specs persist at `openspec/specs/<capability>/spec.md`
- **Delta markers (unique feature):** Proposal artifacts annotate requirements as `ADDED`, `MODIFIED`, or `REMOVED` relative to existing functionality. Makes brownfield change scope explicit at the spec level — the AI and operator agree on what changes, not just what the feature is.
- **Living specs:** Completed specs persist in `openspec/specs/` as architecture documentation; they don't disappear post-archive. This is the same "specs as living documentation" principle as the Starlight MDX approach already recommended in §8.1, but stored in a private directory.
- **Claude Code compatibility:** Integrates via native slash commands (`/opsx:propose`, `/opsx:apply`, `/opsx:archive`). `/loop`-compatible. Works with 25+ AI assistants.
- **vs GitHub Spec Kit:** Lighter artifacts (~250L vs ~800L); less rigid phases; explicitly brownfield-aware. Spec Kit produces a fixed REQUIREMENTS/DESIGN/TASKS trio; OpenSpec separates change proposals (transient) from capability specs (permanent).
- **vs cc-sdd:** cc-sdd is a Claude Code–specific harness (spec → plan → execute); OpenSpec adds delta markers and a clear propose/apply/archive lifecycle. cc-sdd has no brownfield change-tracking.
- **Blocker for jackin:** The `openspec/` directory structure creates an internal artifact hierarchy that competes with the `docs/src/content/docs/specs/*.mdx` public-site approach already chosen in §8.1. They address different concerns (workflow automation vs. public living documentation), so they could coexist, but adds tooling complexity. Node.js 20.19+ is already a dev dependency (via bun), so installation cost is low.
- **jackin verdict:** OpenSpec's delta-marker approach is the most compelling feature not found in any other tool — explicitly tagging ADDED/MODIFIED/REMOVED at the spec level would be high-value for the §4 module-split proposals in this roadmap, where "what changes" needs to be agreed before refactoring begins. The `/opsx:propose` + Starlight MDX combination is feasible: use OpenSpec for the proposal/apply workflow, then migrate the final spec content to a Starlight MDX page for public living documentation. Recommended as a complement to cc-sdd rather than a replacement — evaluate for the first structural refactoring PR.

---

## 2026-04-26 — IIKit (intent-integrity-chain/kit)

**Sources (retrieved 2026-04-26):**
- https://github.com/intent-integrity-chain/kit (README, v2.10.0)
- https://tessl.io/registry/tessl-labs/intent-integrity-kit

**Summary:**
- **What it is:** "Intent Integrity Kit" — closes the "intent-to-code chasm" via cryptographic verification at each phase. Chain: `Intent → Spec → .feature → Steps → Code`. SHA256-locks Gherkin `.feature` files before implementation to prevent AI from modifying tests to match buggy code.
- **Installation:** Via Tessl CLI (`npm install -g @tessl/cli && tessl install tessl-labs/intent-integrity-kit`). Requires Tessl — direct repo clone does NOT produce self-contained skills (shared reference files are only resolved at publish time).
- **Phases (8 skills):** `iikit-core init` → `iikit-00-constitution` → `iikit-01-specify` → `iikit-02-plan` → `iikit-03-checklist` → `iikit-04-testify` → `iikit-05-tasks` → `iikit-06-analyze` → `iikit-07-implement`. Never skip phases.
- **BDD verification chain:** `iikit-04-testify` generates Gherkin `.feature` files from Given/When/Then acceptance criteria and stores a SHA256 hash in `context.json` + git notes. `iikit-07-implement` enforces: hash check (tamper detection), step coverage (`verify-steps.sh`), RED→GREEN TDD cycle, step quality (`verify-step-quality.sh` — no empty bodies, no `assert True`).
- **Tessl integration:** At `/iikit-02-plan`, Tessl installs tiles for the tech stack; at `/iikit-07-implement`, Tessl queries current library APIs before writing code. This is valuable for JS/Python ecosystems with rapidly shifting APIs; less so for Rust where `cargo doc` and `docs.rs` are the authoritative sources.
- **Stars:** 39 (as of 2026-04-24). Active development; v2.10.0 (2026-04-24).
- **Claude Code compatibility:** Explicitly supported (CLAUDE.md → AGENTS.md); also supports Codex, Gemini, OpenCode, Copilot.
- **jackin blockers (multiple):**
  1. **Gherkin dependency:** The BDD verification chain assumes a Gherkin step runner (Cucumber or equivalent). `jackin` uses Rust + `cargo nextest` with `#[test]` — there is no Gherkin step runner, no step coverage tool, and no `verify-steps.sh` equivalent for Rust. The entire cryptographic hash-locking mechanism is inert without a step runner.
  2. **Tessl runtime library queries:** Tessl's value proposition is querying current library APIs at plan/implement time. For Rust, this means querying `docs.rs`-level data — which Tessl doesn't yet support well. The Rust ecosystem is served better by `cargo doc --no-deps`.
  3. **Spec format conflict:** IIKit produces `specs/NNN-feature/spec.md` internal artifacts. These are not compatible with the Starlight MDX living-documentation approach in §8.1 — they would need to be manually migrated post-implementation.
  4. **Heavyweight for single-maintainer:** 8 mandatory phases for every feature PR is disproportionate overhead for a single-maintainer CLI project.
  5. **Tessl lock-in:** Unlike cc-sdd (just `.claude/commands/*.md` files) or OpenSpec (npm install), IIKit can only be installed through Tessl. If Tessl's service or registry changes, IIKit breaks.
- **jackin verdict:** Not recommended. The cryptographic `.feature`-file hash-locking is an elegant solution to AI test corruption, but it's the wrong layer for jackin — Rust's type system and `#[test]` inline tests provide stronger corruption resistance than Gherkin step hashes. The Tessl dependency, 8-phase mandatory workflow, and Gherkin incompatibility with nextest are blocking. Revisit if jackin ever adopts a Gherkin-based acceptance test layer (unlikely given single-crate Rust structure).

---

## 2026-04-26 — cargo-mutants mutation testing

**Sources:**
- https://mutants.rs/ (official docs)
- https://nexte.st/book/cargo-mutants.html (nextest integration)
- https://crates.io/crates/cargo-mutants (v26.1.0, Jan 2026)

**Summary:**
- cargo-mutants integrates with nextest via `--test-tool nextest`.
- Supports `--shard k/n` for distributing large test suites across machines.
- At 1046 tests, a full mutants run is expensive but feasible in CI as a periodic (not per-PR) gate.
- **jackin verdict (iteration 1):** Defer mutation testing for now; adopt `insta` snapshot tests first to raise baseline coverage signal. Revisit when test count stabilises post-PR #171 merge. See §7 Testing.
