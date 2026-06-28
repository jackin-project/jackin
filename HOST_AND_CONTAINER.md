# Host and container boundaries

Two hard rules govern where jackin❯ may write: never touch host-side state without explicit opt-in, and everything it owns inside a container lives under single root. Apply across schemas, design proposals, roadmap items, runtime behavior, PR descriptions.

## Never mutate the host machine silently (hard rule)

**Operator host machine is their property. jackin❯ must never write host-side state — files, git config, repo `.git/config`, `.git/refs`, `~/.gitconfig`, `~/.config/gh/`, `~/.claude/`, `~/.codex/`, host git remotes, or any user repository — without explicit, opt-in, surfaced-in-launch-summary action. All "smoothing" jackin❯ does to make container work belongs *inside container*.**

What rule blocks:

- Rewriting host repo `origin` remote SSH→HTTPS because "container can't push via SSH." Fix belongs in container `--global` git config and credential helper, not host repo `.git/config`.
- Running `gh auth setup-git` on host as part of `jackin` command. Container can run it; host stays untouched.
- Editing `~/.gitconfig`, `~/.ssh/config`, or any user dotfile during launch, refresh, or "fix it for me" path. Suggest in launch summary; do not apply.
- Force-pushing, fetching, pulling, pruning host git repo as provisioning side effect. Only host-side git commands CLI runs today are operator-opted-in (`git_pull_on_entry`, `worktree add` under `isolation = "worktree"`), scoped to workspace mounted repos.
- Writing host `~/.config/gh/hosts.yml` from container in-session `gh auth login`. In-container token rotation must not flow back to host without explicit operator-controlled bidirectional-sync opt-in (tracked under [GitHub CLI auth strategy](docs/content/docs/reference/roadmap/github-cli-auth-strategy.mdx) follow-ups).

**Read paths against host fine.** `gh auth token --hostname github.com`, parsing `~/.config/gh/hosts.yml`, reading `~/.claude.json`, looking up host git user.email — all read-only. Forbidden direction: host-side *writes* triggered by jackin❯ without explicit operator opt-in.

When design proposal or roadmap item mentions doing anything to host, must call out under "Host-side effects" section, implementing PR must surface action in launch summary, change must be opt-in (config flag, CLI flag, or operator confirmation prompt). PRs touching host silently rejected at review.

Reason: host machine is where operator works. Surprise mutations break flow, surface as inexplicable bugs in terminals outside jackin❯, erode trust. jackin❯ absorbs messiness inside containers so host stays clean.

Host root for jackin-owned paths is `~/.jackin/`, with own subdirectory layout (`~/.jackin/{data,cache,sockets,roles,run}/`).

## Container path convention: everything jackin❯ owns lives under `/jackin/` (hard rule)

**Every path jackin❯ creates, mounts, or owns inside role container must live under `/jackin/`.** No FHS-borrowed top-level directories (`/run/jackin/`, `/var/lib/jackin/`, `/opt/jackin/`, `/etc/jackin/`), no scattered locations to discover one-by-one. Operator running `ls /jackin/` inside any role container must see complete map of jackin-owned state in one place.

Layout (current and going forward):

- `/jackin/runtime/` — entrypoint script, hooks, agent-launch scaffolding (read-only image content).
- `/jackin/state/` — runtime markers (`hooks/setup-once.done`, etc.) written during first-boot.
- `/jackin/default-home/` — image-baked default home contents copied into `/home/agent/` on first boot.
- `/jackin/run/` — runtime sockets, pidfiles, other ephemeral runtime state. jackin-capsule daemon socket lives at `/jackin/run/jackin.sock`.
- `/jackin/{claude,codex,amp,kimi,opencode}/` — agent credential mounts.
- `/jackin/host/` — read-only views of host paths exposed into container.

What rule blocks:

- New container paths under `/run/`, `/var/`, `/opt/`, `/srv/`, `/etc/`, or any other FHS root — even when "natural" for asset type (Unix socket under `/run/` is most common drift). Container is single-purpose jackin runtime; FHS layout not what makes in-container experience legible.
- Per-container scratch paths under `/tmp/jackin*` or `/var/run/jackin*`. jackin-owned and ephemeral goes under `/jackin/run/`.
- Hard-coded paths in role-specific scripts bypassing convention because "just for one role." Roles author files under `/home/agent/` or in workspace; jackin-owned content stays under `/jackin/`.

**Host-side state is separate convention.** Container and host roots deliberately parallel but not identical — host follows operator-home dotfile customs (`~/.jackin/`), container follows single-root `/jackin/` convention. Bind-mount mapping `~/.jackin/sockets/<container>/` to `/jackin/run/` is canonical shape.

Wanting new container-side path — place under `/jackin/` first, then justify in PR description if real constraint forces exception (e.g. third-party tool hard-coding `/run/<thing>` that can't be relocated). PRs introducing top-level jackin-owned path outside `/jackin/` without exception note rejected at review and path moved.

Reason: flat single-root convention makes in-container surface debuggable. "What does jackin do to my container?" → `ls /jackin/`. Also makes cleanup straightforward — `rm -rf /jackin` removes every jackin-owned artifact, leaving base image intact for whatever rebuild operator wants.