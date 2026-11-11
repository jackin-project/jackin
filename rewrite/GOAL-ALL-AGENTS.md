# Goal: full coding-agent and provider support in Jackin on OpenShell

Make Jackin support every coding agent and provider in the mapping below, running in OpenShell sandboxes.

## Why

Jackin is now fully rewritten on NVIDIA OpenShell. OpenShell is the only execution engine, and every agent must run inside an OpenShell sandbox with credentials held by the gateway, never inside the sandbox. Today only Codex with OpenAI and `omp` with Z.AI are proven live. The other agents and providers in the mapping are blocked, missing a key, missing a pair, or failing in the sandbox. This goal makes the whole mapping work, with live proof per pair, so a user can pick any supported agent and provider in the console or the CLI and get a working sandbox.

## Scope and rules

Work on the latest checkout only: branch `feature/openshell-native` in jackin, jackin-sentinel, jackin-agent-smith, jackin-the-architect, and terminal-components-baseline. Read `rewrite/DESIGN.md` first (authority, decisions D01, D03, D05, D10, D24, D27, D29), then `rewrite/PLAN.md` Phase A, `rewrite/research/agent-artifacts.md`, and `rewrite/blockers/T02` through `T12`. OpenShell is pinned at 0.1.7 in `compatibility.lock.toml`. Use the latest stable version of every tool and agent binary. Jackin builds role images with Docker BuildKit only through `jackin-image`. No Docker, Podman, or host fallback runs an agent. Never print a secret value in a file, log, label, commit, or evidence file.

## Final mapping to support (22 pairs)

Subscription means the host login routed through the OpenShell provider. The sandbox holds only a placeholder auth file (D10). API key means a static plan key, also held by the gateway and injected only at the provider endpoint.

Coding agents (9): Claude Code, Codex, Grok Build, Antigravity (agy), Kimi Code, MiniMax Code, MiMo Code, omp, Muse Code.

Providers (9): Anthropic, OpenAI, xAI, Google Gemini, Kimi, Z.AI, MiniMax, MiMo, Muse (Meta).

| Coding agent | Provider | Configuration |
|---|---|---|
| Claude Code | Anthropic | Subscription |
| Claude Code | Kimi | API key |
| Claude Code | Z.AI | API key |
| Claude Code | MiniMax | API key |
| Claude Code | MiMo | API key |
| Codex | OpenAI | Subscription |
| Codex | Kimi | API key |
| Codex | Z.AI | API key |
| Codex | MiniMax | API key |
| Codex | MiMo | API key |
| Grok Build | xAI | Subscription |
| Antigravity (agy) | Google Gemini | Subscription |
| Kimi Code | Kimi | Subscription |
| MiniMax Code | MiniMax | Subscription |
| MiMo Code | MiMo | Subscription |
| Muse Code | Muse (Meta) | Subscription |
| omp | Kimi | API key |
| omp | Z.AI | API key |
| omp | MiniMax | API key |
| omp | MiMo | API key |
| omp | Anthropic | API key |
| omp | OpenAI | API key |

Claude Code uses subscription only for Anthropic. Codex uses subscription only for OpenAI. For Kimi, Z.AI, MiniMax, and MiMo, Claude Code and Codex use an API key only, through the provider Anthropic-compatible coding endpoint for Claude Code (base URL and auth token env in the sandbox user config) and the provider OpenAI-compatible endpoint for Codex (a custom model provider in the sandbox Codex config). Kimi Code, MiniMax Code, and MiMo Code use subscription, because each native client supports its own subscription natively. Anthropic and OpenAI API keys are used by `omp` only. Claude Code and Codex never use an Anthropic or OpenAI API key in this mapping.

## Starting state, verified on this host

- Codex with OpenAI (T01) and `omp` with Z.AI (T09) are Supported and live.
- Claude Code (T02) and Antigravity (T05) are blocked-vendor. This goal text is the operator order for live subscription tests of both. Record that order in `rewrite/decisions.tsv`. For Claude Code, prove the host login still works after sandbox runs, because the gateway refresh can rotate the token and break the host login. For Antigravity, prove it works in the sandbox with no desktop keyring, or record the exact failure.
- Grok Build (T03) failed with HTTP 402, subscription balance exhausted. Re-prove when quota allows, or record dated quota evidence.
- Kimi Code (T08), MiMo Code (T07), and the `omp` plan pairs (T10, T11, T12) are built and hermetically tested but have no plan credential on this host.
- MiniMax Code (T06) has no stable standalone Linux binary. Keep it unavailable with that reason until MiniMax publishes one. Never install an agent from npm in a role image.
- Muse Code (T04) passes the gateway and placeholder checks, then the Linux binary fails in the sandbox before provider work. Find the failing filesystem path by running that binary under `strace` outside the sandbox, fix the role image or launch setup, and re-prove.
- Ten pairs do not exist yet and must be added end to end (catalog in `crates/jackin-core/src/agent.rs`, OpenShell provider profile, policy hosts, credential env names, per-agent sandbox config rendering, account commands, console offers, and usage windows): Claude Code with Kimi, Z.AI, MiniMax, MiMo; Codex with Kimi, Z.AI, MiniMax, MiMo; `omp` with Anthropic; `omp` with OpenAI. Prove every allowed model name from vendor documentation and a live reply. Never guess a model name.

## Credentials

Find each subscription or plan credential first in the host agent login, then in the host `omp` store, then in 1Password with `op` only when `op whoami` succeeds. Record only which source was used, never the value. If a credential truly does not exist on this host, keep that pair unavailable, write or update its file in `rewrite/blockers/` with the exact missing input, and continue with the other pairs. Never report SKIP as PASS.

## Proof required for every pair

On a real gateway (`jackin-docker` on OrbStack, with a re-check on `jackin-local`):

- The role image builds and the second build is cached.
- The sandbox user is `sandbox` (uid 1000) with no working `sudo`.
- The agent runs as the child of `jackin-start` and answers a fixed prompt.
- Only placeholders appear in the sandbox env and files. No key appears in `jackin inspect` output or in any captured stderr.
- Egress reaches only that provider's hosts. A non-agent binary gets no credential.
- Detach and reattach replay the session.
- A shell pane leaves the agent process unchanged.
- A commit made in the sandbox exports to `refs/heads/jackin/<sandbox>/<branch>` with the host working tree, index, and HEAD unchanged.
- Purge refuses unexported work without `--force`.

Flip a pair to Supported in the catalog only when its live proof file exists in `rewrite/evidence/`. Keep the console identical to the baseline (D27). No screen code in Jackin.

## Gates and recording

Run G-FAST (`mise exec -- mbx +1.99.1 xtask ci --fast`) after each unit, G-FULL at the end of each phase, and the per-pair live gate. Before writing any test, get its plan approved by an independent test-reviewer subagent. Keep a test only after an independent reviewer sees it fail without the fix and pass with it. Test through public interfaces. Keep tests hermetic. No live network in nextest. Record every decision, commit, gate result, and evidence path in `rewrite/decisions.tsv`. Commit with Conventional Commits and `git commit -s` (committer Alexey Zhokhov <alexey@zhokhov.com>), and push after each verified unit. Work only on `feature/openshell-native`. Do not merge to main. The operator merges.

## Done

Every pair in the mapping is either Supported with its live evidence file, or unavailable with a current blocker file that names the exact missing input (key, quota, vendor artifact, or binary fix). No pair is left unexamined.
