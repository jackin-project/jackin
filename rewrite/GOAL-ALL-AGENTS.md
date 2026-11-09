# Goal: all coding agents and providers

Make Jackin support every coding agent and provider tuple in the verified
D29 mapping, running in NVIDIA OpenShell sandboxes. Complete all 22 tuples
so each is either Supported with current live-gateway evidence or explicitly
unavailable or blocked-vendor with dated, concrete evidence. Resume from the
latest checkout and existing Phase A evidence; preserve valid completed work
and re-verify it on the final head where required.

## Platform and repository boundaries

- OpenShell 0.1.7 remains pinned in compatibility.lock.toml.
- OpenShell is the only execution engine. No Docker, Podman, Apple container,
  or host fallback may run an agent, sandbox, or shell.
- Jackin builds role images with Docker BuildKit only through jackin-image.
- Work only on the latest feature/openshell-native checkouts in jackin,
  jackin-sentinel, jackin-agent-smith, jackin-the-architect, and
  terminal-components-baseline.
- rewrite/DESIGN.md is the authority, especially D01, D03, D05, D10, D24,
  D27, and D29. Read rewrite/PLAN.md Phase A and rewrite/blockers/T02 through
  T12 before starting.
- Use the latest stable version of every tool and agent, without prereleases.
- Never print or record a secret value in a file, log, label, commit, chat,
  or evidence.
- Do not merge to main.

## Authentication rules

- Subscription means the host login through the OpenShell provider, with only
  placeholders inside the sandbox (D10).
- API key means a static plan key, likewise held by the gateway and represented
  only by placeholders inside the sandbox.
- Claude Code uses subscription authentication only for Anthropic.
- Codex uses subscription authentication only for OpenAI.
- For Kimi, Z.AI, MiniMax, and MiMo, Claude Code and Codex use an API key:
  Claude Code through the provider's Anthropic-compatible endpoint using
  ANTHROPIC_BASE_URL and its auth-token environment variable, and Codex through
  the provider's OpenAI-compatible endpoint using a custom model provider in
  config.toml.
- Anthropic and OpenAI API keys are used by omp only.

## Tuple queue

Work through this 22-tuple queue, reconciling each starting note with the
latest code, decisions, blockers, and evidence rather than discarding newer
verified progress:

1. T01: Codex with OpenAI, subscription. Supported; keep it green.
2. T02: Claude Code with Anthropic, subscription. Blocked-vendor. This
   objective is the operator's order for a live subscription test. Record
   that order in rewrite/decisions.tsv. Prove the host login still works
   after sandbox runs, with no refresh-race logout.
3. T03: Grok Build with xAI, subscription. Built but quota exhausted.
   Re-prove when quota allows, or record dated quota evidence.
4. T04: Muse Code with Muse subscription. The gateway path works. Find the
   failing filesystem path with strace outside the sandbox, fix the role image
   or launch setup, and re-prove.
5. T05: Antigravity (agy) with Google Gemini, subscription. Blocked-vendor.
   This objective is the operator's order for a live test. Prove agy works in
   the sandbox without a desktop keyring, or record precisely why it cannot.
6. T06: MiniMax Code with MiniMax plan key. Keep unavailable while MiniMax
   has no stable Linux binary. Do not install the agent from npm in an image.
7. T07: MiMo Code with MiMo Token Plan key, Singapore plan. Built; finish
   its live proof or document the current blocker.
8. T08: Kimi Code with Kimi membership key. Built; finish its live proof or
   document the current blocker.
9. T09: omp with Z.AI plan key. Supported; keep it green. Move the sandbox
   omp pin to the latest stable release and re-prove it.
10. T10: omp with Kimi membership key.
11. T11: omp with MiniMax plan key.
12. T12: omp with MiMo Token Plan key.
13. T13: Claude Code with Kimi, API key, Anthropic-compatible endpoint.
14. T14: Claude Code with Z.AI, API key, Anthropic-compatible endpoint.
15. T15: Claude Code with MiniMax, API key, Anthropic-compatible endpoint.
16. T16: Claude Code with MiMo, API key, Anthropic-compatible endpoint.
17. T17: Codex with Kimi, API key, OpenAI-compatible endpoint.
18. T18: Codex with Z.AI, API key, OpenAI-compatible endpoint.
19. T19: Codex with MiniMax, API key, OpenAI-compatible endpoint.
20. T20: Codex with MiMo, API key, OpenAI-compatible endpoint.
21. T21: omp with Anthropic, API key only.
22. T22: omp with OpenAI, API key only.

For T13-T22, complete and verify every required integration surface: the D29
catalog, OpenShell provider profiles, policy hosts, per-agent sandbox config
rendering, account flow, console offers, and usage windows. Prove every allowed
model name from vendor documentation and a live reply for that pair. Never
guess a model name.

## Credential discovery

- Search for each plan key first in the host omp store, then in 1Password
  using op only when op whoami succeeds, then in the host agent login.
- Record only which credential source was used, never its value.
- If a required key genuinely does not exist on the host, keep that pair
  unavailable, write or update its rewrite/blockers/ file, and continue with
  independent pairs.
- Never mark SKIP as PASS.

## Live proof

A pair may flip to Supported in crates/jackin-core/src/agent.rs only after its
complete live proof is stored under rewrite/evidence/. Run the proof on a real
gateway, primarily jackin-docker on OrbStack, and re-check it on jackin-local.
The proof must establish all of the following:

- The image builds successfully and is cached on rebuild.
- The sandbox user is sandbox and has no sudo access.
- The agent runs as the child of jackin-start and answers a fixed prompt.
- The sandbox contains only placeholders; no key appears in files,
  environment variables, inspect output, or stderr.
- Egress reaches only the provider hosts.
- Detach and reattach replay works.
- Opening a shell pane leaves the agent running.
- Export lands on refs/heads/jackin/<sandbox>/<branch> while leaving the host
  working tree unchanged.
- Purge refuses unexported work.
- The credential class pays for the response, with no paid fallback outside
  the tuple's permitted authentication class.

## Verification and delivery loop

- Run G-FAST after each verified unit.
- At the end, run G-FULL and the G-E2E journey for each Supported pair, using
  the exact gate commands and current Rust pin from rewrite/PLAN.md.
- Before writing any new test, obtain approval of its test plan from an
  independent reviewer, naming the behavior, positive and negative cases, and
  why no existing test covers them.
- Keep each new test only after an independent reviewer observes it fail on
  broken code and pass on fixed code.
- Do not weaken, skip, or delete a test to conceal broken behavior.
- Record every decision and gate result in rewrite/decisions.tsv and update
  the applicable Phase A tasks and evidence.
- Verify the required committer identity, use Conventional Commits, commit
  with git commit -s, and push after each verified unit.
- Use subagents for implementation, review, and verification as required by
  the repository protocol, with a different reviewer for each implementer.

## Blocked-path rule

- For missing credentials, exhausted quota, vendor restrictions, unavailable
  stable artifacts, gateway or upstream limitations, or interactive host
  authentication that cannot be completed, record dated evidence and the exact
  failing condition in rewrite/blockers/ and rewrite/decisions.tsv.
- Obtain independent blocker confirmation where rewrite/PLAN.md requires it.
- Continue all remaining independent work. Never substitute a fallback,
  fabricate evidence, weaken a proof, or report a blocked pair as Supported.

## Completion criterion

The goal is complete when:

1. Every tuple T01-T22 has a final-head disposition: Supported with the full
   required evidence, or unavailable/blocked-vendor with a dated blocker and
   no remaining independent implementation or verification work.
2. All Supported pairs have passed the required final G-FULL and G-E2E proof,
   and every completed unit has its G-FAST result.
3. All new tests have the required independent plan approval and red/green
   verification.
4. The catalog, evidence, blockers, decisions, and Phase A records agree with
   the final code.
5. All verified units are committed with git commit -s and pushed on
   feature/openshell-native, with no merge to main.
6. The final report lists each tuple's status, evidence or blocker path, gate
   results, commit SHAs, and any remaining external blocker.
