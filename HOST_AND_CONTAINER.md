# Host and container boundaries

Two hard rules govern where jackin❯ may write: never touch host-side state without explicit opt-in, and everything it owns inside a container lives under single root. Apply across schemas, design proposals, roadmap items, runtime behavior, PR descriptions.

## Never mutate the host machine silently (hard rule)

**Operator host machine is their property. jackin❯ must never write host-side state — files, git config, repo `.git/config`, `.git/refs`, `~/.gitconfig`, `~/.config/gh/`, `~/.claude/`, `~/.codex/`, host git remotes, or any user repository — without explicit, opt-in, surfaced-in-launch-summary action. All "smoothing" jackin❯ does to make container work belongs *inside container*.**

What rule blocks:

- Rewriting host repo `origin` remote SSH→HTTPS because "container can't push via SSH." Fix belongs in container `--global` git config and credential helper, not host repo `.git/config`.
- Running `gh auth setup-git` on host as part of `jackin` command. Container can run it; host stays untouched.
- Editing `~/.gitconfig`, `~/.ssh/config`, or any user dotfile during launch, refresh, or "fix it for me" path. Suggest in launch summary; do not apply.
- Force-pushing, fetching, pulling, pruning host git repo as provisioning side effect. Only host-side git commands CLI runs today are operator-opted-in (`git_pull_on_entry`, `worktree add` under `isolation = "worktree"`), scoped to workspace mounted repos.
- Writing host `~/.config/gh/hosts.yml` from container in-session `gh auth login`. In-container token rotation must not flow back to host without explicit operator-controlled bidirectional-sync opt-in (tracked under [GitHub CLI auth strategy](<docs/content/roadmap/(agent-runtimes-authentication)/github-cli-auth-strategy.mdx>) follow-ups).

**Read paths against host fine.** `gh auth token --hostname github.com`, parsing `~/.config/gh/hosts.yml`, reading `~/.claude.json`, looking up host git user.email — all read-only. Forbidden direction: host-side *writes* triggered by jackin❯ without explicit operator opt-in.

When design proposal or roadmap item mentions doing anything to host, must call out under "Host-side effects" section, implementing PR must surface action in launch summary, change must be opt-in (config flag, CLI flag, or operator confirmation prompt). PRs touching host silently rejected at review.

Reason: host machine is where operator works. Surprise mutations break flow, surface as inexplicable bugs in terminals outside jackin❯, erode trust. jackin❯ absorbs messiness inside containers so host stays clean.

Host root for jackin-owned paths is `~/.jackin/`, with own subdirectory layout (`~/.jackin/{data,cache,sockets,roles,run}/`).

## Credential transport

Forwarded credential values cross the runtime boundary through a temporary host-only env file with mode `0600`; only non-sensitive `JACKIN_*` launch metadata may appear in process arguments. On-demand literal values remain in the host-side allowlist, while the container-visible config carries only a fixed redacted marker. The runtime removes the env file immediately after the container launch command returns. Secrets must never appear in runtime argv or container-visible config.

Rotate any credential previously configured as an on-demand literal. Older launches exposed that value through the container filesystem even when exec approval was still required.

## Runtime backend mount enforcement

Both supported backends enforce read-only mounts. Docker receives its existing `:ro` bind options; the apple-container backend emits the same option through Apple `container` v0.11.0 or newer for directory mounts. Apple `container` rejects the single-file bind mounts required by worktree isolation, so that combination fails closed before cleanup or launch; use Docker or shared isolation. A backend must reject a launch instead of silently dropping an operator-selected mount restriction.

## Usage broker boundary

One host-only Rust usage broker owns account discovery, provider calls, refresh
generations, atomic cached results, cooldowns, retry deadlines, and failure counts.
All active Desktop/Capsule requests for one canonical account join the same
generation; manual force never queues a second generation behind active work. A wait
timeout does not release ownership until bounded provider work terminates. Broker,
state, protocol, or authorization failure is fail-closed and performs zero provider
calls.

Broker directories are host-only `0700`; sockets/state files are `0600`. Each account
state update is a same-directory no-follow, fsync, atomic-rename envelope containing
the generation, terminal/last-good projection, deadlines, and consecutive failure
count. Provider-supplied `Retry-After` is preserved; otherwise one persisted local
backoff is shared across processes.

The global broker socket, account catalog, credential sources, and state tree are
never mounted into a container. Each Capsule receives a local relay at
`/jackin/run/usage.sock`; the relay's immutable allowlist is derived from credential
capabilities actually forwarded at launch. Credentials created only inside a Capsule
are not monitored until a separate secure-enrollment design exists. The former
writable `/jackin/usage-shared` snapshot/cooldown/lock tree is removed and is not a
fallback.

Docker bind-mounts the per-container host socket directory at `/jackin/run` and
starts the usage relay through a guest-local stdio proxy. The proxy treats the
Docker Capsule daemon as `(pid=1, uid=0, gid=0)` unless launch metadata overrides
`JACKIN_CAPSULE_SUPERVISOR_PID`. Apple Container does not preserve guest peer
credentials across a host Unix-socket mount, so it file-mounts launch config (and
`host.sock` when on-demand credentials exist) and uses the same guest-local proxy
over a host-started stdio tunnel. Apple launch metadata identifies the Capsule
supervisor as `(pid=2, uid=0, gid=0)` under `vminitd` PID 1 (entrypoint-after-init
contract, not a runtime probe). Other root peers are rejected before the allowlist
is consulted.

The Docker relay proxy does not treat container root as the supervisor: launch-wide
capabilities require the kernel peer tuple `(pid=1, uid=0, gid=0)`, which is the
Capsule daemon. Session peers remain bound to their exact configured `(uid, gid)`.

## Container path convention: everything jackin❯ owns lives under `/jackin/` (hard rule)

**Every path jackin❯ creates, mounts, or owns inside role container must live under `/jackin/`.** No FHS-borrowed top-level directories (`/run/jackin/`, `/var/lib/jackin/`, `/opt/jackin/`, `/etc/jackin/`), no scattered locations to discover one-by-one. Operator running `ls /jackin/` inside any role container must see complete map of jackin-owned state in one place.

Layout (current and going forward):

- `/jackin/runtime/` — entrypoint script, hooks, agent-launch scaffolding (read-only image content).
- `/jackin/state/` — runtime markers (`hooks/setup-once.done`, etc.) written during first-boot.
- `/jackin/default-home/` — image-baked default home contents copied into `/home/agent/` on first boot.
- `/jackin/run/` — runtime sockets, pidfiles, other ephemeral runtime state. The Capsule daemon socket is `/jackin/run/jackin.sock`; the capability-scoped usage relay is `/jackin/run/usage.sock`.
- `/jackin/{amp,claude,codex,grok,kimi-code,opencode}/` — agent credential mounts.
- `/jackin/host/` — read-only views of host paths exposed into container.

What rule blocks:

- New container paths under `/run/`, `/var/`, `/opt/`, `/srv/`, `/etc/`, or any other FHS root — even when "natural" for asset type (Unix socket under `/run/` is most common drift). Container is single-purpose jackin❯ runtime; FHS layout not what makes in-container experience legible.
- Per-container scratch paths under `/tmp/jackin*` or `/var/run/jackin*`. jackin-owned and ephemeral goes under `/jackin/run/`.
- Hard-coded paths in role-specific scripts bypassing convention because "just for one role." Roles author files under `/home/agent/` or in workspace; jackin-owned content stays under `/jackin/`.

**Host-side state is separate convention.** Container and host roots deliberately parallel but not identical — host follows operator-home dotfile customs (`~/.jackin/`), container follows single-root `/jackin/` convention. Docker bind-mounts `~/.jackin/sockets/<container>/` to `/jackin/run/`. Apple Container file-mounts only the launch config and any host sockets that must exist before `container run`.

Wanting new container-side path — place under `/jackin/` first, then justify in PR description if real constraint forces exception (e.g. third-party tool hard-coding `/run/<thing>` that can't be relocated). PRs introducing top-level jackin-owned path outside `/jackin/` without exception note rejected at review and path moved.

Reason: flat single-root convention makes in-container surface debuggable. "What does jackin❯ do to my container?" → `ls /jackin/`. Also makes cleanup straightforward — `rm -rf /jackin` removes every jackin-owned artifact, leaving base image intact for whatever rebuild operator wants.
