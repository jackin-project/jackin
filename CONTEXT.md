# jackin❯ Domain Language

Shared vocabulary for jackin❯ — agent containers, their lifecycle, and how operators start and return to them. This file is a glossary only; it carries no implementation detail.

## Lifecycle

**Instance**:
One agent container with its own durable home and identity, created by a launch. An instance can be live, exited, or un-cleanly terminated. Multiple instances of the same workspace + role + agent may exist at once.
_Avoid_: container (an instance is more than its container — it includes durable host state), session.

**Session**:
A single agent tab/pane inside a live instance, multiplexed by the in-container daemon. Many sessions share one instance.
_Avoid_: tab, pane, window (use "session" as the canonical noun; tab/pane are UI renderings of it).

**Launch**:
The operator action of picking a workspace + role + agent to begin work. A launch only ever **creates** a new instance or **restores** an un-cleanly-terminated one. A launch never reconnects to a live instance.
_Avoid_: start, run, open.

**Reconnect**:
Attaching to an already-live instance the operator explicitly selected. Reconnect is a hardline into the running container's daemon. It is reachable only by selecting a concrete instance, never from a launch pick.
_Avoid_: attach, resume (resume is the operator-facing word for the launch-side restore flow, not for reconnect), hardline (the mechanism; "reconnect" is the operator action).

**Restore**:
Bringing back an instance that was **not** cleaned up properly — crashed, killed, or interrupted. Restore reuses the same instance identity and durable home. A healthy live instance is never restored.
_Avoid_: recover, revive.

**Restore candidate**:
An instance eligible to be restored from a launch pick: un-cleanly terminated (crashed, preserved-dirty, preserved-unpushed, restore-available) or a stale index row whose container is actually gone. A healthy live instance is explicitly **not** a restore candidate.
_Avoid_: resumable instance.

## Home state

**Durable home**:
The host-backed agent home for an instance — the real, persistent agent state (config, sessions, tokens) that survives container deletion and is authoritative on restore.
_Avoid_: mounted home, agent home.

**Default-home**:
The image-baked factory home (plugins, skills, agent config, non-secret tool state) that seeds an empty durable home on first start. It never contains operator secrets.
_Avoid_: skeleton home, template home.

**First-seed**:
The one-time, atomic population of an empty durable home from the default-home plus the auth handoff. After first-seed the durable home is authoritative and is never re-seeded or merged.
_Avoid_: provisioning, initialization.

## Role environment

**Role hook**:
A script a role author ships to run at container start — `setup-once.sh` (once), `source.sh` (every start, env/PATH), `preflight.sh` (every start, validation). Hooks are the author's domain; jackin runs them faithfully and hard-fails the start on a non-zero exit, but does not police, sandbox, or rate-limit them.
_Avoid_: lifecycle script, init script.
