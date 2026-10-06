# Goal: unified agent usage

Complete plans 001–008 sequentially on `chore/roadmap-unified-agent-usage` and
PR #898. Completion means one canonical Rust projection and durable broker serve the
simple CLI, native Console, resolved-launch Capsule, and production desktop; all
provider, parity, accessibility, and signed-distribution contracts pass.

## Operator protocol

1. Run only the first TODO row whose dependencies are DONE.
2. Re-read its cited specs/research and current source before editing.
3. Commit with DCO and Codex co-author; push immediately to the tracked PR #898 branch.
4. Mark DONE only with recorded command evidence. Mark BLOCKED with exact external
   input or proven architectural contradiction. Never create another branch or PR.
5. After changing plan files intentionally, regenerate the package fingerprint.

## Final gates

```sh gates
test "$(git branch --show-current)" = "chore/roadmap-unified-agent-usage"
test "$(gh pr list --head chore/roadmap-unified-agent-usage --json number --jq '.[0].number')" = "898"
rtk cargo xtask research check
rtk mise run fmt
rtk mise run lint
rtk mise run test
rtk mise run desktop-ci
rtk mise run desktop-merge
```

Credentialed Developer ID/notarization/publication/Homebrew proof is Plan 008 done
evidence. It is intentionally not rerun by this local goal script because credentials
are external and the immutable published digest must be verified from its release log.

