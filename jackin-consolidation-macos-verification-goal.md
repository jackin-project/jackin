/goal

# Consolidate jackin, then prove macOS account, usage, and capsule functionality

## 1. Mission, sequence, and final state

Repository: https://github.com/jackin-project/jackin
Canonical checkout: `/Users/donbeave/Projects/jackin-project/jackin`
Required repository discovery tool: https://github.com/donbeave/repo-scan
Known local repo-scan implementation to inspect first: `/Users/donbeave/Projects/repo-scan`

Execute this task. Do not stop after analysis, a plan, recommendations, a checklist, opening a PR, or reporting that unit tests pass.

First discover and reconcile every accessible local jackin clone, linked worktree, bare object store, branch, stash, detached state, meaningful uncommitted change, remote branch, and PR. Integrate all correct, nonredundant work into `main`. Preserve the intent of work that needs adaptation rather than blindly importing obsolete implementations. Prove every disposition. Then safely remove redundant local copies and integrated non-main branches, leaving the canonical checkout on `main`.

After this initial consolidation and safe cleanup, verify jackin on this actual macOS host. Fix every demonstrated defect in account discovery, account selection, usage screens, provider limits, credential handoff, real container startup, Capsule usage, and reconnect/restore. Extend and strengthen tests so they exercise the behavior the user actually depends on. Land the fixes through the repository's protected workflow. Finish with another fresh discovery and cleanup pass so only the canonical checkout and `main` remain again.

The final product must:

- Discover all supported account sources available to this macOS user, including multiple accounts for the same provider and accounts configured outside default profile directories.
- Show the complete global account inventory and accurate available limits on host surfaces. A compact summary must not impose a hidden provider or account limit on the full inventory.
- Launch real Docker capsules through normal jackin user workflows, preserve the selected identities, and continue using the accounts' authorized authentication without unnecessary re-login.
- Show all accounts available to a Capsule's authorized scope, not merely the currently focused account or one row per agent. Preserve account identity and usage semantics across host, broker, launch, container, and UI boundaries.
- Demonstrate these behaviors on this host with the actual binaries/images and appropriate live authentication/usage checks, in addition to deterministic regression tests.

Never report full completion while a required behavior is broken, untested, skipped, or blocked. A proven external limitation can justify an explicit incomplete status, not a false success. Continue every independent workstream that remains feasible.

## 2. Mandatory execution model: exact match, no fallback

Use exactly:

```text
model = gpt-6.1-sol
reasoning effort = medium
```

This applies to the coordinator and every subagent, researcher, implementer, reviewer, security reviewer, tester, verifier, blocker investigator, resumed agent, and replacement agent. It also applies to any model inference initiated as part of this jackin task.

No other model is authorized. No automatic selection, fallback, alias that cannot be resolved to the exact requested model, stronger equivalent, effort substitution, or provider-side silent fallback is permitted. Previous jackin instructions specifying a different model or reasoning effort are superseded by this goal. Do not change unrelated repositories' model policies.

Before repository work begins, verify the actual execution harness can request this exact model and effort. Configure it using its real supported controls; do not invent a flag or claim a setting works because it appears in a prompt. Check available trusted runtime/request metadata, not just an agent's self-description. Retain a secret-free record of the effective configuration and verification method. Apply and verify the same configuration at every agent spawn and resume.

If exact configuration is unavailable or cannot be verified, fail closed before delegating or continuing substantive work. Report the concrete failing capability check. Do not switch models to get unstuck and do not conceal the limitation.

Testing another supported agent does not authorize inference with that agent's default model. Non-generative login status, identity, usage, version, startup, and protocol checks are allowed. A generated smoke response is allowed only through a route that verifiably executes `gpt-6.1-sol` with `medium`. For other runtimes, separate authentication/startup verification from unexecuted inference; never relabel the former as a successful model completion.

## 3. Autonomous execution and aggressive delegation

Never ask routine clarification, prioritization, branch-name, implementation, or next-step questions. This goal authorizes the necessary jackin changes, reviewed integration, and evidence-gated cleanup described here. Resolve ambiguity using repository code, history, tests, current official provider documentation, and independent subagent review. Do not use an old instruction to ask for routine permission when this goal already supplies it.

Autonomy does not authorize credential bypass, fabricating consent, rewriting shared history, defeating branch protection, touching unrelated repositories, or deleting unverified data. When an OS permission, provider consent, or external capability is genuinely missing, capture the evidence, preserve affected state, report the exact required action, and continue unrelated work. Do not repeatedly retry an unchanged blocker or misclassify a code defect as an external limitation.

Use subagents as the default execution mechanism. The coordinator decomposes, delegates, tracks dependencies, resolves conflicting evidence, and integrates independently verified results. Delegate substantive research, implementation, testing, security review, and final verification whenever supported. Do not silently replace unavailable subagent capability with an unreported serial workflow.

Start independent workstreams for:

1. Machine-wide repository discovery, coverage, and recovery safety.
2. Remote branches, PR history/reviews, source provenance, and integration ordering.
3. Account discovery, provider/agent identity, configuration, and migrations.
4. Host Console/native usage surfaces and canonical usage projection.
5. Container launch, credential transport, Capsule UI, and session restore.
6. Regression-test architecture, production-path E2E, and generated CI coverage.
7. Independent security review and independent acceptance verification.

Expand or split workstreams as independent tasks emerge. Reviews must cover different failure modes rather than repeat the same checklist. The author of a fix must not be its only verifier.

Parallelize safely. Prefer one active integration branch and one canonical checkout. Assign disjoint file ownership. Serialize all Git index, checkout, merge, commit, push, and shared-manifest changes through one integration owner. A subagent must not run a branch-changing command while another writes in that checkout. Use a temporary worktree only when isolation is genuinely necessary; record it in the inventory and remove it after safe integration. Do not create a branch or clone per trivial task.

Bound concurrent compilation, tests, Docker fixtures, and filesystem scans according to measured host capacity. Run repo-scan once per deliberate discovery generation and share its result. Use scoped `rg`/file reads instead of repeated whole-machine searches. Avoid competing Cargo processes on the same target lock. Resource coordination prevents contention; it is not a reason to omit required work.

## 4. Mandatory decision and root-cause rules

Apply the following rules throughout, including to old recovered work and review findings:

### How to decide what to do

Judge every piece of work by whether it **should** be done — is it correct, is the current state wrong or inconsistent, does it serve the goal — and **never by ROI, cost, effort, or "is it worth it."** Do not label a known-wrong thing "low-value," "marginal," "an edge case," or "not worth it" to justify leaving it unfixed; reasoning by ROI is exactly what keeps work mediocre. ("The reference / competitor also gets it wrong" is a *gap* argument, not a correctness one — it never makes a wrong thing acceptable.)

The only valid reason to stop short of doing the right thing is that it **provably cannot** be done — a demonstrated limit of the model or tools, not an assumed or cost-based one. "Hard," "heavy," "expensive," or "a lot of work" is never a reason to stop; "proven impossible / blocked" is. When unsure which it is, find out — try it, measure it, prove it — before deciding, and never declare a limit you have not proven.

Present choices to me by correctness and feasibility (real impossibilities, real *capability* tradeoffs like portability or expressiveness), not by ROI.

### Fixing bugs

Assume a correct architecture has no bugs — so every bug is evidence that the architecture *permits* it to exist, not merely that one code path is wrong. **Before fixing any bug, first diagnose the root cause: ask why the architecture allowed this bug to exist at all,** and whether it is one instance of a whole *class* of bugs the same structure would keep producing.

Prefer fixes that remove the structural condition that let the bug exist — so this bug and others like it can no longer occur — over patches at the symptom layer (a guard, a special case, or a workaround that leaves the enabling structure in place). Reach for a symptom-layer patch only when the root-cause fix is **provably** infeasible or genuinely belongs in a separate change, never merely because it is larger or harder; when you do, say so and name the root cause you are deferring.

This is a thinking-first rule, not a mandate to refactor on every fix: the root-cause analysis is **always** required; reshaping the architecture to act on it is frequent but conditional on its being the right and feasible move.

For every defect record: reproduction, expected behavior, actual behavior, root cause, enabling architectural condition, related failure class, chosen correction, regression that fails before the correction, verification evidence, and independent review result.

## 5. Frequent commits and protected integration

Commit small, coherent changes frequently, after a meaningful verified step and before switching workstreams. Push progress regularly to the correct active branch. Do not accumulate a large dirty tree or save all work for one final commit. Keep code, its regression test, and necessary documentation changes together where practical.

Follow current signing/sign-off, commit-message, formatting, and PR requirements. Never include secrets, raw account scans, private host archives, or unsanitized logs in a commit or public PR comment.

Reuse a suitable existing branch/PR. When a new branch is necessary, choose a reasonable name autonomously; create only the minimum required branches. `main`-only is the required resting/final state, not permission to bypass protected PR integration.

Synchronize active branches with `main` using normal merges, not rebases. Do not amend published history or force-push. Re-read repository merge settings: the review snapshot allowed squash landing and disallowed merge-commit/rebase landing. Use the currently permitted reviewed PR merge method without changing those settings. Distinguish merging main into a branch from the server's PR landing method.

After each landing, fetch and validate the actual resulting `main` SHA. A passing source branch, a GitHub mergeability flag, a pending auto-merge, or an advisory evidence ledger is not proof of a verified landing.

## 6. Repository evidence to re-read first

The preparatory remote review observed `main` at:

```text
6c389d38eadab93d6d6a4005e01dbdd8c4160221
```

Treat this as a starting reference, not a pinned implementation target. Re-fetch and record the current heads at execution time. Files and PRs can change.

Observed non-main branches and PRs:

| Branch | Observed head | PR | Required investigation |
|---|---|---|---|
| `codex/ci-evidence-ledger-20260929` | `2990df17e25f30afca84804d9c402abc1ce00231` | #1108 | Draft advisory CI provenance ledger; reconcile with the current workflow generator. Do not promote advisory receipts into behavioral proof. |
| `codex/credential-routing-recovery-20260930` | `a6c85ba8a154ccce224dce80d1ed5ef8fa03bc6e` | #1109 | Exact credential/source/provider-route identity, alias handling, mixed-source rejection, and private config/catalog publication. Verify current main before adapting recovered changes. |

These are not an exhaustive inventory of local work or future remote state.

Read current `AGENTS.md`, applicable nested instructions, `RULES.md`, `ENGINEERING.md`, `BRANCHING.md`, `COMMITS.md`, `PULL_REQUESTS.md`, `HOST_AND_CONTAINER.md`, `PROJECT_STRUCTURE.md`, `TESTING.md`, `DEFECT_LEDGER.md`, relevant crate READMEs, and existing consolidation records under `plans/repository-consolidation/` when present. Reconcile historical assertions against code and current execution evidence.

Starting source map:

| Area | Starting points |
|---|---|
| Agent identity and launchable runtimes | `crates/jackin-core/src/agent.rs`, runtime adapters |
| Account/configuration authority | `crates/jackin-config/`, account CLI and workspace account commands in `crates/jackin/` |
| Discovery and account identity | `crates/jackin-usage/src/host.rs`, `host/discovery.rs`, `host/accounts*`, credential resolver |
| Broker and durable quota state | `crates/jackin-usage/src/host/broker*`, `coordinator*`, provider modules under `usage/` |
| Host orchestration and credential staging | `crates/jackin-runtime/`, `crates/jackin-launch/`, `crates/jackin-instance/`, `crates/jackin-env/` |
| Protocol and capability boundaries | `crates/jackin-protocol/`, runtime relay, Capsule `usage_relay_proxy*` |
| Actual Capsule process and UI | `crates/jackin-capsule/src/daemon*`, `session*`, `runtime_setup*`, `tui*`, `client*` |
| Host UI | `crates/jackin-console/`, shared `crates/jackin-tui/`, `native/`, `crates/jackin-usage-ffi/` |
| Existing broker integration tests | `crates/jackin/tests/usage_broker_e2e.rs`, `usage_broker_e2e/docker.rs`, recovery tests |
| Build, test selection, and CI | `.config/nextest.toml`, `mise.toml`, `crates/jackin-xtask/`, `.velnor/config.toml`, `.github/workflows/`, Docker build inputs |

Specific review leads, not blanket claims that every related path is broken:

- The runtime registry contains 12 agents: Claude, Codex, Amp, Kimi, OpenCode, Grok, Antigravity, Gemini, Cursor, Muse, omp, and Hermes. Enumerate the current registry instead of freezing this list.
- `HostSurfaceId::ALL` has more provider surfaces than the seven entries in `DESKTOP_PROVIDER_ORDER`. Trace every consumer of the seven-provider contract and prevent the full accounts/usage UI from silently omitting supported sources. Multi-provider clients must not become a fake billing provider or conflate distinct upstream accounts.
- The usage architecture places provider calls and shared state in a host broker; Capsule discovery is capability-scoped. Verify this actual boundary rather than mounting the host credential tree as a workaround.
- The observed broker Docker tests use `python:3.14-alpine` and scripted socket clients. These can prove aspects of the transport contract, but cannot alone prove the production Capsule binary, real agent startup/authentication, native UI, or account selection works.
- `TESTING.md` requires an Apple Silicon/macOS/OrbStack release-path lane for usage changes, while the observed `.velnor/config.toml` selects an Ubuntu Rust stack and excludes `native/**`, `docker/**`, and other paths from discovery. Audit actual generated jobs and required checks for missing coverage and documentation drift. Do not assume a gate exists merely because documentation names it.

## 7. Phase A — inventory and preservation before mutation

### 7.1 Establish execution and recovery boundaries

Verify this is the intended macOS host and user context. Record OS version, CPU architecture, canonical path identity, available disk space, Git version, Docker context, and running jackin-related processes without printing credentials. Do not mistake a remote Linux execution environment for this host.

Create a private evidence/recovery directory outside every candidate checkout and outside paths scheduled for deletion. Use restrictive permissions. Keep sensitive host inventories and backups there; commit only sanitized requirement/provenance summaries where repository policy permits. Record the evidence path without publishing its contents automatically.

Identify active editors, agents, containers, and processes using candidate checkouts. Quiesce only task-owned activity when safe. If another actor is changing a source, re-snapshot it and defer its deletion until stable. Do not kill unrelated work or delete the working directory of the agent performing consolidation.

### 7.2 Use repo-scan as the discovery owner

Inspect the installed repo-scan binary and the known local implementation. Record source revision/build provenance and actual `--help`. Do not assume the local build is current, overwrite another agent's dirty repo-scan work, or invent CLI flags. Use the current verified implementation.

The reviewed CLI supports this initial discovery shape; verify it before running:

```sh
repo-scan scan https://github.com/jackin-project/jackin \
  --scope machine \
  --force-rescan \
  --status full \
  --report "$PRIVATE_EVIDENCE/repo-scan-before.json"
```

`PRIVATE_EVIDENCE` is the private directory established above. Capture the actual exit status and inspect the report schema and coverage fields. The reviewed tool documents `3` as a usable but incomplete/gapped result and `130` as interrupted. A cached query is not fresh discovery. Exit `0`, including zero matches, is not by itself proof that every intended root was examined.

Use its real `resume`, targeted invalidation/rescan, and report facilities where appropriate. Cover readable user locations, temporary directories, mounted local volumes, nonstandard paths, hidden locations, linked worktree metadata, and bare stores. Account for macOS APFS aliases and `/private` path aliases without treating them as independent clones or silently omitting real roots.

Prove coverage: roots attempted/completed, inaccessible paths, permission denials, excluded roots, stale/provisional records, unfinished traversal, and errors. Resolve avoidable gaps. Do not claim machine-wide completeness with unresolved scope gaps. A repo-scan correctness defect must be independently reproduced and corrected or explicitly bounded before its report can authorize deletion; do not silently substitute `find` for the required discovery tool.

Repo-scan is discovery/reporting software, not a remote-PR enumerator or merge/delete engine. Use Git and GitHub APIs after discovery for exact refs, remote state, PRs, reviews, and integration. Do not extend repo-scan with destructive functionality for this task.

### 7.3 Reconcile each local source using Git

For every discovered clone, worktree, or object store, record canonical filesystem identity, repository identity, Git directory, common Git directory, object-store dependencies, remotes, current HEAD, worktree registrations, and local state. Match normalized repository identity, not a directory basename or only `origin`.

Inspect all local refs and remote-tracking refs, tags, detached HEADs, stashes, reflogs, staged/unstaged changes, untracked files, and meaningful ignored files. Inspect relevant unreachable objects before any pruning. Include submodule metadata, nested repositories, shallow/partial clones, alternates, and worktrees whose parent directory is elsewhere. Use bounded, NUL-safe operations for arbitrary path names.

Preserve refs and object reachability before fetching with pruning or deleting anything. Preserve dirty and untracked state separately: a Git bundle does not contain uncommitted files, ignored local configuration, ACLs, or every external object dependency. Do not blindly commit all ignored files; they may be credentials or unrelated data.

Create restore-tested recovery artifacts for unique work. Preserve necessary Git objects and original source-to-commit provenance, plus private filesystem state with necessary modes, symlinks, and metadata. Keep secrets out of remote branches. Recovery bundles/archives are allowed outside the checkout tree; they are not extra live development clones.

Test restoration into an isolated temporary location, verify its relevant content and object connectivity, then remove that task-owned restoration checkout safely. Do not claim an archive is valid because it exists or has a nonzero size.

### 7.4 Enumerate GitHub and all relevant remotes completely

Authenticate using the host's existing authorized Git/GitHub configuration without exposing tokens. Enumerate all pages of repository branches and PRs, including draft PRs and fork-origin PRs. Fetch and compare the exact relevant refs. Enumerate all configured remotes for discovered copies, not just `origin`.

Read each candidate PR's body, commits, diff, issue comments, reviews, inline review threads, unresolved conversations, check runs, statuses, and relevant workflow logs. Include closed/unmerged PRs and historical branches referenced by local recovery records when they contain unaccounted work. Search by content/provenance as well as branch names.

Do not confuse a missing legacy status list with passing check runs. Do not assume a PR body describing older test results certifies its latest head.

Maintain one inventory/disposition ledger with source ID, location/ref/PR, original SHA, unique intent, dirty-state preservation, dependencies, selected integration method, landed SHA, verification evidence, and cleanup eligibility. Stable IDs must survive branch deletion.

## 8. Phase B — integrate all work without losing intent

Review every source relative to current `main` and the other sources. Prefer oldest-first integration when dependencies permit; use dependency ordering when necessary and record why it differs.

Classify each source or coherent change as: integrate directly; adapt to current architecture; already integrated/equivalent; duplicate of another source; superseded with explicit requirement coverage; or blocked with preserved work. Incorrect or obsolete code need not be imported, but its valid requirements must be addressed. Never discard a known defect or requested capability on an effort/ROI argument.

Use ancestry where informative, plus patch equivalence, diff/behavior review, tests, and a source-to-landed-change map. Squash merges and adaptations mean `git branch --merged` is not sufficient proof. Conversely, an empty diff against one branch does not prove all source requirements survived.

Resolve conflicts semantically. Inspect both sides and related invariants; do not take `ours` or `theirs` wholesale to make a merge succeed. Reject stale generated workflows, obsolete auth bypasses, removed providers, test deletions, weakened assertions, and reverted fixes unless a verified replacement exists.

Use existing PRs when suitable. Obtain independent correctness and security review of the exact candidate diff. Address every actionable review finding and re-read feedback after updates. Satisfy protected checks and actual required review rules without bypasses. If an external required approval is unavailable, preserve the prepared branch and report that specific landing blocker; do not self-authorize around protection.

After each landing, validate `origin/main` at the actual new SHA and update the provenance ledger. Continue until every source has an evidence-backed disposition and all correct outstanding work has landed.

## 9. Phase C — safely remove redundant copies and branches

Deletion is a separate, explicitly verified transaction, not a side effect of discovery or a blanket cleanup command.

Before deleting any source, an independent reviewer must confirm:

- Every unique intended change has a verified disposition and, where required, a landed implementation on main.
- Its dirty/untracked/ignored local state is either preserved and restore-tested, migrated safely, or explicitly classified as reproducible disposable output.
- The source is stable, not active, and has not changed since its final snapshot.
- The canonical checkout is healthy and does not depend on the source's common Git directory, alternates, objects, submodule metadata, mounted worktree, or filesystem path.
- No unrelated nested repository, personal data, credential store, or active user process would be removed.
- The exact deletion path/ref is allowlisted, canonicalized, and revalidated immediately before mutation.

The retained canonical checkout must be self-contained. If it is a linked worktree whose primary Git directory would be deleted, relocate/reconstruct the necessary Git administration and object ownership safely before deleting that source. Verify object connectivity, refs, submodule behavior, and operation without the old location.

Remove linked worktrees with Git-aware operations after preservation. Delete ordinary duplicate clones only by reviewed exact path, never a recursive name glob. Never delete `main`, the canonical checkout, its parent directory, unrelated repositories, remote repositories, release tags, user agent profiles, or Docker volumes containing user work. Do not use global Git cleanup or Docker prune commands.

Delete integrated obsolete local and owned remote non-main branches only after exact-head verification. Re-read the remote tip immediately before deletion; if it moved, re-inventory it. Do not delete foreign fork branches or unauthorized protected refs. Close superseded PRs only with a clear linkage to the landed equivalent; do not manufacture a GitHub merged state.

Perform another fresh repo-scan generation. Verify one intended working checkout, canonical `main` synchronized with `origin/main`, no redundant task-owned worktrees/object stores, and no unaccounted local or owned remote non-main branches. Report all actual coverage limits. Initial consolidation is not complete if unique work remains stranded.

## 10. Phase D — establish the real macOS baseline

Now validate the consolidated product, starting with the user-facing installation and workflows rather than only library tests.

Record exact executable paths, versions, source SHAs/build metadata, capsule image digests, agent CLI versions, OS/architecture, Docker engine/context/socket, and relevant non-secret runtime settings. Inspect shell aliases, PATH, Homebrew/source-installed binaries, launch services, broker processes, and existing capsule versions for stale or mixed builds.

Determine the host's actual Docker setup. The reviewed repository requires macOS/OrbStack evidence for the usage-broker lane; generic Linux Docker evidence does not satisfy that specific claim. Do not silently switch Docker context, reset the engine, alter global proxy settings, or remove existing containers to make tests pass.

Use the pinned repository toolchain through `mise`. Read current build/export tasks and produce the real macOS host binary and correct Linux capsule architecture. Validate host/capsule protocol and build compatibility. Test the binaries the operator actually launches, not only `target/debug` while PATH still resolves an old installation.

Capture a before-state inventory of jackin configuration, workspace grants, account references, and running sessions privately. Read-only discovery must not rewrite/migrate host config or trigger account login side effects. Use isolated data/config roots and task-labeled containers for destructive or invalid-state scenarios. Preserve production sessions and restore any intentional temporary changes.

Start with the normal Console and normal account/usage screens. Exercise relevant CLI entry points and the native macOS usage application when present. Record baseline failures with exact reproduction steps; then fix root causes and rerun the same scenarios.

## 11. Phase E — account inventory, identity, and discovery

Build a capability matrix from the current agent registry, provider registry, account schema, adapters, CLI installation support, configuration, UI, and documented promises. Do not equate agent names with billing providers or assume API-key and subscription routes expose the same limits.

For each supported agent/provider/auth mode/account record: discovery source, stable secret-free identity, provider/team/project identity where relevant, launch route, host visibility, quota capability, workspace grant, capsule handoff, in-capsule visibility, refresh/restore behavior, and test status.

Cover the current equivalents of Claude Code, Codex, Amp, Kimi Code, OpenCode, Grok Build, Antigravity, Gemini CLI, Cursor Agent, Muse Code, omp, and Hermes. Cover configured upstream providers including OpenAI, Anthropic, Z.AI, MiniMax, Google, xAI, Meta, Kimi, OpenRouter, and other providers the current product claims to support. Do not invent a native quota API for a multi-provider client.

Verify default host profiles; custom profile directories; supported HOME/XDG overrides; named accounts; registered API-key references; permitted environment/protected-source references; global config; every effective workspace/role configuration; and supported macOS Keychain-backed identities. Discover outside the current working directory. Do not crawl arbitrary browser/password stores or bypass Keychain consent.

Compare production discovery with an independently assembled private expected inventory. The expected list must not be computed by calling the same discovery function being tested. Account for every known local account and every discovered unresolved credential source.

Prove:

- Two distinct accounts for one provider remain distinct, including same display labels, multiple organizations/projects, and multiple subscription tiers.
- One underlying account seen through multiple legitimate source aliases is deduplicated without losing provenance or usable routes. Mere email equality does not authorize merging identities.
- Agent, upstream provider, account, credential source/revision, and API route cannot silently substitute for one another.
- Incomplete/anonymous/invalid sources remain visible as actionable unresolved diagnostics; do not fabricate an authenticated identity or hide them as an empty successful scan.
- Add, remove, rename, revoke, reconfigure, and refresh transitions update membership correctly. Cached history must not resurrect removed accounts.
- Denied, unreadable, malformed, missing, locked, expired, and interaction-required sources produce explicit isolated states, not false zero usage or global discovery failure.
- Discovery is deterministic and read-only, handles concurrent config changes safely, and does not resolve the same protected source repeatedly or cause repeated interactive prompts.
- Multiple profile homes and API-key/subscription credentials cannot mix through ambient process environment, stale config, or provider alias fallback.

For unreferenced nonstandard profile locations that cannot be identified safely, provide honest discovery coverage plus the supported explicit add/import workflow; never claim arbitrary hidden credentials were exhaustively found.

## 12. Phase F — usage semantics, host UI, and refresh

Trace one authoritative account/usage model through provider parsing, canonical broker state, transport DTOs, Console, native FFI/Swift, Capsule, and persistence. Remove lossy projections or duplicate interpretations that allow the surfaces to disagree.

Verify every supported quota window, including short/session windows, daily, weekly, monthly, model-specific/shared pools, reset timestamps, remaining/used values, plan/status, and balances when they are actual quota constraints. Do not replace limits with token estimates or introduce historical spending/pricing dashboards outside this goal.

Keep unlimited, zero remaining, unknown, unsupported, unavailable, stale, loading, and expired distinct. Preserve last-good values on transient failure but mark their age and error. Do not show a stale value as fresh, an unsupported metric as 100% remaining, or a failed request as zero consumption. Keep percentage direction and units explicit.

Test timezone/reset handling, expired/reset rollover, clock movement, missing denominators, zero limits, negative/malformed values, and multiple pools without conflating their meanings. Use authoritative provider observations, timestamps, and documented tolerances when comparing changing live values; do not require two time-separated requests to be numerically identical.

In actual host screens verify full inventory, account switching, correct selection identity after sorting/reordering, detail windows, all available quota buckets, provider/model labels, recency/errors, manual refresh, initial refresh, background refresh, and persistence after restart. A compact seven-provider glance can remain a summary only if all supported accounts are reachable in the full experience and the summary does not claim completeness.

Exercise keyboard navigation, mouse/trackpad scrolling, focus, resize, narrow terminals, long/Unicode labels, large account counts, loading/error/empty states, and native UI accessibility. Check real rendered output and interactions, not only generated DTOs. Snapshot acceptance must be reviewed against correct behavior, not automatically updated to bless a regression.

Verify broker behavior under simultaneous Console/native/Capsule clients: bounded concurrency across distinct accounts, single-flight refresh for the same canonical account, coherent generations, shared rate-limit deadlines, cancellation, reconnect, owner loss, and timeout ownership. Forced/manual refresh must not bypass safety backoff or launch duplicate provider calls.

## 13. Phase G — authenticated production capsules

Trace selected account bindings end to end:

```text
host source -> canonical account/provider/route -> workspace grants ->
launch plan -> instance credential staging/capabilities -> real Docker image ->
Capsule setup -> agent process environment/home -> authenticated operation ->
host broker usage -> scoped relay -> Capsule account and usage UI
```

Test normal Console launch and applicable `jackin load` paths. Use real constructed role images and the production `jackin-capsule` PID 1, real attach/PTY/session machinery, real setup code, and production relay transport. A Python socket stand-in, mock Docker client, `--version`, or empty shell container does not satisfy this acceptance path.

Use a dedicated verification workspace with explicit account grants. To exercise all available host accounts inside a capsule, explicitly grant the eligible accounts to that verification scope; do not silently broaden the grants of existing user workspaces. Host inventory is global. Capsule inventory is all accounts authorized for that capsule, not all host secrets.

For each locally available supported account, verify selected provider/account/route identity, successful credential preparation, expected private home/file permissions, correct container CLI/config, and a provider-authenticated non-generative identity/status/usage operation where supported. Verify that the account is truly accepted, not merely that a credential file exists. Respect the exact-model restriction for any inference smoke test.

Test multiple accounts for one agent concurrently in separate tabs/sessions and capsules; multiple providers; account selection from zero/one/many candidates; starting a new tab; switching focus; detach/reattach; container stop/start; broker restart; host application restart; session restore; and relevant schema/credential revision migrations.

Account choices must persist explicitly. A selected account cannot silently fall back to the host default, another source, another provider endpoint, or another team after refresh, restart, or restore. Failed preparation must not expose a partially prepared session as authenticated.

Verify that all authorized accounts and their available limits can be inspected from inside the capsule, including accounts not currently running in the focused tab. Compare scoped Capsule views with the corresponding host account snapshots. Do not collapse multiple accounts into a single provider row.

Test token expiry/rotation, revoked grants, changed credential material, interrupted staging, simultaneous launches, and stale restored sessions using isolated fixtures or safely issued test credentials. Never revoke or overwrite the user's real credentials to create a test case. Define synchronization/conflict ownership for providers with rotating credentials so concurrent capsules cannot overwrite newer state.

Test negative boundaries: foreign account capability, stale/forged capability, cross-workspace request, unauthorized source alias, host catalog read, path traversal/symlink substitution, overly broad environment inheritance, and secret leakage in logs, errors, process arguments, image layers, Docker metadata, snapshots, and telemetry. Do not fix authentication by exposing the entire host home, all credentials, or the host Docker socket to agents.

Verify Linux-in-container details independently from macOS host behavior: correct CPU architecture and binaries, shell availability, paths, writable directories, permissions, network/DNS/proxy behavior, certificates, subprocess environment, and agent installers. Correct failures structurally without disabling TLS checks or isolation.

## 14. Phase H — make the tests prove product behavior

Maintain a requirement-to-test-to-evidence matrix. Every important acceptance behavior must have an explicit scenario, independent oracle, expected failing condition, automation layer, and result at an identified source/build revision.

Use complementary layers:

1. Pure Rust unit/property tests for identity, account/provider/route binding, quota parsing, source precedence, generation transitions, permissions, and error semantics.
2. Deterministic adapter/contract tests using sanitized fixtures and local protocol servers for authentication/usage responses, 401/403/429, malformed/partial payloads, expiry, and transport failures.
3. Real process integration tests for host discovery, broker ownership, config publication, source changes, relay authorization, and concurrency.
4. Real Docker integration tests that execute the built production Capsule and normal launch/setup/attach paths, using deterministic test credentials/provider boundaries where suitable.
5. Real UI interaction/render tests for Console, Capsule, and relevant native macOS surfaces.
6. Controlled host acceptance against actually available accounts, authorized provider status/usage APIs, the installed binaries, and the actual Docker environment.

Prefer Rust for new harnesses and repository-native tooling. Keep test fixtures isolated from the user's HOME, XDG state, Keychain, provider profiles, and Docker resources. Avoid racing process-global environment mutation; use injected environments or separate child processes. Fixture cleanup must delete only resources created by that fixture.

The test matrix must cover at least:

| ID | Scenario and required assertion |
|---|---|
| A01 | Global/default/custom-source discovery matches an independent expected inventory. |
| A02 | Same-provider multiple accounts and same-label accounts preserve distinct identities. |
| A03 | Equivalent aliases deduplicate without provider/source/route substitution. |
| A04 | Add/remove/rename/revoke/rescan transitions and stale history are correct. |
| A05 | Denied/missing/malformed/interaction-required sources remain explicit, isolated diagnostics. |
| U01 | Every supported usage window and quota pool survives all projections. |
| U02 | Unknown/stale/unavailable/unlimited/exhausted/loading are distinct. |
| U03 | Reset/timezone/clock/percentage/unit boundary cases are correct. |
| U04 | Full host account inventory is not restricted by a compact provider list. |
| U05 | Account switching and refresh preserve row identity under reordering. |
| B01 | Two and twenty same-account clients share one authorized provider refresh per generation. |
| B02 | Distinct accounts run concurrently within bounds without state leakage. |
| B03 | Owner loss, timeout, rate limiting, cancellation, and restart preserve coherent ownership/state. |
| C01 | Normal product launch creates the production Capsule and a usable real PTY session. |
| C02 | Each available account's intended provider accepts its forwarded authentication. |
| C03 | Two accounts for the same agent can coexist without home/env/credential cross-contamination. |
| C04 | Capsule displays all authorized accounts, not only the active tab account. |
| C05 | Host and Capsule agree on the same account's quota snapshot and generation. |
| C06 | Detach/reattach, new tabs, stop/start, and session restore preserve account bindings. |
| S01 | Unauthorized/stale capabilities and cross-workspace sources fail closed with zero forbidden provider calls. |
| S02 | Staging/config-publication interruption and symlink attacks cannot publish mixed identity/secret state. |
| S03 | Logs, snapshots, telemetry, Docker metadata, and artifacts reveal no credential material. |
| T01 | Keyboard/mouse/trackpad/resize/native UI interactions work on real rendered screens. |
| V01 | A missing executable, Docker daemon, expected test case, or required artifact cannot produce a passing acceptance gate. |
| V02 | Targeted regressions fail on the broken implementation and pass on the corrected implementation. |
| V03 | Final-main installed binary and capsule image correspond to the verified source/builds. |

Demonstrate the red/green property for each discovered defect. Run its new regression against the pre-fix implementation or a controlled reversion/mutation that restores the failure. For already-integrated historical fixes, use an isolated reconstructed baseline. Never mutate the shared live checkout underneath another worker.

Use targeted mutations for critical assertions: drop a quota bucket, swap two account bindings, collapse account rows, substitute a default credential, sever the production relay, disable provider polling, bypass a capability check, and restore stale membership. The corresponding test must fail for the intended reason, not because the project no longer compiles. Do not commit deliberate mutants.

A test asserting only exit zero, file existence, a mock invocation, or a snapshot generated by the same faulty mapper is insufficient for the behavior it claims to prove. Test the data path and the rendered/interactive result with independently specified expectations.

Inventory tests that are ignored, feature-gated, profile-filtered, cfg-excluded, swallowed by early returns, or never selected. Separate helper child-process tests from acceptance scenarios; a no-op helper cannot inflate acceptance counts. Audit retries/flakes instead of accepting a lucky rerun.

Mandatory suites must fail when prerequisites are missing or zero expected scenarios execute. Record enumerated versus executed scenario names, expected counts, failures, skips, filters, and flaky outcomes. A JUnit file with only passing unrelated tests is not proof that a required scenario ran.

## 15. Phase I — correct generated CI and validate exact artifacts

Read the current generated workflow and generator/config semantics. Build an actual coverage map for Rust, native Swift/FFI, Docker images/runtime, provider fixtures, UI, and macOS/OrbStack gates. Compare this with documented requirements and protected checks.

Fix required coverage through the owned Velnor generation/configuration path, not hand edits to generated output. Use the current generator; do not restore obsolete workflows merely because an old PR contains them. If a required capability is missing from the generator, prove the gap, implement/test the necessary narrowly scoped upstream support with the same model policy where authorized, or report the concrete integration blocker. Do not weaken the jackin acceptance contract to fit a limited generator.

Make relevant source changes schedule the necessary regression layers. Native/FFI changes must not be certified solely by Ubuntu Rust tests; Docker/setup changes must not bypass real Capsule tests. Verify generated output is current and regeneration is clean. Update stale commands/docs alongside corrected behavior.

Do not cache external host/live-account acceptance as though it were a timeless pure unit-test result. Any reuse must be valid for the exact source closure, toolchain, features, platform, test contract, and artifact. The final host acceptance must run against the final installed build and current environment.

The reviewed repository documents these commands; inspect current help and configuration before using them and correct documented drift rather than inventing success:

```sh
mise install
cargo nextest run
cargo nextest run --all-features
cargo nextest run -p jackin-usage -p jackin-usage-ffi
cargo nextest run -p jackin-capsule -p jackin-console
cargo xtask ci --fast
cargo xtask ci
cargo xtask ci --e2e
cargo nextest run -p jackin --features e2e --profile docker-e2e
mise run desktop-ci
mise run desktop-merge
```

Prepare/export the actual capsule build with the current supported build task before Docker tests. Inspect `TESTING.md` for the supported `build-jackin-capsule --export` or PR-sync path, nextest profiles, JUnit locations, and native UI prerequisites. Follow current debug/redaction conventions for manual product verification.

Run full applicable formatting, Clippy, tests, configuration/migration contracts, security/dependency checks, generated-workflow validation, docs, UI, and E2E gates. Preserve strong assertions and required protections. Performance requirements must be achieved by removing redundant work and correcting caching/selection, not dropping scenarios or reclassifying required tests as optional.

## 16. Evidence, independent sign-off, and final cleanup

Use explicit result states per requirement: PASS with evidence; FAIL; BLOCKED with a demonstrated external cause; or NOT APPLICABLE with a verified capability reason. Missing credentials are a limitation of live coverage, not proof that an adapter works or does not work. "Unsupported" must follow verified provider/product capability, not inability to find an endpoint on the first attempt.

Record evidence at the actual commit/build under test: command, non-secret environment facts, binary/image identity, test/scenario names, timestamps, results, red/green regression proof, and artifact references. Keep credentials, account identifiers, private path inventories, and raw recordings in the private evidence area. Publish only redacted summaries permitted by repository policy. Do not commit raw screenshots/logs when repository rules prohibit it.

Independent final reviewers must verify: every source disposition; cleanup safety; account/credential isolation; correctness of usage semantics; actual production-path coverage; negative tests; exact-model policy; generated CI; and final installed artifact provenance. An implementer's status message is not an acceptance artifact.

After fixes land, revalidate the resulting `origin/main`, install/build from that verified result, and rerun the affected final host acceptance scenarios. An unrelated later commit still requires an explicit equivalence argument for reusing earlier results; never stamp new evidence with a SHA that was not actually tested.

Remove temporary verification branches/worktrees/clones and only task-created disposable test resources after their work is safely accounted for. Keep useful private recovery archives. Do not stop user sessions or erase persisted user agent state. Return the canonical checkout to clean `main`, synchronized with `origin/main`.

Repeat machine-wide repo-scan with a fresh generation and re-enumerate owned remote heads/PRs. Reconcile anything created or changed during the task. Verify exact canonical path identity and self-contained Git storage again. Do not claim all branches are gone when a concurrent new branch, an unresolved protected ref, or unmerged unique work remains.

## 17. Definition of done

Do not mark this goal complete until all of the following are true:

- The exact coordinator/subagent model and effort were enforced throughout with verifiable configuration and no fallback.
- Fresh discovery has no unaccounted accessible jackin copies, worktrees, relevant object stores, branches, or unique local state; coverage gaps are not disguised as completeness.
- Every local/remote/PR work item has an evidence-backed disposition and all correct outstanding work is integrated into main.
- Only `/Users/donbeave/Projects/jackin-project/jackin` remains as the intended development checkout, with independent healthy Git storage, clean synchronized `main`, and no obsolete owned non-main branches.
- Every supported account source actually available to this user is accounted for, including unresolved sources with explicit diagnostics.
- Global host account/usage screens show the full supported inventory and correct quota semantics, without hidden seven-provider or one-account limits.
- Actual production capsules launch normally, preserve intended authentication/account/provider bindings, and expose correct scoped usage for every available authorized account.
- Multi-account concurrency, refresh, reconnect, restart, and restore are verified, and forbidden cross-account access fails closed.
- Required deterministic, process, production Docker, UI, and applicable macOS/OrbStack tests executed rather than skipped, and critical regressions demonstrated red/green behavior.
- Generated CI actually covers required behavior; protected checks pass on the final landed main revision, and final installed host/capsule artifacts are identified and verified.
- Independent correctness/security/verification reviews have no unresolved actionable findings, and no credential or user-data loss occurred.

If an external condition provably prevents an item, leave the goal incomplete, preserve the affected sources, and identify the exact unverified behavior and necessary external action. Continue all remaining feasible work. Never convert a proven limitation into a claim of full product readiness.

## 18. Final report

Return a factual completion report with:

1. Final main SHA, installed host binary identity, Capsule image digest/build, and host/Docker environment.
2. Discovery coverage; before/after counts; integrated PRs/branches; adaptations/equivalence proofs; private recovery location; exact cleanup performed; any retained exceptions.
3. Defects reproduced, their architectural root causes, fixes, landed commits, and regression-fails-before/passes-after evidence.
4. Per-agent/provider/account-capability verification matrix, clearly separating fixture, production-container, live-auth/usage, UI, and any exact-model inference evidence.
5. Executed test suites/scenarios and exact-head CI results, including all skips, filters, flakes, unmet prerequisites, and negative-test outcomes.
6. Independent review results, remaining genuine external blockers, and an honest completed/incomplete verdict against the definition of done.

Do not report only that tests are green. Show that the user's actual account, usage, authentication, and Capsule workflows work on this macOS host, and that the tests would catch them breaking again.
