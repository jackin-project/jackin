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
