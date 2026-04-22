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

## Codex Commit Attribution (agent-only)

Until Codex supports automatic commit trailers, every commit created by the Codex agent in this repository must include this exact trailer:

```text
Co-authored-by: Codex <codex@openai.com>
```

Add it manually when creating or amending Codex-generated commits.

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
- [CONTRIBUTING.md](CONTRIBUTING.md) — contribution flow, DCO v1.1 text, and license terms for external contributors.
