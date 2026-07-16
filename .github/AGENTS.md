# GitHub Agent Rules

Apply this file to pull requests, GitHub metadata, composite actions, and repository automation under `.github/`. Workflow-specific rules are directory-local.

## Pull Requests

- Read [`../PULL_REQUESTS.md`](../PULL_REQUESTS.md) before opening, updating, or merging a PR. Agent-created PRs target `main` unless the operator names another base.
- Use `gh` for PR metadata, checks, and merges. Build Markdown bodies in a file, create or edit with `--body-file`, then read the rendered body back.
- Keep the PR title and body aligned with the final diff. Always include the template's isolated `jackin-dev pr sync <PR_NUMBER>` checkout block.
- Do not force-push without the operator's explicit approval for that remote branch.

## Merge Gate

- Merge only when every required PR check passes. Never bypass a failed or pending check without explicit operator approval for that named check.
- Squash-merge through `gh`; preserve the PR number in the squash title and generate the squash body with `jackin-pr-trailers`.

## Actions and Composites

- Pin third-party actions to a full commit SHA with the upstream version comment. Use the least privileged `permissions` required by each workflow or job.
- Before changing `.github/workflows/`, read and follow that directory's local instructions. Keep composite actions reusable and apply the same token and cache rules when they participate in workflows.
