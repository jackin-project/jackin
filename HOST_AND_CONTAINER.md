# Host and container boundaries

Two hard rules govern where jackin' is allowed to write: it must never touch host-side state without explicit opt-in, and everything it owns inside a container lives under a single root. Both apply across schemas, design proposals, roadmap items, runtime behavior, and PR descriptions.

## Never mutate the host machine silently (hard rule)

**The operator's host machine is their property. jackin' must never write to host-side state — files, git config, repo `.git/config`, `.git/refs`, `~/.gitconfig`, `~/.config/gh/`, `~/.claude/`, `~/.codex/`, the host's git remotes, or any user repository — without an explicit, opt-in, surfaced-in-the-launch-summary action. All "smoothing" jackin' does to make a container work belongs *inside the container*.**

Examples of what this rule blocks:

- Rewriting a host repository's `origin` remote from SSH to HTTPS because "the container can't push via SSH." The fix belongs in the container's `--global` git config and credential helper, not the host repo's `.git/config`.
- Running `gh auth setup-git` on the host as part of a `jackin` command. The container can run it; the host stays untouched.
- Editing `~/.gitconfig`, `~/.ssh/config`, or any user dotfile during a launch, refresh, or "fix it for me" path. Suggest the change in the launch summary; do not apply it.
- Force-pushing, fetching, pulling, or pruning on the host's git repo as a side effect of provisioning. The only host-side git commands the CLI runs today are the ones the operator explicitly opted into (`git_pull_on_entry`, `worktree add` under `isolation = "worktree"`), and those stay scoped to the workspace's mounted repos.
- Writing the host's `~/.config/gh/hosts.yml` from the container's in-session `gh auth login`. In-container token rotation must not flow back to the host without an explicit operator-controlled bidirectional-sync opt-in (tracked under the [GitHub CLI auth strategy](docs/content/docs/reference/roadmap/github-cli-auth-strategy.mdx) follow-ups).

**Read paths against the host are fine.** `gh auth token --hostname github.com`, parsing `~/.config/gh/hosts.yml`, reading `~/.claude.json`, looking up the host's git user.email — all read-only. The forbidden direction is host-side *writes* triggered by jackin' without explicit operator opt-in.

When a design proposal or roadmap item mentions doing anything to the host, the proposal must call it out under a "Host-side effects" section, the implementing PR must surface the action in the launch summary, and the change must be opt-in (config flag, CLI flag, or operator confirmation prompt). PRs that touch the host silently must be rejected at review.

The reason: the host machine is where the operator works. Surprise mutations break their flow, surface as inexplicable bugs in terminals outside jackin', and erode trust in the orchestrator. The whole point of jackin' is to absorb the messiness inside containers so the host stays clean.

The host root for jackin-owned paths is `~/.jackin/`, with its own subdirectory layout (`~/.jackin/{data,cache,sockets,roles,run}/`).

## Container path convention: everything jackin' owns lives under `/jackin/` (hard rule)

**Every path jackin' creates, mounts, or owns inside a role container must live under `/jackin/`.** No FHS-borrowed top-level directories (`/run/jackin/`, `/var/lib/jackin/`, `/opt/jackin/`, `/etc/jackin/`), no scattered locations the operator has to discover one-by-one. An operator who runs `ls /jackin/` inside any role container must see the complete map of jackin-owned state in one place.

Concrete layout (current and going forward):

- `/jackin/runtime/` — entrypoint script, hooks, agent-launch scaffolding (read-only image content).
- `/jackin/state/` — runtime markers (`hooks/setup-once.done`, etc.) written during first-boot.
- `/jackin/default-home/` — image-baked default home contents copied into `/home/agent/` on first boot.
- `/jackin/run/` — runtime sockets, pidfiles, and other ephemeral runtime state. The jackin-capsule daemon socket lives at `/jackin/run/jackin.sock`.
- `/jackin/{claude,codex,amp,kimi,opencode}/` — agent credential mounts.
- `/jackin/host/` — read-only views of host paths exposed into the container.

Examples of what this rule blocks:

- New container paths under `/run/`, `/var/`, `/opt/`, `/srv/`, `/etc/`, or any other FHS root — even when they "feel natural" for the asset type (a Unix socket under `/run/` is the most common drift). The container is a single-purpose jackin runtime; the FHS layout is not what makes the in-container experience legible.
- Per-container scratch paths under `/tmp/jackin*` or `/var/run/jackin*`. If it's jackin-owned and ephemeral, it goes under `/jackin/run/`.
- Hard-coded paths in role-specific scripts that bypass the convention because "this is just for one role." Roles author their own files under `/home/agent/` or in the workspace; jackin-owned content stays under `/jackin/`.

**Host-side state is a separate convention.** The container and host roots are deliberately parallel but not identical — the host follows operator-home dotfile customs (`~/.jackin/`), the container follows the single-root `/jackin/` convention. A bind-mount that maps `~/.jackin/sockets/<container>/` to `/jackin/run/` is the canonical shape.

When you find yourself wanting to introduce a new container-side path, place it under `/jackin/` first, then justify in the PR description if a real constraint forces an exception (e.g. a third-party tool that hard-codes `/run/<thing>` and cannot be relocated). PRs that introduce a top-level jackin-owned path outside `/jackin/` without an exception note must be rejected at review and the path moved.

The reason: a flat, single-root convention makes the in-container surface debuggable. An operator who wants to know "what does jackin do to my container?" can `ls /jackin/` and see the answer. The rule also makes future cleanup straightforward — `rm -rf /jackin` removes every jackin-owned artifact, leaving the base image intact for whatever rebuild the operator wants next.
