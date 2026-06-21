# Skeptical Review — PR #576 (`chore/launch-speed-roadmap`)

**Title:** instant-launch fast-path + UID-agnostic images + prewarm/diagnostics CLI
**Size:** +17,779 / −2,642 across 121 files, 278 commits
**Base:** `main`
**Reviewer stance:** adversarial — looking for what is wrong, risky, or over-built, not what works.

> **Overall verdict:** The code itself is competent and unusually well-instrumented. The *packaging* is the headline problem: this is an unreviewable mega-PR that fuses at least five independent workstreams, and it ships several concrete correctness and security holes — including a re-introduction of the very bind-mount race the PR fixes elsewhere, and an incomplete `--rebuild` fix that re-opens a bug a prior commit claimed to close.

Findings are grouped: **Merge blockers** → **Strong** → **Design / maintainability** → **Process** → **Confirmed-OK**. Each finding has: *What*, *Why it matters*, *Evidence*, *Fix*.

---

## Merge blockers

### B1. `--rebuild` still bypasses the build via a second, ungated fast-path

- **What:** `jackin load <role> --rebuild` is supposed to force a fresh image build. Commit `31840abe` ("honor `--rebuild` instead of taking attach/start fast paths") added a `&& !opts.rebuild` guard — but only to the *early* restore gate. A *second* restore decision exists further down and is not guarded, so `--rebuild` against a **running** current-role container still attaches the stale container and never builds.
- **Why it matters:** `--rebuild` is the operator's escape hatch when an image is suspected stale/broken. Silently honoring it as "attach to the old thing" defeats the one command they reach for when something is wrong. The prior commit's message implies the bug is fixed; it is not, which is worse than an open bug because nobody is looking.
- **Evidence:** `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs:527-571`. Under `--rebuild`, `early_restore_container` is `None`, so control reaches the second block at line 532 (`resolve_restore_candidate`), whose `AttachCurrentRole` (line 545) and `StartCurrentRole` (line 555) arms `return` early. `resolve_current_restore_candidate` (`launch.rs:2208-2227`) returns `AttachCurrentRole` for any running current-role container regardless of `rebuild`. No regression test covers running/stopped/missing current-role container under `--rebuild` — the commit message itself flags this as an unwritten follow-up.
- **Fix:** Gate the second block too. Pass `rebuild` into `resolve_restore_candidate` so it returns `RecreateCurrentRole`/`StartFresh` (never `Attach`/`Start`) when `rebuild`, or wrap the line-532 arm in `if !opts.rebuild`. Add the three-state regression test (running / stopped / missing × `--rebuild`).

### B2. Shared `extrausers/passwd` re-introduces the exact bind-mount inode race the PR fixes for auth files

- **What:** To run containers as the host operator's UID, the runtime writes a passwd line (`agent:x:<uid>:0:...`) to a **single shared** host file `~/.jackin/extrausers/passwd`, then bind-mounts it read-only into **every** container. The write is an unconditional `write tmp` + `rename` on every launch, which swaps the file's inode.
- **Why it matters:** On macOS, renaming a single-file bind-mount source invalidates the live mount inside an already-running container. This is *precisely* the failure mode commit `f1454e3a2` diagnosed and fixed for single-file auth mounts ("on macOS that rename invalidates a live single-file bind mount into the running container") — and the author added a no-churn guard *there* but not here. Because the passwd content is provably constant for a given host UID, the rename is pure churn. A second launch while a first container runs can swap the passwd inode out from under the live `:ro` mount → `getpwuid` breaks in the running container → lost `$HOME`/user identity → git/ssh/CLI breakage.
- **Evidence:** `crates/jackin-runtime/src/runtime/launch.rs:927-928` (write + rename), `:967` (`:ro` mount). Contrast with the guard added in `auth.rs:983-1006`. The inline comment here even claims atomicity protects against torn reads but is silent on the inode-swap-under-live-mount problem fixed one commit earlier.
- **Fix:** Apply the same no-churn guard: `if fs::read_to_string(&path).is_ok_and(|c| c == line) { skip rename }`. A per-container temp name does not help — the shared *destination* is what gets its inode swapped.

### B3. Containers run with primary group 0 (root group) — undocumented privilege widening for an untrusted sandbox

- **What:** The new `docker run --user` string is `format!("{uid}:0")` — every agent process runs with **primary GID 0 (the `root` group)**.
- **Why it matters:** A jackin agent container is an untrusted-code-execution sandbox. Many Debian files are group-`root` and group-readable/writable; with GID 0 the agent gains access to every group-0-writable path in the image. The OpenShift arbitrary-UID pattern does use GID 0, but only paired with a *hardened* image where group-0 grants nothing beyond the home tree. This PR normalizes `/home/agent` and `/jackin/default-home` to `g=u` but does **not** audit the rest of the filesystem for group-0-writable sensitive paths. This is the single most security-relevant choice in the PR and it ships with no written rationale or tradeoff note.
- **Evidence:** `crates/jackin-runtime/src/runtime/identity.rs:34`. Home-tree normalization in commit `6f1c9d99c` and the group-0 derived-image layer, but no FS-wide audit.
- **Fix:** At minimum, document why GID 0 is acceptable for an untrusted-agent container. Better: use a supplementary group rather than primary GID 0, or prove (in a test) that no group-0-writable sensitive paths exist in the construct image.

### B4. Adopted prewarm DinD sidecar is orphaned on every early error between adoption and cleanup-arming

- **What:** When a prewarmed DinD sidecar exists, launch *adopts* the running container + network + certs volume and immediately deletes the on-disk state record (so nothing else can re-adopt them). But the cleanup handler that would tear these down is not armed until ~150 lines later. Any error in between leaks a live container/network/volume with no state record pointing at it.
- **Why it matters:** One of the `?` exits in the gap is `verify_github_token_present?` — a *routine* operator error (expired/missing token), not an exotic edge case. So a common, expected failure leaks Docker resources every time, with no record to clean them up later. Over a workday of bad-token launches this accretes orphaned DinD containers.
- **Evidence:** Adoption at `launch_pipeline.rs:1084`; state deletion via `remove_prewarmed_dind_state` (`launch_dind.rs:515`); cleanup armed at `launch_pipeline.rs:1234`. Fallible exits in the gap: lines 1128, 1138, 1166, 1214, 1231. `AdoptedDindSidecar` has no `Drop` (`launch_dind.rs:41-44`).
- **Fix:** Give `AdoptedDindSidecar` a `Drop` that tears down the adopted resources, or fold the adopted names into `LoadCleanup` immediately after adoption (before any fallible step), with `disarm()` on the success handoff.

---

## Strong findings

### S1. New auth + DinD overlap is not cancellation-safe (and leaks on overlapped failure)

- **What:** The PR's headline "less serialized" win overlaps auth/env prep with sidecar/DinD startup using a bare `tokio::select!`. That select does **not** race the cancel token, so Ctrl+C is ignored until both branches finish. Two follow-on lines (`seed_codex_project_trust?` and a `tokio::join!` over sidecar+materialize) also now leak a started sidecar / staged host worktrees on failure, because what was harmless when serial is no longer harmless when overlapped.
- **Why it matters:** Cancellation responsiveness is an explicit goal of this very PR (commit `58803d57` fixed exactly this for `docker build`). The new parallel section did not get the same treatment, so a long keychain/`gh` shell-out (tens of seconds) ignores Ctrl+C. The leak paths are now the *common* failure path, not an edge.
- **Evidence:**
  - `launch_pipeline.rs:1337-1343` — bare `select!`, no `while_waiting`/cancel-token race; wraps `spawn_blocking(RoleState::prepare_for_agents)` (shells to `gh`, macOS `security`, file copies).
  - `launch_pipeline.rs:1363` — `seed_codex_project_trust(&state, workspace)?` returns `Err` without `cleanup.run()`; the sidecar is now already started (overlapped), so it leaks.
  - `launch_pipeline.rs:1470` — `tokio::join!(sidecar_wait, materialize_wait)` does not short-circuit; on sidecar failure, `materialize_workspace` still runs `git worktree add` on the host, and `LoadCleanup` never removes worktrees.
- **Fix:** Wrap the `select!` (or each future) in `steps.while_waiting(...)` so the cancel token aborts promptly. Route `seed_codex_project_trust` failure through `cleanup.run`. Use `tokio::try_join!` (or check sidecar result first) and/or extend `LoadCleanup` to unstage worktrees it staged.

### S2. `diagnostics compare` makes statistical claims it cannot back

- **What:** The diagnostics `compare` subcommand prints deltas like `"+4.1s, 5.6x slower"` from **single-run** point measurements (N=1 vs N=1). There is no run-count concept, no median/mean/variance, and no warning that one cold-vs-warm delta is dominated by noise. Where a stage *does* have multiple samples, the code reduces them with **max**, not a central tendency.
- **Why it matters:** The stated purpose is to "prove whether a launch change moved work out of the foreground path." You cannot prove that from one sample per condition — disk cache, Docker daemon state, and network jitter dominate a single launch. The tool will confidently print "5.6x slower" off two noisy samples, and the docs amplify the false confidence with the word "prove."
- **Evidence:** `crates/jackin/src/cli/diagnostics.rs:582-599` (single `startup_duration_ms`), `:777-809` (`format_startup_delta` emits ratios off two samples), `:1209-1211` and `:1310-1320` (`max_duration` / per-sample flattening). `docs/content/docs/commands/diagnostics.mdx` repeats the "prove" framing.
- **Fix:** Either accept multiple artifacts per condition and aggregate (median + spread, label conditions not files) with a sample-count/variance column, or drop the ratio/"Nx slower"/"prove" language and present single-shot numbers as explicitly anecdotal.

### S3. prewarm has an unbounded build fan-out — self-inflicted DoS

- **What:** `prewarm --image --all-roles` spawns an unbounded `JoinSet` over every role, and each role task spawns another unbounded `JoinSet` over every supported agent. So it launches *R roles × A agents* full `build_agent_image` runs concurrently.
- **Why it matters:** On a box with a dozen roles this is disk blowout, Docker daemon thrash, and OOM — a footgun triggered by a single innocuous-looking flag. There is no `--concurrency`, no semaphore, no disk precheck, and no warning in `--help` or docs.
- **Evidence:** `crates/jackin/src/cli/prewarm.rs:444-467` (outer unbounded `JoinSet`); `crates/jackin-runtime/src/runtime/image.rs:224-242` (inner unbounded `JoinSet` over agents). `--all-workspaces` dedups roles but does nothing about build fan-out.
- **Fix:** Bound the fan-out with a semaphore (small default, e.g. 2–4) and/or add `--concurrency`. Surface estimated disk cost before an `--all-*` build.

### S4. Inconsistent partial-failure handling across prewarm phases

- **What:** Three sibling prewarm phases use three different failure policies. Agent binaries collect per-agent errors and continue (soft). Images abort all remaining roles on the first `rows?` error (hard). Role repos collect-all then hard-exit.
- **Why it matters:** prewarm's entire value is "fill as many caches as possible before launch." Aborting the remaining work on the first failure (the images path) is the wrong default and is inconsistent with the binary path two screens above it. One bad role image throws away visibility into the roles that succeeded.
- **Evidence:** `prewarm.rs:118-136` (binaries, soft), `:475-480` (`print_image_prewarm_rows(rows?)?` aborts siblings), `:324-328` (roles, soft-collect/hard-exit).
- **Fix:** Make images collect-all-then-report like the other two; never let one role's `rows?` swallow siblings' output.

### S5. prewarm attributes task-panic failures to the wrong agent

- **What:** When a spawned prewarm task panics or is cancelled, `join_next()` yields a `JoinError` whose payload doesn't carry the agent. The error path hardcodes `Agent::Claude` as the culprit.
- **Why it matters:** A panic in (say) the Kimi binary prewarm is reported and sorted as a **Claude** failure — actively misleading during exactly the debugging session where you most need the truth.
- **Evidence:** `prewarm.rs:655-663` (`agent: Agent::Claude` hardcoded in the `Err(error)` arm of the JoinSet join).
- **Fix:** Key the JoinSet by agent (or carry the agent in an `AbortHandle` map) so the error names the real agent; at minimum print "unknown agent (task failure)" instead of a specific wrong name.

---

## Design / maintainability

### D1. Image hash label is documented as authoritative but the code doesn't treat it that way

- **What:** `naming.rs` documents the recipe-hash label as "the fast-path authority: when the local image's hash matches the current recipe, launch can reuse it." But `classify_image_labels` still runs `recipe_label_mismatch` *after* a hash match, which rebuilds the image if any *diagnostic* label is missing or edited.
- **Why it matters:** Diagnostic labels are derived from the same recipe the hash covers, so this second pass can never legitimately disagree with the hash — it only fires on externally mutated images, converting a hash *hit* into a needless rebuild. It also runs a per-launch label walk for no correctness gain, and contradicts the documented contract (two sources of truth).
- **Evidence:** `crates/jackin-runtime/src/runtime/image.rs:685-689` (hash match still calls `recipe_label_mismatch`), `:698-711` (mismatch logic), vs `crates/jackin-runtime/src/runtime/naming.rs:67-71` (hash = authority).
- **Fix:** On hash match, `return None` (reuse) directly. Only fall into `recipe_label_mismatch` on the *miss* branch to produce a precise human-readable reason.

### D2. `rebuild` recomputed three times; `cache_bust` semantics drift between decision-time and build-time recipes

- **What:** The `rebuild` flag is recomputed at least three times along the build path, and the `cache_bust` value feeding the recipe hash is computed by two *different* ladders at decision time vs build time. They reconcile only because `store_cache_bust` is called (as a side effect) before the hash is computed.
- **Why it matters:** This is a fragile, side-effect-ordered invariant spread across functions. Reorder `store_cache_bust` after `build_image_recipe`, or add a build branch that mints a timestamp without persisting it, and the *next* launch computes a different expected hash → **permanent rebuild loop**. None of the decision tests exercise a cache-bust role, so this would ship silently. The consumer also re-flattens the decision enum in two `match` blocks with `unreachable!()` arms — a smell that the enum is being split and rejoined rather than carrying the right shape.
- **Evidence:** `image.rs:271` / `:1852` / `:1802` (three `rebuild` recomputations); `:608-618` (decision-time `cache_bust_recipe_value`) vs `:1804-1820` (build-time ladder, `store_cache_bust` at `:1816`); consumer `unreachable!()` at `launch_pipeline.rs:946,994`.
- **Fix:** Extract one `cache_bust_for_recipe(paths, image, manifest, rebuild)` used by both sites so build and decision cannot drift. Resolve `rebuild`/base-image once in `decide_role_image`. Collapse the consumer's two matches into one exhaustive match without `unreachable!()`. Add a test: build with `rebuild=true`, then assert the next `decide_role_image` returns `Reuse`.

### D3. `load_role_with` is a 1,687-line function

- **What:** The core launch orchestration function spans `launch_pipeline.rs:139-1826`.
- **Why it matters:** Findings B4, S1 are direct symptoms — at this size, tracking which `cleanup` is armed across dozens of `?` exits is not humanly reliable, and an inner `async { … }.await` block moves `cleanup` so the outer `Err` arm can't run it. The function is the single biggest reason this PR is hard to review and easy to leak resources in.
- **Evidence:** `launch_pipeline.rs:139-1826`; inner block / outer `Err` arm at `:1794`.
- **Fix:** Extract restore-decision, credential-resolution, sidecar/auth-overlap, and runtime-launch phases into named functions, each owning its cleanup scope. Prefer RAII/guard cleanup over manual `cleanup.run()` at every exit.

### D4. Dead/speculative code shipped in a "launch speed" PR

- **What:** Two distinct cases of code that no caller exercises:
  - `shared_runner.rs` (`SharedCommandRunner`) is entirely `#[allow(dead_code)]` on both the type and `new`; nothing constructs it outside its own test.
  - `selected_agent_version: Option<String>` is added to two decision-enum variants, threaded through the whole decision tree, and pattern-matched at its only real consumer as `selected_agent_version: _` — discarded. Its sole live use already receives it as a separate argument.
- **Why it matters:** YAGNI. Speculative infrastructure pads an already-huge diff, widens types, forces every `match` arm to name a dead field, and invites readers to believe it feeds logic it does not.
- **Evidence:** `crates/jackin-runtime/src/runtime/shared_runner.rs:13-32`; `image.rs:104-124` (field added), `:417` (populated), `launch_pipeline.rs:939,943` (`: _` discarded), `image.rs:739` (real use, separate arg).
- **Fix:** Hold `shared_runner.rs` until the branch that uses it. Drop the enum field; pass the label value straight into `emit_image_reuse` where it's consumed.

### D5. Arbitrary-UID edge cases unhandled and untested

- **What:** The `--user <uid>:0` + extrausers scheme has unhandled boundary conditions: euid 0 (root), euid 1000 (collision with the baked `agent` user), the empty-but-never-written `extrausers/group` file, a debug log that lies about the socket-dir mode, and an unvalidated macOS assumption.
- **Why it matters:** Several are silent no-ops that "work by luck": euid 1000 collides with the baked `agent:x:1000`, NSS `files` source wins, extrausers is shadowed → mechanism does nothing but appears to work. euid 0 → `agent:x:0:0` shadows root semantics, `$HOME` silently reverts to `/root` for direct `getpwuid` callers. The whole scheme's premise ("this process owns every bind-mount source") is dubious under Docker Desktop on macOS, where the VM remaps ownership — asserted in comments, never validated.
- **Evidence:** `identity.rs:34,46` (no guard for euid 0 / 1000); `docker/construct/Dockerfile:546-547` (`: > extrausers/group` created empty, never populated); `launch.rs:974` debug log claims `0o700` after the revert removed the perm-set; comments cite macOS 501 as motivation without validating Docker Desktop VM ownership.
- **Fix:** Short-circuit euid==0 and euid==1000 explicitly (skip extrausers when it would collide or run as root). Either wire up `extrausers/group` symmetrically or drop it and comment that group resolution rides baked `/etc/group` (GID 0 only). Correct the socket-dir log. Add a real DinD assertion that a file created `0600` by UID 1000 in `/jackin/default-home` is readable by a container running `--user <other-uid>:0`.

### D6. Untrusted branch name injected raw into a Docker image label

- **What:** `role_source_ref` is set to the branch override verbatim and emitted as `--label jackin.recipe.role_source_ref=<branch>`. The image *tag* sanitizes the branch (replaces `/`, lowercases); the label value does not.
- **Why it matters:** A branch containing whitespace/control characters flows unsanitized into BuildKit label metadata. Because builds use argv (not a shell), this is a metadata/parse nuisance rather than RCE — but it's an unsanitized-input smell the PR's own tag code already guards against, i.e. an inconsistency that will eventually bite.
- **Evidence:** `image.rs:520` (recipe field), `:801-804` (diagnostic label), `:768-778`/`:1838` (emitted as label); contrast `naming.rs:130-133` (tag sanitization).
- **Fix:** Store the sanitized slug or a hash in the label, matching the tag. Add a test that a `role_source_ref` with `/` produces a stable hash.

### D7. Silent error → rebuild fallbacks can mask a degraded Docker daemon

- **What:** Both `list_image_tags` and `inspect_image_labels` failures are swallowed to a `debug_log!` and treated as "rebuild." `published_image_freshness` treats every pull/inspect failure as `Stale`.
- **Why it matters:** If the Docker daemon is intermittently degraded, every launch silently downgrades to a full rebuild instead of surfacing "your Docker is unhealthy." The operator sees an endless rebuild storm with no visible cause unless they set `JACKIN_DEBUG=1`.
- **Evidence:** `image.rs:294-308` (`ImageListFailed`), `:400-414` (`InspectFailed`), `:2363-2378` (freshness → `Stale` on any failure).
- **Fix:** Emit a compact, always-on (`clog!`) warning when label/tag inspection fails, so the rebuild-storm root cause is visible without debug mode.

### D8. Test gaps on the highest-risk paths

- **What:** New tests cover happy-path fast-path selection and one sidecar-create failure, but there are zero tests for the paths this review flags as risky.
- **Why it matters:** The single existing cleanup test only exercises the path that *does* call `cleanup.run`, giving false confidence. The dangerous behavior — cancellation, `--rebuild`, adopted-prewarm orphaning, overlapped-failure leaks, cache-bust reconciliation, the dual-recipe published-fresh reuse path, `RefreshInBackground` — is untested.
- **Evidence:** `launch/tests.rs` (happy-path only; no `cancel_token`/`LaunchCancelled`, no `--rebuild`-vs-container, no adopted-prewarm-orphan, no sidecar-fail-with-materialize-leak). `image/tests.rs` reuse tests only cover single-recipe `base=None`; no dual-recipe `Reuse` or `RefreshInBackground` decision test. prewarm tests cover only arg resolution and `should_*` predicates — no fan-out, failure-aggregation, or mis-attribution coverage.
- **Fix:** Add: (a) inject `verify_github_token_present` failure after prewarm adoption, assert adopted DinD removed; (b) fire cancel token during auth/DinD overlap, assert prompt `LaunchCancelled` + full cleanup; (c) `--rebuild` against a running current-role container asserts a build occurs; (d) `decide_role_image` cases for fresh-published+workspace-labeled → `Reuse` and stale-published+matching-local → `RefreshInBackground`.

### D9. `extrausers/passwd` written world-readable; no-churn guard fails open on non-UTF-8

- **What:** Two smaller hygiene issues around the identity files. The passwd line is written with bare `std::fs::write` (default umask → `0o644`, world-readable) while sibling credential files use `0o600`. The auth no-churn guard uses `read_to_string`, so any non-UTF-8 or transient read failure makes `is_ok_and` return `false` and falls into the rename branch — the exact inode-swap it exists to prevent.
- **Why it matters:** The passwd content is low-sensitivity (UID isn't secret), so this is hygiene, not a credential leak — but it silently diverges from the codebase's private-file discipline, sitting among `0o600` files. The guard "fails open into the failure it guards against," which is the worst failure direction for a guard.
- **Evidence:** `launch.rs:927` (`std::fs::write`, no mode); `instance/auth.rs:993-995` (`read_to_string(target).is_ok_and(...)`).
- **Fix:** Write passwd `0o600` (or `0o644` with a comment justifying public). Compare bytes (`std::fs::read` vs `content.as_bytes()`); prefer in-place rewrite-if-changed (truncate+write same inode) so a real content change also doesn't stale the mount.

### D10. "Skip rename on unchanged sync" narrows the race but does not close it

- **What:** The guard from `f1454e3a2` correctly stops *unchanged* re-syncs from swapping the inode. But the named root cause is that the background sibling-auth prewarm races `docker create`. When host auth content *does* change between foreground provision and the prewarm re-run, the guard doesn't apply, `write_private_file` renames, and the live mount stales as before — just in a rarer window. The regression test only pins the unchanged path, implying the race is gone.
- **Why it matters:** A fix that narrows a race while a test asserts the happy path reads as "fixed" in review but still fails in production under auth rotation. Also: the foreground `prepare_for_agents` already provisions the full agent set, so the fire-and-forget `spawn_sibling_auth_prewarm` (whose `JoinHandle` is dropped — never awaited or cancelled) is redundant re-provisioning that *causes* the churn.
- **Evidence:** `instance/auth.rs:983-1006` (guard); `launch.rs:457` (`spawn_sibling_auth_prewarm`, handle dropped); `launch_pipeline.rs:1318` (foreground already provisions `supported_agents()`).
- **Fix:** Don't spawn sibling-auth prewarm when the foreground already provisioned the full set. If kept, hold the `JoinHandle` and abort on cleanup. Prefer in-place rewrite (same inode) so content changes update the mounted file rather than replacing it.

### D11. `FailedSetup` containers are restore candidates

- **What:** `InstanceStatus::FailedSetup` is included in `is_restore_candidate()`. Combined with the leak paths above, a container left half-built by a failed launch gets marked `FailedSetup`, and the next launch's fast path will `AttachCurrentRole`/`StartCurrentRole` straight into it without re-validating image or setup completeness.
- **Why it matters:** Warm-reuse attaching to a *known-failed* container is wrong by construction — the operator gets a broken environment that looks like a successful fast attach.
- **Evidence:** `manifest.rs:188` (`FailedSetup` in `is_restore_candidate`); `launch.rs:2208-2227` (attach/start without setup re-validation).
- **Fix:** Exclude `FailedSetup` from the attach/start fast path (allow only `RecreateCurrentRole`), or re-validate setup completeness before attaching.

---

## Process

### P1. Unreviewable mega-PR — at least five independent concerns in one branch

- **What:** +17,779 / −2,642 across 121 files and 278 commits, bundling concerns that share no review surface:
  1. Launch fast-path + diagnostics instrumentation (the actual feature).
  2. UID-agnostic image refactor (`docker run --user` + `libnss-extrausers`, construct VERSION 0.13→0.14) — security/correctness-sensitive container change.
  3. Two new CLI commands (~3,070 lines): `cli/prewarm.rs` +1,106, `cli/diagnostics.rs` +1,964.
  4. CI caching overhaul (Swatinem/rust-cache + `.cargo/config.toml`).
  5. Root-docs restructure + "caveman-compress" (deletes `COMMITS.md`/`BRANCHING.md`, rewrites 10 root markdown files).
- **Why it matters:** None of these depend on each other for correctness. CI and docs-restructure have zero coupling to launch speed and should have merged first, independently. A change this heterogeneous can't satisfy the repo's own `PULL_REQUESTS.md` body-shape rule, and reviewers can't hold all five domains in head at once — which is exactly how B1–B4 slip through.
- **Fix:** Split into ≥4 PRs: (a) CI cache; (b) docs restructure; (c) UID refactor; (d) launch + prewarm + diagnostics. Merge (a)/(b) first.

### P2. "caveman-compress" applied to human-facing contributor docs — lossy, no backup

- **What:** Commit `66f789752` ran the `caveman-compress` skill against human-read governance docs (`CONTRIBUTING.md`, `PULL_REQUESTS.md`, `PRERELEASE.md`, etc.).
- **Why it matters:** Two concrete problems. (1) It degrades English grammar, not just verbosity: "Merge commits → **breaks** DCO" became "→ **break** DCO"; "Rebase **keeps** history clean" → "Rebase **keep** history clean"; "license **under** same terms" → "license same terms". These are onboarding docs new *human* contributors read first. Caveman compression targets AI memory files to save input tokens — a benefit that doesn't exist for prose humans read. (2) The skill mandates a human-readable `FILE.original.md` backup; **none exist** (`ls *.original.md` → no matches). Pre-compression wording is recoverable only via git archaeology, and the same PR also *deletes* `COMMITS.md`/`BRANCHING.md` — so canonical contributor guidance was relocated and stylistically degraded in one unreviewed sweep.
- **Fix:** Revert the caveman-compress of human-facing docs. If compression is desired, keep it to AI memory files and produce the mandated backups.

### P3. Branch was debugged in CI, not locally

- **What:** The commit history shows ~17 reactive "make CI green" commits rather than feature work: `fix: green the launch-speed branch CI`, `ci: re-trigger checks (dropped pull_request synchronize)`, `fix(clippy): apply workspace clippy suggestions`, two `style(...)` for `cargo fmt`, four separate `fix(spellcheck): add … to codebook wordlist`, three test-flakiness fixes, and six byte-identical `feat(runtime): record docker build step timings` commits never squashed.
- **Why it matters:** fmt/clippy/spellcheck failing on push means the local merge-readiness checks mandated by `TESTING.md`/`CONTRIBUTING.md` were skipped before pushing. The churn also makes the history hard to bisect and review commit-by-commit.
- **Fix:** Run merge-readiness (fmt, clippy, spellcheck, tests) locally before pushing. Squash the duplicate/noise commits before merge.

### P4. Roadmap entry status is a 634-word run-on sentence

- **What:** `instant-launch-architecture.mdx`'s `**Status**` block is a single ~634-word, single-sentence paragraph enumerating every shipped micro-optimization comma-separated.
- **Why it matters:** The roadmap-freshness gate is technically satisfied (status moved, deferrals listed), but the *quality* defeats the purpose — no human can extract "done vs not" from a 634-word sentence. The Evidence section (baseline runs) is solid; the top of the doc is unusable.
- **Evidence:** `docs/content/docs/reference/roadmap/instant-launch-architecture.mdx:5-6`.
- **Fix:** Rewrite Status as a short bulleted shipped/deferred list.

---

## Confirmed OK (skeptic checked, no action)

- **prewarm reuses the real launch build primitives** rather than reimplementing them — the thing most likely to be wrong (logic drift between prewarm and launch) is actually fine.
- **`.cargo/config.toml` is correctness-safe** — only `net.retry`, sparse protocol, and an alias; no `frozen`/`offline`/vendoring, so no stale-deps or masked-build-failure vector. Swatinem cache is keyed off `Cargo.lock` content, won't serve wrong deps. (One residual cost risk: `cache-all-crates` + per-job keys can evict the warm default-branch cache under GitHub's 10 GB limit → cold builds; cost/speed, not correctness.)
- **prewarm/diagnostics user docs are thorough** — all flags tabled, examples copy-pasteable, host side-effects spelled out. The new sibling-credential-laziness env behavior is documented in `environment-variables.mdx`. Construct schema versioned (VERSION 0.13→0.14) per the pre-release rule.
- **Image-logic failure bias is toward over-rebuild, not silent stale reuse** — except the decide→run window (relies on `docker run` failing closed; should be documented/tested as such).
- **`SharedCommandRunner` is a sane small abstraction** (Arc<Mutex<R>>, ~77 LoC, tight scope) — not a god-object; the only objection is that it's unused (see D4).

---

## Recommended sequencing

1. **Split** the PR (P1) — land CI cache + docs restructure separately; revert the caveman-compress of human docs (P2).
2. **Fix the four blockers** (B1–B4) with the regression tests from D8.
3. **Address cancellation + leak safety** (S1) — it's the same class as B4 and shares fixes.
4. **Bound prewarm fan-out + unify failure handling** (S3, S4, S5) before exposing the command widely.
5. **Soften diagnostics claims** (S2) until multi-run aggregation exists.
6. Design cleanups (D1–D11) can follow as the launch code is decomposed (D3).
