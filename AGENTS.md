# AGENTS.md

This repository uses `main` as its primary branch. This file is the canonical home for rules and restrictions that apply only to AI agents. Rules that apply equally to human contributors and agents live in topic-specific files linked under **Shared conventions** below.

## Pull Request Merging (agent-only)

**Agents must never merge a pull request without explicit per-PR confirmation from the human operator.**

- Open the PR, share the URL, and stop. The default response after creating a PR is "PR URL — ready for your review" — not a merge command in the same turn.
- Prior "just do it" / "don't wait for me" / "proceed autonomously" / "merge silently" authorizations apply only to the specific workstream the operator was discussing when they issued them. They do not carry forward to later PRs in the same session or to new sessions. Treat each PR as a fresh approval gate.
- `--admin` / branch-protection bypass is a privilege, not a default. Use it only when the operator explicitly authorizes merging *this specific PR*.
- Phrasing that does NOT authorize merge (ask anyway): "proceed", "don't wait for me", "do everything autonomously", "looks good". Phrasing that does: "merge it", "merge this one", "you can merge now", "ship it" (still prefer to confirm "ship = merge now?" for high-blast-radius PRs).
- Bounded authorization: if the operator says "merge all the PRs we just discussed" or similar, merge only the named set — not unrelated PRs that exist or that you open later.

If you are uncertain whether authorization applies to the PR in front of you, ask. The cost of pausing is ~30 seconds; the cost of merging something the operator wasn't ready for is much higher.

## Commit Attribution (agent-only)

Every commit created by an AI agent in this repository must include **exactly one** `Co-authored-by` trailer identifying the agent that made the commit. The trailer identifies the **agent tool**, not the underlying model — **never stack multiple agent trailers on one commit** (for example, an Amp-generated commit must not also carry `Co-authored-by: Claude` or `Co-authored-by: Codex` just because Amp used one of those vendors' models under the hood).

Until the listed agents emit their trailers automatically, the trailer must be added by hand when creating or amending the commit.

**Trailers by agent:**

- **Claude** (Claude Code CLI, or any Claude-API coding agent used directly):

  ```text
  Co-authored-by: Claude <noreply@anthropic.com>
  ```

- **Codex** (OpenAI Codex CLI):

  ```text
  Co-authored-by: Codex <codex@openai.com>
  ```

- **Amp** (Sourcegraph Amp, regardless of underlying model):

  ```text
  Co-authored-by: Amp <amp@ampcode.com>
  ```

Amp may additionally emit an `Amp-Thread-ID:` metadata trailer; that is acceptable alongside the single `Co-authored-by: Amp` trailer because the thread ID identifies the conversation, not a second agent.

If you are uncertain which agent is creating the commit, ask — the trailer is how the operator tracks which agent produced which change, and wrong attribution is worse than no attribution.

## Code review & automated scanning (agent-only)

When performing code review or automated scanning on this repository, do not flag items listed under "Accepted exceptions" on the [Open review findings](docs/src/content/docs/reference/roadmap/open-review-findings.mdx) roadmap catalog. Those items are retained intentionally and have been reviewed.

The catalog itself is a forward-looking backlog — consult it on demand when a review task calls for it. It is not operational context and should not be loaded at session start.

## Shared conventions

Rules in the files below apply to everyone working in the repo — human and agent:

- [RULES.md](RULES.md) — documentation-location convention (no project rules in tool-specific files).
- [BRANCHING.md](BRANCHING.md) — branch naming, feature-branch policy, what never to commit to `main`.
- [COMMITS.md](COMMITS.md) — Conventional Commits format, DCO sign-off, pre-commit verification commands.
- [TESTING.md](TESTING.md) — test runner setup and commands.
- [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) — navigational map of the codebase, documentation site, Docker assets, and CI workflows.
- [DEPRECATED.md](DEPRECATED.md) — ledger of deprecated APIs, CLIs, config values, and usage patterns that are still supported but should eventually be removed.
- [CONTRIBUTING.md](CONTRIBUTING.md) — contribution flow, DCO v1.1 text, and license terms for external contributors.
